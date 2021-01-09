#[allow(unused_imports)]
use log::{error, info};

use std::{thread, error};
use async_channel::{Sender, Receiver};
use async_std::net::UdpSocket;
use ipc_channel::ipc::IpcSender;

use util::ipc_payload::{PatternPayload, IPCCommand};
use std::net::ToSocketAddrs;
use async_std::task;
use std::time::Duration;


pub(crate) enum IPCWorkerCommand {
    Stop,
    SetPort(u16),
    Send(PatternPayload),
}


async fn try_udp_send_receiver(port: u16) -> Result<IpcSender<IPCCommand>, Box<dyn error::Error>> {
    let socket = UdpSocket::bind("127.0.0.1:0").await?;
    let to = format!("127.0.0.1:{}", port).to_socket_addrs()?.next().ok_or("empty list")?;

    let (ipc_sender, ipc_receiver) = ipc_channel::ipc::channel::<IPCCommand>()?;

    let serialized_ipc_receiver = bincode::serialize(&ipc_receiver)?;
    socket.send_to(&*serialized_ipc_receiver, to).await?;
    task::sleep(Duration::new(1, 0)).await;

    ipc_sender.send(IPCCommand::Ping)?;

    Ok(ipc_sender)
}


async fn ipc_worker(ipc_worker_sender: Sender<IPCWorkerCommand>, ipc_worker_receiver: Receiver<IPCWorkerCommand>) {
    let mut port = None;
    let mut ipc_sender: Option<IpcSender<IPCCommand>> = None;

    while let Ok(command) = ipc_worker_receiver.recv().await {
        match command {
            IPCWorkerCommand::Stop => {
                break;
            }
            IPCWorkerCommand::SetPort(new_port) => {
                port = Some(new_port);
                ipc_sender = match try_udp_send_receiver(new_port).await {
                    Ok(ipc_sender) => Some(ipc_sender),
                    Err(err) => {
                        error!("Error while connecting to arpegiator on port {} : {}", port.unwrap(), err);
                        let ipc_worker_sender = ipc_worker_sender.clone();
                        task::spawn(async move {
                            task::sleep(Duration::new(1,0)).await;
                            ipc_worker_sender.send(IPCWorkerCommand::SetPort(port.unwrap())).await.unwrap();
                        });
                        None
                    }
                };
            }
            IPCWorkerCommand::Send(payload) => {
                let ipc_sender_ref = match ipc_sender.as_ref() {
                    None => {
                        error!("IPC not ready, ignoring {:?}", payload);
                        continue;
                    }
                    Some(ipc_sender) => ipc_sender
                };

                if let Err(err) = ipc_sender_ref.send(IPCCommand::PatternPayload(payload)) {
                    error!("IPC failed ; will attempt to reconnect ({})", err);
                    ipc_sender = None;
                    ipc_worker_sender.send(IPCWorkerCommand::SetPort(port.unwrap())).await.unwrap();
                };
            }
        }
    }
}


pub(crate) fn spawn_ipc_worker() -> Sender<IPCWorkerCommand> {
    let (worker_sender, worker_receiver) = async_channel::unbounded();
    {
        let worker_sender = worker_sender.clone();
        thread::spawn(move || task::block_on(
            ipc_worker(worker_sender, worker_receiver)
        ))
    };
    worker_sender
}
