use log::{error, info};
use util::pattern_payload::PatternPayload;
use std::thread::JoinHandle;
use std::thread;
use async_channel::{unbounded, Sender, Receiver, RecvError};
use async_std::net::UdpSocket;
use async_std::task;
use async_std::io::Error;
use futures_lite::FutureExt;

use crate::midi_controller_worker::{midi_controller_worker, ControllerCommand};



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

#[derive(Debug)]
enum WorkerResult {
    Command(WorkerCommand),
    PayloadError(bincode::Error),
    ChannelError(RecvError),
    SocketError(Error)
}

enum SocketResult {
    Recv(usize),
    Stop,
    Error(Error)
}

async fn spawn_socket_worker(port: u16,
                             notes_sender: Sender<PatternPayload>,
                             socket_stop_channel: Receiver<bool>,
                             worker_result_sender: Sender<WorkerResult>
) {
    let mut buf = vec![0u8; 1024];

    let socket = match UdpSocket::bind(format!("127.0.0.1:{}", port)).await {
        Ok(socket) => {
            info!("Listening on port {}", port);
            socket
        }
        Err(err) => {
            error!("Cannot bind on port {} : {:?}", port, err);
            worker_result_sender.send(WorkerResult::SocketError(err)).await.unwrap();
            return;
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
                        worker_result_sender.send(WorkerResult::PayloadError(err)).await.unwrap();
                        return;
                    }
                }
            }
            SocketResult::Stop => {
                info!("Quitting socket port {}", port);
                return;
            },
            SocketResult::Error(err) => {
                info!("Error {} : quitting socket port {}", err, port);
                worker_result_sender.send(WorkerResult::SocketError(err)).await.unwrap();
                return;
            }
        }
    }
}


async fn command_reader(command_receiver: Receiver<WorkerCommand>, worker_result_sender: Sender<WorkerResult>) {
    loop {
        match command_receiver.recv().await {
            Ok(command) => {
                #[cfg(feature="worker_debug")]
                info!("Received command {:?}", command);
                worker_result_sender.send(WorkerResult::Command(command)).await.unwrap()
            },
            Err(err) => {
                error!("Error while reading command channel: {}", err);
                worker_result_sender.send(WorkerResult::ChannelError(err)).await.unwrap();
                return
            }
        }
    }
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

            info!("spawning command receiver");
            {
                let command_receiver = command_receiver.clone();
                let worker_result_sender = worker_result_sender.clone();
                task::spawn(command_reader(command_receiver, worker_result_sender));
            }

            loop {
                #[cfg(feature="worker_debug")]
                info!("waiting for a command");
                let worker_result = worker_result_receiver.recv().await.unwrap();
                #[cfg(feature="worker_debug")]
                info!("Got {:02X?}", worker_result);

                match worker_result {
                    WorkerResult::Command(command) => {
                        match command {
                            WorkerCommand::Stop => {
                                socket_stop_channels.sender.send(true).await.unwrap();
                                midi_controller_channels.sender.send(ControllerCommand::Stop).await.unwrap();
                                return;
                            }
                            WorkerCommand::SetPort(port) => {
                                info!("Switching to port {}" , port);
                                socket_stop_channels.sender.send(true).await.unwrap();

                                let (sender, receiver) = unbounded::<bool>();
                                socket_stop_channels.sender = sender;
                                socket_stop_channels.receiver = receiver;

                                info!("socket worker stopped");
                                {
                                    let notes_sender = notes_sender.clone();
                                    let socket_stop_receiver = socket_stop_channels.receiver.clone();
                                    let worker_result_sender = worker_result_sender.clone();
                                    task::spawn(spawn_socket_worker(port, notes_sender, socket_stop_receiver,
                                                                    worker_result_sender));
                                }

                                info!("stopping controller worker");

                                midi_controller_channels.sender.send(ControllerCommand::Stop).await.unwrap();

                                info!("controller worker stopped");
                                let (sender, receiver) = unbounded::<ControllerCommand>();
                                midi_controller_channels.sender = sender ;
                                midi_controller_channels.receiver = receiver;

                                task::spawn(midi_controller_worker(format!("Arpegiator {}", port),
                                                                   midi_controller_channels.receiver.clone()));
                            }

                            WorkerCommand::SendToController(controller_command) => {
                                midi_controller_channels.sender.send(controller_command).await.unwrap();
                            }
                        }
                    }
                    WorkerResult::PayloadError(err) => {
                        error!("Invalid payload received. Data received from wrong service ? ( {} )", err);
                    }
                    WorkerResult::SocketError(_) => {
                        // don't respawn, wait until user chooses another port
                    }
                    WorkerResult::ChannelError(err) => {
                        error!("Command channel error, quitting worker ({})", err);
                        socket_stop_channels.sender.send(true).await.unwrap();
                        midi_controller_channels.sender.send(ControllerCommand::Stop).await.unwrap();
                        return;
                    }
                }
            }
        }
    };

    let handle = thread::spawn(move || task::block_on(main));

    WorkerChannels {
        command_sender,
        notes_receiver,
        worker: handle,
    }
}
