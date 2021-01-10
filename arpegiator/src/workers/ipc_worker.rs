#[allow(unused_imports)]
use log::{error, info};

use async_channel::Sender;
use async_std::net::UdpSocket;
use async_std::task;
use ipc_channel::ipc::{IpcSender, IpcReceiver, IpcReceiverSet, IpcSelectionResult};

use util::ipc_payload::{PatternPayload, IPCCommand, BootstrapPayload};

use crate::workers::main_worker::WorkerCommand;
use std::{thread, error};
use std::mem::take;
use util::system::Uuid;


pub(crate) enum IPCWorkerCommand {
    SocketReceive(IpcReceiver<IPCCommand>, Uuid),
    Stop(Sender<()>, Uuid),
    IPCDisconnect(Uuid),
    PayloadReceived(PatternPayload, Uuid),
}

fn bootstrap_ipc(ipc_server_name: String) -> Result<IpcReceiver<IPCCommand>, Box<dyn error::Error>> {
    let ipc_bootstrap_sender = IpcSender::connect(ipc_server_name)?;
    let (ipc_sender, ipc_receiver) = ipc_channel::ipc::channel()?;
    ipc_bootstrap_sender.send(BootstrapPayload::Channel(ipc_sender))?;
    Ok(ipc_receiver)
}


async fn udp_receive_worker(socket: UdpSocket, sender: Sender<IPCWorkerCommand>) {
    let mut buf = vec![0u8; 1024];
    let mut exit_event_id = Uuid::default();

    while let Ok(len) = socket.recv(&mut buf).await {
        exit_event_id = Uuid::new_v4();

        info!("[{}] Received {} bytes", exit_event_id, len);

        let ipc_worker_command = match bincode::deserialize::<String>(&buf[..len]) {
            Ok(ipc_server_name) => {
                match bootstrap_ipc(ipc_server_name) {
                    Ok(ipc_receiver) => {
                        info!("[{}] Received IPC Receiver via UDP", exit_event_id);
                        IPCWorkerCommand::SocketReceive(ipc_receiver, exit_event_id)
                    }
                    Err(err) => {
                        info!("[{}] failed to get an IPC Receiver via UDP: {}", exit_event_id, err);
                        continue;
                    }
                }
            }
            Err(err) => {
                error!("[{}] Ignoring invalid UDP payload ({}) - expected a name string", exit_event_id, err);
                continue;
            }
        };

        if let Err(err) = sender.send(ipc_worker_command).await {
            error!("[{}] UDP Worker: error while trying to send worker command {}, quitting UDP Worker",
                   exit_event_id, err);
            break;
        }
    };

    #[cfg(feature = "worker_debug")]
    info!("[{}] Leaving udp receiver worker", exit_event_id)
}


fn spawn_ipc_receiver_thread(ipc_receiver: IpcReceiver<IPCCommand>,
                             ipc_worker_sender: Sender<IPCWorkerCommand>) -> Result<IpcSender<IPCCommand>, Box<dyn error::Error>> {
    let mut set = IpcReceiverSet::new()?;
    set.add(ipc_receiver)?;

    let (sender, receiver) = ipc_channel::ipc::channel::<IPCCommand>()?;
    set.add(receiver)?;

    thread::spawn(|| task::block_on(async move {
        let mut exit_event_id = Uuid::default();

        #[cfg(feature = "worker_debug")] info!("started ipc worker");
        'mainloop: while let Ok(results) = set.select() {
            for result in results {
                let opaque_message = match result {
                    IpcSelectionResult::MessageReceived(_, opaque_message) => opaque_message,
                    IpcSelectionResult::ChannelClosed(_) => {
                        exit_event_id = Uuid::new_v4();
                        error!("[{}] ipc channel closed", exit_event_id);
                        break 'mainloop;
                    }
                };
                match opaque_message.to::<IPCCommand>().unwrap() {
                    IPCCommand::PatternPayload(payload) => {
                        let event_id = Uuid::new_v4();
                        if let Err(err) = ipc_worker_sender.send(IPCWorkerCommand::PayloadReceived(payload, event_id)).await {
                            exit_event_id = event_id;
                            error!("[{}] could not send payload to ipc worker {}", exit_event_id, err);
                            break 'mainloop;
                        }
                    }
                    IPCCommand::Ping(sender) => {
                        info!("Received ping from peer");
                        match sender.send(()) {
                            Ok(_) => { info!("pong sent") }
                            Err(err) => { info!("error while sending pong: {}", err) }
                        }
                    }
                    IPCCommand::Stop(ack_channel_sender, event_id) => {
                        exit_event_id = event_id;
                        #[cfg(feature = "worker_debug")] info!("[{}] Stopping IPC worker thread", exit_event_id);
                        ack_channel_sender.send(()).unwrap_or_else(|err| {
                            error!("[{}] Could not signal ipc thread stop {}", err, exit_event_id);
                        });
                        break 'mainloop;
                    }
                }
            }
        };
        ipc_worker_sender.try_send(IPCWorkerCommand::IPCDisconnect(exit_event_id)).unwrap_or_else(|err| {
            error!("[{}] Error {} while signaling IPC receiver quitting", exit_event_id, err);
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

        let mut exit_event_id : Uuid = Uuid::default();

        while let Ok(command) = ipc_worker_receiver.recv().await {
            match command {
                IPCWorkerCommand::SocketReceive(ipc_receiver_from_socket, event_id) => {
                    if ipc_receiver_sender.is_some() {
                        if let Err(err) = close_ipc_receiver_thread(
                            take(&mut ipc_receiver_sender).unwrap(), event_id) {
                            error!("[{}] Error while shutting down ipc receiver worker {}", event_id, err)
                        }
                    }

                    let ipc_worker_sender = ipc_worker_sender.clone();

                    if let Ok(sender) = spawn_ipc_receiver_thread(ipc_receiver_from_socket, ipc_worker_sender) {
                        ipc_receiver_sender = Some(sender);
                    };
                }
                IPCWorkerCommand::Stop(sender, event_id) => {
                    exit_event_id = event_id;
                    match sender.send(()).await {
                        Ok(_) => { info!("[{}] Quitting socket port {}", exit_event_id, port); }
                        Err(err) => { info!("[{}] Error while quitting socket port {}: {}", exit_event_id, port, err); }
                    }
                    break;
                }
                IPCWorkerCommand::IPCDisconnect(event_id) => {
                    ipc_receiver_sender = None;
                    error!("[{}] IPC Receiver disconnected", event_id)
                }
                IPCWorkerCommand::PayloadReceived(payload, event_id) => {
                    let _payload_time = payload.time;
                    if let Err(err) = pattern_sender.send(payload).await {
                        exit_event_id = event_id;
                        error!("[{}] IPC worker: notes sender channel error, quitting ({})", exit_event_id, err);
                        break;
                    }

                    #[cfg(feature = "device_debug")]
                    {
                        let local_time = unsafe { mach::mach_time::mach_absolute_time() };
                        let diff_nanoseconds = local_time - _payload_time;
                        info!("Received time: {:?} current time: {:?} = {} nanoseconds",
                              payload_time,
                              local_time,
                              diff_nanoseconds);
                    }
                }
            }
        }

        #[cfg(feature = "worker_debug")] info!("[{}] stopping udp worker on port {}", exit_event_id, port);
        udp_worker_handle.cancel().await;
        #[cfg(feature = "worker_debug")] info!("[{}] udp worker on port {} stopped", exit_event_id, port);

        if let Some(sender) = take(&mut ipc_receiver_sender) {
            close_ipc_receiver_thread(sender, exit_event_id).unwrap_or_else(|err| {
                info!("[{}] ipc receiver thread did not quit gracefully: {}", exit_event_id, err);
            })
        }

        if let Err(err) = worker_command_sender.send(WorkerCommand::IPCWorkerStopped(exit_event_id, port)).await {
            info!("[{}] Could not signal main worker {}", err, exit_event_id);
        }

        #[cfg(feature = "worker_debug")] info!("[{}] exiting ipc worker ( port {} )", exit_event_id, port);
    }));

    Ok(returned_ipc_worker_sender)
}


fn close_ipc_receiver_thread(ipc_receiver_sender: IpcSender<IPCCommand>, event_id: Uuid) -> Result<(), Box<dyn
error::Error>> {
    #[cfg(feature = "worker_debug")] info!("[{}] close_ipc_receiver_thread: enter", event_id);
    let (ack_sender, ack_receiver) = ipc_channel::ipc::channel::<()>()?;

    #[cfg(feature = "worker_debug")] info!("[{}] stopping ipc receiver", event_id);
    ipc_receiver_sender.send(IPCCommand::Stop(ack_sender, event_id))?;

    #[cfg(feature = "worker_debug")] info!("[{}] waiting for ack from ipc receiver", event_id);
    ack_receiver.try_recv().map_err(|x| format!("Error while receiving ack from ipc receiver {:?}", x))?;

    #[cfg(feature = "worker_debug")] info!("[{}] stopped ipc receiver", event_id);
    Ok(())
}
