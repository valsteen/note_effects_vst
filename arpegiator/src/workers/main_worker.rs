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
use util::system::Uuid;


#[derive(Debug)]
pub(crate) enum WorkerCommand {
    Stop(Uuid),
    SetPort(u16, Uuid),
    SetSampleRate(f32),
    SetBlockSize(i64),
    SendToMidiOutput { buffer_start_time: u64, messages: Vec<MidiMessageWithDelta> },
    IPCWorkerStopped(Uuid, u16),
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
        #[cfg(feature = "worker_debug")] info!("starting workers");

        async move {
            let mut current_port : Option<u16> = None;
            let mut midi_out_worker_sender : Option<Sender<MidiOutputWorkerCommand>> = None;
            let mut ipc_worker_sender : Option<Sender<IPCWorkerCommand>> = None;
            let mut exit_event_id = Uuid::default();
            let mut exit_reason = "";

            while let Ok(command) = command_receiver.recv().await {
                match command {
                    WorkerCommand::SetPort(port, event_id) => {
                        if current_port.is_some() && current_port.unwrap() == port {
                            continue;
                        }
                        info!("[{}] Switching to port {}", event_id, port);
                        current_port = Some(port);

                        close_workers(&mut midi_out_worker_sender, &mut ipc_worker_sender, event_id).await;

                        {
                            let pattern_sender = pattern_sender.clone();
                            let command_sender = command_sender.clone();

                            match spawn_ipc_worker(port, pattern_sender, command_sender) {
                                Ok(sender) => {
                                    ipc_worker_sender = Some(sender);
                                }
                                Err(err) => {
                                    exit_event_id = event_id;
                                    exit_reason = "Cannot start ipc worker";

                                    #[cfg(feature = "worker_debug")]
                                    error!("[{}] Cannot start ipc worker: {}", exit_event_id, err);
                                    break;
                                }
                            }
                        }

                        match spawn_midi_output_worker(format!("Arpegiator {}", port)) {
                            Ok(sender) => {
                                midi_out_worker_sender = Some(sender);
                            }
                            Err(_) => {
                                exit_event_id = event_id;
                                exit_reason = "Could not spawn midi output worker";

                                #[cfg(feature = "worker_debug")]
                                error!("[{}] Could not spawn midi output worker", exit_event_id);
                                close_workers(&mut midi_out_worker_sender, &mut ipc_worker_sender, exit_event_id).await;
                                break;
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
                    WorkerCommand::IPCWorkerStopped(event_id, ipc_worker_port) => {
                        if current_port.is_none() || current_port.unwrap() != ipc_worker_port {
                            continue;
                        }
                        exit_reason = "IPC worker stopped";
                        exit_event_id = event_id;
                        close_workers(&mut midi_out_worker_sender, &mut ipc_worker_sender, event_id).await;
                        break;
                    }
                    WorkerCommand::Stop(event_id) => {
                        exit_reason = "Stop received";
                        exit_event_id = event_id;
                        close_workers(&mut midi_out_worker_sender, &mut ipc_worker_sender, event_id).await;
                        break;
                    }
                }
            }

            #[cfg(feature = "worker_debug")]
            info!("[{}] exiting main worker: {}", exit_event_id, exit_reason)
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
    ipc_worker_sender: &mut Option<Sender<IPCWorkerCommand>>,
    event_id: Uuid
) {
    if let Some(ipc_worker_sender) = take(ipc_worker_sender) {
        let (ack_sender, ack_receiver) = async_channel::bounded(1);
        ipc_worker_sender.send(IPCWorkerCommand::Stop(ack_sender, event_id)).await.unwrap_or_else(|err| {
            error!("[{}] Could not contact worker sender for shutdown : {}", event_id, err);
        });
        match ack_receiver.recv().await {
            Ok(_) => { error!("[{}] ipc worker exit ack", event_id) }
            Err(err) => { error!("[{}] ipc worker did not ack exit: {}", event_id, err) }
        }
    };

    if let Some(midi_out_worker_sender) = take(midi_out_worker_sender) {
        let (ack_sender, ack_receiver) = async_channel::bounded(1);
        midi_out_worker_sender.send(MidiOutputWorkerCommand::Stop(ack_sender, event_id)).await.unwrap_or_else(|err| {
            error!("[{}] Could not contact midi output worker for shutdown : {}", event_id, err);
        });
        match ack_receiver.recv().await {
            Ok(_) => { info!("[{}] midi out worker exit ack", event_id) }
            Err(err) => { error!("[{}] midi out did not ack exit: {}", event_id, err) }
        }
    };
}
