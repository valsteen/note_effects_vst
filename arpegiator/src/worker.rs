use log::{error, info};
use util::pattern_payload::PatternPayload;
use std::thread::JoinHandle;
use std::thread;
use smol::channel::{unbounded, Sender, Receiver, RecvError};
use smol::net::UdpSocket;
use crate::midi_controller_worker::{midi_controller_worker, ControllerCommand};
use smol::io::Error;
use smol::Task;
use smol::future::FutureExt;


#[derive(Debug)]
pub enum WorkerCommand {
    Stop,
    SetPort(u16),
    SendToController(ControllerCommand)
}

pub struct WorkerChannels {
    pub command_sender: Sender<WorkerCommand>,
    pub notes_receiver: Receiver<PatternPayload>,
    pub worker: JoinHandle<()>,
}

enum WorkerResult {
    Command(WorkerCommand),
    PayloadError(bincode::Error),
    ChannelError(RecvError),
    SocketError(Error),
    SocketStopped,
    MidiControllerStopped,
}

enum SocketResult {
    Recv(usize),
    Stop,
    Error(Error)
}

async fn spawn_socket_worker(port: u16, notes_sender: Sender<PatternPayload>, socket_stop_channel: Receiver<bool>) ->
                                                                                                        WorkerResult {
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
        let socket_receive = async {
            match socket.recv(&mut buf).await {
                Ok(len) => SocketResult::Recv(len),
                Err(err) => SocketResult::Error(err)
            }
        };

        let stop_receive = async {
            match socket_stop_channel.recv().await {
                Ok(_) => SocketResult::Stop,
                Err(_) => SocketResult::Stop
            }
        };

        match socket_receive.race(stop_receive).await {
            SocketResult::Recv(len) => {
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
            SocketResult::Stop => {
                info!("Quitting socket port {}", port);
                return WorkerResult::SocketStopped
            },
            SocketResult::Error(err) => {
                info!("Error {} : quitting socket port {}", err, port);
                return WorkerResult::SocketError(err)
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
        Ok(command) => {
            info!("Received command {:?}", command);
            WorkerResult::Command(command)
        },
        Err(err) => {
            error!("Error while reading command channel: {}", err);
            WorkerResult::ChannelError(err)
        }
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


struct MidiControllerChannels {
    sender: Sender<ControllerCommand>,
    receiver: Receiver<ControllerCommand>,
}

struct SocketStopChannels {
    sender: Sender<bool>,
    receiver: Receiver<bool>,
}


pub fn create_worker_thread() -> WorkerChannels {
    let (command_sender, command_receiver) = unbounded::<WorkerCommand>();
    let (worker_result_sender, worker_result_receiver) = unbounded();
    let (worker_task_sender, worker_task_receiver) = unbounded();
    let (notes_sender, notes_receiver) = unbounded::<PatternPayload>();

    let main = {
        async move {
            let (sender, receiver) = unbounded::<ControllerCommand>();
            let mut midi_controller_channels = MidiControllerChannels {
                sender,
                receiver,
            };

            let (sender, receiver) = unbounded::<bool>();
            let mut socket_stop_channels = SocketStopChannels {
                sender,
                receiver
            } ;

            let mut socket_worker_task: Option<Task<()>> = None;
            let mut midi_controller_task: Option<Task<()>> = None;
            let mut command_receiver_task : Option<Task<()>> = None;

            loop {
                if command_receiver_task.is_none() {
                    info!("spawning command receiver");
                    command_receiver_task = {
                        let command_receiver = command_receiver.clone();
                        let worker_result_sender = worker_result_sender.clone();
                        Some(schedule!(command_reader(command_receiver), worker_task_sender, worker_result_sender))
                    };
                }

                let runnable = worker_task_receiver.try_recv().unwrap();
                info!("Executing runnable {:?}", runnable);
                runnable.run();

                let worker_result = worker_result_receiver.recv().await.unwrap();

                match worker_result {
                    WorkerResult::Command(command) => {
                        command_receiver_task = None;

                        match command {
                            WorkerCommand::Stop => {
                                if let Some(socket_worker_task) = socket_worker_task {
                                    socket_stop_channels.sender.send(true).await.unwrap();
                                    socket_worker_task.cancel().await;
                                }

                                if let Some(midi_controller_task) = midi_controller_task {
                                    midi_controller_channels.sender.send(ControllerCommand::Stop).await.unwrap();
                                    midi_controller_task.cancel().await;
                                }
                                return;
                            }
                            WorkerCommand::SetPort(port) => {
                                info!("Switching to port {}" , port);
                                if let Some(socket_worker_task) = socket_worker_task {
                                    socket_stop_channels.sender.send(true).await.unwrap();
                                    socket_worker_task.cancel().await;
                                }

                                let (sender, receiver) = unbounded::<bool>();
                                socket_stop_channels.sender = sender;
                                socket_stop_channels.receiver = receiver;

                                info!("socket worker stopped");
                                socket_worker_task = {
                                    let notes_sender = notes_sender.clone();
                                    let socket_stop_receiver = socket_stop_channels.receiver.clone();
                                    Some(schedule!(spawn_socket_worker(port, notes_sender, socket_stop_receiver),
                                    worker_task_sender, worker_result_sender))
                                };

                                info!("stopping controller worker");
                                if let Some(midi_controller_task) = midi_controller_task {
                                    midi_controller_channels.sender.send(ControllerCommand::Stop).await.unwrap();
                                    midi_controller_task.cancel().await;
                                }
                                info!("controller worker stopped");
                                let (sender, receiver) = unbounded::<ControllerCommand>();
                                midi_controller_channels.sender = sender ;
                                midi_controller_channels.receiver = receiver;

                                midi_controller_task = {
                                    let midi_controller_receiver = midi_controller_channels.receiver.clone();
                                    Some(
                                        schedule!(
                                        spawn_controller_worker(format!("Arpegiator {}", port), midi_controller_receiver),
                                        worker_task_sender, worker_result_sender)
                                    )
                                };
                            }
                            WorkerCommand::SendToController(controller_command) => {
                                midi_controller_channels.sender.send(controller_command).await.unwrap();
                            }
                        }
                    }
                    WorkerResult::PayloadError(err) => {
                        error!("Invalid payload received. Data received from wrong service ? ( {} )", err);
                        socket_worker_task = None
                    }
                    WorkerResult::SocketError(_) => {
                        // don't respawn, wait until user chooses another port
                    }
                    WorkerResult::ChannelError(err) => {
                        error!("Command channel error, quitting worker ({})", err);

                        if let Some(socket_worker_task) = socket_worker_task {
                            socket_worker_task.cancel().await;
                        }

                        if let Some(midi_controller_task) = midi_controller_task {
                            midi_controller_channels.sender.send(ControllerCommand::Stop).await.unwrap();
                            midi_controller_task.cancel().await;
                        }
                        return;
                    }

                    WorkerResult::MidiControllerStopped => {
                        // controller worker quit. Incompatibility or resource already taken, wait until the user
                        // chooses another port
                    }
                    WorkerResult::SocketStopped => {
                    }
                }
            }
        }
    };

    let handle = thread::spawn(move || smol::block_on(main));

    WorkerChannels {
        command_sender,
        notes_receiver,
        worker: handle,
    }
}
