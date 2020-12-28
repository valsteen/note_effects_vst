use log::{error, info};
use util::pattern_payload::PatternPayload;
use std::thread::JoinHandle;
use std::thread;
use smol::channel::{unbounded, Sender, Receiver};
use smol::future::{race, pending};
use async_net::UdpSocket;

pub enum SocketCommand {
    Stop,
    SetPort(u16)
}

pub struct SocketChannels {
    pub command_sender: Sender<SocketCommand>,
    pub notes_receiver: Receiver<PatternPayload>
}

enum FutureResult {
    Command(SocketCommand),
    Payload(PatternPayload),
    PayloadError,
    ChannelError,
    SocketError
}

pub fn create_socket_thread() -> (JoinHandle<()>, SocketChannels) {
    let (command_sender, command_receiver) = unbounded::<SocketCommand>();
    let (notes_sender, notes_receiver) = unbounded::<PatternPayload>();

    let handle = thread::spawn(move || {
        let mut socket : Option<UdpSocket> = None;
        let mut buf = vec![0u8; 1024];

        smol::block_on( async {
            loop {
                let channel_future = async {
                    match command_receiver.recv().await {
                        Ok(command) => {
                            FutureResult::Command(command)
                        }
                        Err(err) => {
                            error!("Error on command channel while receiving {:?} {}", err, err);
                            FutureResult::ChannelError
                        }
                    }
                };

                let socket_future = async {
                    match socket.as_ref() {
                        None => pending().await,
                        Some(socket) => {
                            match socket.recv(&mut buf).await {
                                Ok(len) => {
                                    match bincode::deserialize::<PatternPayload>(&buf[..len]) {
                                        Ok(payload) => {
                                            FutureResult::Payload(payload)
                                        }
                                        Err(err) => {
                                            error!("Could not deserialize: {:?}", err);
                                            FutureResult::PayloadError
                                        }
                                    }
                                }
                                Err(err) => {
                                    error!("Socket error: {:?}", err);
                                    FutureResult::SocketError
                                }
                            }
                        }
                    }
                };

                match race(channel_future, socket_future).await {
                    FutureResult::Command(command) => {
                        match command {
                            SocketCommand::Stop => {
                                return
                            }
                            SocketCommand::SetPort(port) => {
                                socket = None;

                                match UdpSocket::bind(format!("127.0.0.1:{}", port)).await {
                                    Ok(le_socket) => {
                                        info!("Listening on port {}", port);
                                        socket = Some(le_socket)
                                    }
                                    Err(err) => {
                                        error!("Cannot bind on port {} : {:?}", port, err)
                                    }
                                }
                            }
                        }
                    }
                    FutureResult::Payload(payload) => {
                        notes_sender.send(payload).await.unwrap();
                    }
                    FutureResult::ChannelError => {
                        // unrecoverable
                        return
                    }
                    FutureResult::SocketError => {
                        socket = None
                    }
                    FutureResult::PayloadError => {
                        // noop, just loop over
                    }
                }
            }
        });
    });

    (handle, SocketChannels {
        command_sender,
        notes_receiver
    })
}
