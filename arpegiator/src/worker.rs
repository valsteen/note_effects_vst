use log::{error, info};
use util::pattern_payload::PatternPayload;
use std::thread::JoinHandle;
use std::thread;
use smol::channel::{unbounded, Sender, Receiver, RecvError};
use smol::net::UdpSocket;
use crate::midi_controller_worker::{midi_controller_worker, ControllerCommand};
use smol::io::Error;
use smol::Task;


pub enum WorkerCommand {
    Stop,
    SetPort(u16),
}

pub struct WorkerChannels {
    pub command_sender: Sender<WorkerCommand>,
    pub midi_controller_sender: Sender<ControllerCommand>,
    pub notes_receiver: Receiver<PatternPayload>,
    pub worker: JoinHandle<()>,
}

enum WorkerResult {
    Command(WorkerCommand),
    PayloadError(bincode::Error),
    ChannelError(RecvError),
    SocketError(Error),
    MidiControllerStopped,
}

async fn spawn_socket_worker(port: u16, notes_sender: Sender<PatternPayload>) -> WorkerResult {
    let mut buf = vec![0u8; 1024];

    let socket = match UdpSocket::bind(format!("127.0.0.1:{}", port)).await {
        Ok(socket) => {
            info!("Listening on port {}", port);
            socket
        }
        Err(err) => {
            error!("Cannot bind on port {} : {:?}", port, err);
            return WorkerResult::SocketError(err);
        }
    };

    loop {
        match socket.recv(&mut buf).await {
            Ok(len) => {
                match bincode::deserialize::<PatternPayload>(&buf[..len]) {
                    Ok(payload) => {
                        notes_sender.send(payload).await.unwrap();
                    }
                    Err(err) => {
                        error!("Could not deserialize: {:?}", err);
                        return WorkerResult::PayloadError(err);
                    }
                }
            }
            Err(err) => {
                error!("Socket error: {:?}", err);
                return WorkerResult::SocketError(err);
            }
        }
    }
}


async fn spawn_controller_worker(name: String, control_channel: Receiver<ControllerCommand>) -> WorkerResult {
    midi_controller_worker(name, control_channel).await;
    WorkerResult::MidiControllerStopped
}

async fn command_reader(command_receiver: Receiver<WorkerCommand>) -> WorkerResult {
    match command_receiver.recv().await {
        Ok(command) => WorkerResult::Command(command),
        Err(err) => WorkerResult::ChannelError(err)
    }
}


macro_rules! schedule {
    ($future:expr, $task_queue:expr, $result_queue:expr) => {{
        let result_queue = $result_queue.clone();
        let task_queue = $task_queue.clone();
        let future = async move {
            result_queue.send($future.await).await.unwrap();
        };
        let (runnable, task) = async_task::spawn_local(future,
            move |runnable| task_queue.try_send(runnable).unwrap()
        );
        runnable.schedule();
        task
    }}
}

pub fn create_worker_thread() -> WorkerChannels {
    let (command_sender, command_receiver) = unbounded::<WorkerCommand>();
    let (notes_sender, notes_receiver) = unbounded::<PatternPayload>();
    let (midi_controller_sender, midi_controller_receiver) = unbounded::<ControllerCommand>();
    let (worker_result_sender, worker_result_receiver) = unbounded();
    let (worker_task_sender, worker_task_receiver) = unbounded();

    {
        let command_receiver = command_receiver.clone();
        let task = schedule!(command_reader(command_receiver), worker_task_sender, worker_result_sender);
        task.detach();
    }

    let main = {
        let midi_controller_sender = midi_controller_sender.clone();

        async move {
            let mut socket_worker_task: Option<Task<()>> = None;
            let mut midi_controller_task: Option<Task<()>> = None;

            loop {
                let runnable = worker_task_receiver.try_recv().unwrap();
                runnable.run();

                let worker_result = worker_result_receiver.recv().await.unwrap();

                match worker_result {
                    WorkerResult::Command(command) => {
                        match command {
                            WorkerCommand::Stop => {
                                return;
                            }
                            WorkerCommand::SetPort(port) => {
                                if let Some(socket_worker_task) = socket_worker_task {
                                    socket_worker_task.cancel().await;
                                }
                                socket_worker_task = {
                                    let notes_sender = notes_sender.clone();
                                    Some(schedule!(spawn_socket_worker(port, notes_sender), worker_task_sender,
                                    worker_result_sender))
                                };

                                if let Some(midi_controller_task) = midi_controller_task {
                                    midi_controller_sender.send(ControllerCommand::Stop).await.unwrap();
                                    midi_controller_task.cancel().await;
                                }

                                midi_controller_task = {
                                    let midi_controller_receiver = midi_controller_receiver.clone();
                                    Some(
                                        schedule!(
                                        spawn_controller_worker(format!("Arpegiator {}", port), midi_controller_receiver),
                                        worker_task_sender, worker_result_sender)
                                    )
                                };

                                {
                                    let command_receiver = command_receiver.clone();
                                    let task = schedule!(command_reader(command_receiver), worker_task_sender, worker_result_sender);
                                    task.detach();
                                }
                            }
                        }
                    }
                    WorkerResult::PayloadError(err) => {
                        error!("Invalid payload received. Data received from wrong service ? ( {} )", err);
                        socket_worker_task = None
                    }
                    WorkerResult::SocketError(_) => {
                        // don't respawn, wait until user chooses another port
                        socket_worker_task = None
                    }
                    WorkerResult::ChannelError(err) => {
                        error!("Command channel error, quitting worker ({})", err);

                        if let Some(socket_worker_task) = socket_worker_task {
                            socket_worker_task.cancel().await;
                        }

                        if let Some(midi_controller_task) = midi_controller_task {
                            midi_controller_sender.send(ControllerCommand::Stop).await.unwrap();
                            midi_controller_task.cancel().await;
                        }
                        return;
                    }

                    WorkerResult::MidiControllerStopped => {
                        // controller worker quit. Incompatibility or resource already taken, wait until the user
                        // chooses another port
                        midi_controller_task = None
                    }
                }
            }
        }
    };

    let handle = thread::spawn(move || smol::block_on(main));

    WorkerChannels {
        command_sender,
        notes_receiver,
        midi_controller_sender,
        worker: handle,
    }
}
