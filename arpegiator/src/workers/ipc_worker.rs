#[allow(unused_imports)]
use log::{error, info};

use async_channel::Sender;
use async_std::net::UdpSocket;
use async_std::task;
use futures_lite::io::{Error, ErrorKind};
use ipc_channel::ipc::{IpcSender, IpcReceiver, IpcReceiverSet};

use util::ipc_payload::{PatternPayload, IPCCommand};

use crate::workers::main_worker::WorkerCommand;
use std::{thread, error};
use std::mem::take;


pub(crate) enum IPCWorkerCommand {
    SocketReceive(IpcReceiver<IPCCommand>),
    Stop,
    IPCDisconnect,
    PayloadReceived(PatternPayload),
}


async fn udp_receive_worker(socket: UdpSocket, sender: Sender<IPCWorkerCommand>) {
    let mut buf = vec![0u8; 1024];

    while let Ok(len) = socket.recv(&mut buf).await {
        let ipc_worker_command = match bincode::deserialize::<IpcReceiver<IPCCommand>>(&buf[..len]) {
            Ok(ipc_receiver) => {
                info!("Received IPC Receiver via UDP");
                IPCWorkerCommand::SocketReceive(ipc_receiver)
            }
            Err(err) => {
                error!("Ignoring invalid UDP payload ({}) - expected ICP receiver", err);
                continue;
            }
        };

        if let Err(err) = sender.send(ipc_worker_command).await {
            error!("UDP Worker: error while trying to send worker command {}, quitting UDP Worker", err);
            break;
        }
    };
}


fn spawn_ipc_receiver_thread(ipc_receiver: IpcReceiver<IPCCommand>,
                             ipc_worker_sender: Sender<IPCWorkerCommand>) -> Result<IpcSender<IPCCommand>, Box<dyn error::Error>> {
    let mut set = IpcReceiverSet::new()?;
    set.add(ipc_receiver)?;

    let (sender, receiver) = ipc_channel::ipc::channel::<IPCCommand>()?;
    set.add(receiver)?;

    thread::spawn(|| task::block_on(async move {
        while let Ok(results) = set.select() {
            for result in results {
                let (_, opaque_message) = result.unwrap();
                match opaque_message.to::<IPCCommand>().unwrap() {
                    IPCCommand::PatternPayload(payload) => {
                        if let Err(err) = ipc_worker_sender.send(IPCWorkerCommand::PayloadReceived(payload)).await {
                            error!("could not send payload to ipc worker {}", err);
                            break;
                        }
                    }
                    IPCCommand::Ping => {
                        info!("Received ping from peer")
                    }
                    IPCCommand::Stop(ack_channel_sender) => {
                        ack_channel_sender.send(()).unwrap_or_else(|err| {
                            error!("Could not signal ipc thread stop {}", err);
                        });
                        info!("Stopping IPC worker thread");
                        break;
                    }
                }
            }
        };
        ipc_worker_sender.try_send(IPCWorkerCommand::IPCDisconnect).unwrap_or_else(|err| {
            error!("Error {} while signaling IPC receiver quitting", err);
        });
    }));

    Ok(sender)
}


pub(crate) fn spawn_ipc_worker(port: u16,
                               pattern_sender: Sender<PatternPayload>,
                               worker_command_sender: Sender<WorkerCommand>,
) -> Result<Sender<IPCWorkerCommand>, Box<dyn error::Error>> {
    let (ipc_worker_sender, ipc_worker_receiver) = async_channel::unbounded::<IPCWorkerCommand>();

    let returned_ipc_worker_sender = ipc_worker_sender.clone();

    let socket = task::block_on(UdpSocket::bind(format!("127.0.0.1:{}", port)))?;

    thread::spawn(move || task::block_on(async move {
        let mut ipc_receiver_sender: Option<IpcSender<IPCCommand>> = None;
        let udp_worker_handle = task::spawn(
            udp_receive_worker(socket, ipc_worker_sender.clone())
        );

        while let Ok(command) = ipc_worker_receiver.recv().await {
            match command {
                IPCWorkerCommand::SocketReceive(ipc_receiver_from_socket) => {
                    if ipc_receiver_sender.is_some() {
                        if let Err(err) = close_ipc_receiver_thread(take(&mut ipc_receiver_sender).unwrap()) {
                            error!("Error while shutting down ipc receiver worker {}", err)
                        }
                    }

                    let ipc_worker_sender = ipc_worker_sender.clone();

                    if let Ok(sender) = spawn_ipc_receiver_thread(ipc_receiver_from_socket, ipc_worker_sender) {
                        ipc_receiver_sender = Some(sender);
                    };
                }
                IPCWorkerCommand::Stop => {
                    info!("Quitting socket port {}", port);
                    break;
                }
                IPCWorkerCommand::IPCDisconnect => {
                    ipc_receiver_sender = None;
                    error!("IPC Receiver disconnected")
                }
                IPCWorkerCommand::PayloadReceived(payload) => {
                    if let Err(err) = pattern_sender.send(payload).await {
                        error!("IPC worker: notes sender channel error, quitting ({})", err);
                        break;
                    }
                }
            }
        }

        udp_worker_handle.cancel().await;
        if let Some(sender) = take(&mut ipc_receiver_sender) {
            close_ipc_receiver_thread(sender).unwrap_or_else(|err| {
                info!("ipc receiver thread did not quit gracefully: {}", err);
            })
        }

        if let Err(err) = worker_command_sender.send(WorkerCommand::IPCWorkerStopped).await {
            info!("Could not signal main worker {}", err);
        }
    }));

    Ok(returned_ipc_worker_sender)
}


fn close_ipc_receiver_thread(ipc_receiver_sender: IpcSender<IPCCommand>) -> Result<(), Box<dyn error::Error>> {
    let (ack_sender, ack_receiver) = ipc_channel::ipc::channel::<()>()?;
    ipc_receiver_sender.send(IPCCommand::Stop(ack_sender))?;
    ack_receiver.try_recv().map_err(|x| Error::new(ErrorKind::Other, format!("{:?}", x)))?;
    Ok(())
}
