use {
    log::{error, info},
    std::{thread, error},
    async_channel::{Sender, Receiver},
    async_std::net::UdpSocket,
    ipc_channel::ipc::IpcSender,
    util::ipc_payload::{PatternPayload, IPCCommand, BootstrapPayload},
    std::net::ToSocketAddrs,
    async_std::task,
    std::time::Duration
};

pub(crate) enum IPCWorkerCommand {
    Stop,
    SetPort(u16),
    Send(PatternPayload),
    TryConnect
}


async fn try_udp_send_receiver(port: u16) -> Result<IpcSender<IPCCommand>, Box<dyn error::Error + Send + Sync>> {
    let socket = UdpSocket::bind("127.0.0.1:0").await?;
    let to = format!("127.0.0.1:{}", port).to_socket_addrs()?.next().ok_or("empty list")?;

    let (one_shot, name) = ipc_channel::ipc::IpcOneShotServer::new()?;
    let serialized_name = bincode::serialize(&name)?;
    socket.send_to(&*serialized_name, to).await?;

    let (bootstrap_result_sender, bootstrap_result_receiver) = async_channel::unbounded();

    thread::spawn(move || {
        let (_, result) = one_shot.accept().unwrap();
        match result {
            BootstrapPayload::Channel(ipc_sender) => {
                info!("Returning ipc_sender");
                bootstrap_result_sender.try_send(ipc_sender).unwrap()
            }
            BootstrapPayload::Timeout => {
                error!("timed out while waiting for a connection");
            }
        }
    });

    task::sleep(Duration::new(1,0 )).await;

    let ipc_sender = bootstrap_result_receiver.try_recv().or_else(|e| {
        IpcSender::<BootstrapPayload>::connect(name).or(Err("Cannot connect to signal timeout on ipc bootstrap"))?
            .send(BootstrapPayload::Timeout).or(Err("Cannot send timeout to ipc bootstrapper"))?;
        Err(format!("Connection timeout {}", e))
    })?;

    let (ping_sender, ping_receiver) = ipc_channel::ipc::channel::<()>()?;
    info!("sending ping");
    ipc_sender.send(IPCCommand::Ping(ping_sender))?;
    task::sleep(Duration::new(1,0)).await;

    ping_receiver.try_recv().or(Err("Pong not received"))?;
    info!("Ping received");
    Ok(ipc_sender)
}

async fn ipc_worker(ipc_worker_sender: Sender<IPCWorkerCommand>, ipc_worker_receiver: Receiver<IPCWorkerCommand>) {
    let mut port = None;
    let mut ipc_sender: Option<IpcSender<IPCCommand>> = None;
    let mut retry_scheduled = false;

    while let Ok(command) = ipc_worker_receiver.recv().await {
        match command {
            IPCWorkerCommand::Stop => {
                break;
            }
            IPCWorkerCommand::TryConnect => {
                retry_scheduled = false;
                if ipc_sender.is_some() || port.is_none() {
                    // already connected or not configured
                    continue;
                }
                ipc_sender = match try_udp_send_receiver(port.unwrap()).await {
                    Ok(ipc_sender) => Some(ipc_sender),
                    Err(err) => {
                        error!("Error while connecting to arpegiator on port {} : {}", port.unwrap(), err);
                        task::sleep(Duration::new(1,0)).await;
                        retry_scheduled = true;
                        ipc_worker_sender.send(IPCWorkerCommand::TryConnect).await.unwrap();
                        None
                    }
                };
            },
            IPCWorkerCommand::SetPort(new_port) => {
                ipc_sender = None;
                port = Some(new_port);
                if !retry_scheduled {
                    ipc_worker_sender.send(IPCWorkerCommand::TryConnect).await.unwrap();
                    retry_scheduled = true
                }

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
                    if !retry_scheduled {
                        ipc_worker_sender.send(IPCWorkerCommand::TryConnect).await.unwrap();
                        retry_scheduled = true
                    }
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
