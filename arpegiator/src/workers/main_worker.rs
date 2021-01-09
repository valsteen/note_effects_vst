use std::thread;
use std::thread::JoinHandle;

use async_channel::{Receiver, Sender, unbounded};
use async_std::task;
use log::{error, info};

use util::midi_message_with_delta::MidiMessageWithDelta;
use util::ipc_payload::PatternPayload;

use crate::workers::ipc_worker::{spawn_ipc_worker, IPCWorkerCommand};
use crate::workers::midi_output_worker::{MidiOutputWorkerCommand, spawn_midi_output_worker};
use std::mem::take;


#[derive(Debug)]
pub(crate) enum WorkerCommand {
    Stop,
    SetPort(u16),
    SetSampleRate(f32),
    SetBlockSize(i64),
    SendToMidiOutput { buffer_start_time: u64, messages: Vec<MidiMessageWithDelta> },
    IPCWorkerStopped,
}

pub(crate) struct WorkerChannels {
    pub command_sender: Sender<WorkerCommand>,
    pub pattern_receiver: Receiver<PatternPayload>,
    pub worker: JoinHandle<()>,
}


pub(crate) fn create_worker_thread() -> WorkerChannels {
    let (command_sender, command_receiver) = unbounded::<WorkerCommand>();
    let (pattern_sender, pattern_receiver) = unbounded::<PatternPayload>();

    let returned_command_sender = command_sender.clone();

    let main = {
        async move {
            let mut midi_out_worker_sender : Option<Sender<MidiOutputWorkerCommand>> = None;
            let mut ipc_worker_sender : Option<Sender<IPCWorkerCommand>> = None;

            while let Ok(command) = command_receiver.recv().await {
                match command {
                    WorkerCommand::SetPort(port) => {
                        info!("Switching to port {}", port);

                        close_workers(&mut midi_out_worker_sender, &mut ipc_worker_sender).await;

                        {
                            let pattern_sender = pattern_sender.clone();
                            let command_sender = command_sender.clone();

                            match spawn_ipc_worker(port, pattern_sender, command_sender) {
                                Ok(sender) => {
                                    ipc_worker_sender = Some(sender);
                                }
                                Err(err) => {
                                    error!("Cannot start ipc worker: {}", err);
                                    continue;
                                }
                            }
                        }

                        match spawn_midi_output_worker(format!("Arpegiator {}", port)) {
                            Ok(sender) => {
                                midi_out_worker_sender = Some(sender);
                            }
                            Err(_) => {
                                error!("Could not spawn midi output worker");
                                close_workers(&mut midi_out_worker_sender, &mut ipc_worker_sender).await;
                            }
                        }
                    }

                    WorkerCommand::SendToMidiOutput { buffer_start_time, messages } => {
                        if let Some(midi_out_worker_sender) = midi_out_worker_sender.as_ref() {
                            midi_out_worker_sender.send(
                                MidiOutputWorkerCommand::SendToController { buffer_start_time, messages }
                            ).await.unwrap();
                        }
                    }
                    WorkerCommand::SetSampleRate(rate) => {
                        if let Some(midi_out_worker_sender) = midi_out_worker_sender.as_ref() {
                            midi_out_worker_sender.send(MidiOutputWorkerCommand::SetSampleRate(rate)).await.unwrap();
                        }
                    }
                    WorkerCommand::SetBlockSize(_size) => {
                        // not used
                    }
                    WorkerCommand::IPCWorkerStopped => {
                        close_workers(&mut midi_out_worker_sender, &mut ipc_worker_sender).await;
                    }
                    WorkerCommand::Stop => {
                        close_workers(&mut midi_out_worker_sender, &mut ipc_worker_sender).await;
                    }
                }
            }
        }
    };

    let worker = thread::spawn(move || task::block_on(main));

    WorkerChannels {
        command_sender: returned_command_sender,
        pattern_receiver,
        worker
    }
}

async fn close_workers(
    midi_out_worker_sender: &mut Option<Sender<MidiOutputWorkerCommand>>,
    ipc_worker_sender: &mut Option<Sender<IPCWorkerCommand>>) {
    if let Some(ipc_worker_sender) = take(ipc_worker_sender) {
        ipc_worker_sender.send(IPCWorkerCommand::Stop).await.unwrap_or_else(|err| {
            error!("Could not contact worker sender for shutdown : {}", err);
        })
    };

    if let Some(midi_out_worker_sender) = take(midi_out_worker_sender) {
        midi_out_worker_sender.send(MidiOutputWorkerCommand::Stop).await.unwrap_or_else(|err| {
            error!("Could not contact midi output worker for shutdown : {}", err);
        })
    };
}
