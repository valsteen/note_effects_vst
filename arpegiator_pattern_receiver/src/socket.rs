use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::mpsc::{channel, Sender};
use std::thread;
use std::thread::JoinHandle;

use util::pattern_payload::PatternPayload;

pub enum SenderSocketCommand {
    Stop,
    SetPort(u16),
    Send(PatternPayload),
}


pub fn create_socket_thread() -> (JoinHandle<()>, Sender<SenderSocketCommand>) {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let (sender, receiver) = channel::<SenderSocketCommand>();

    let handle = thread::spawn(move || {
        let mut to: Option<SocketAddr> = None;

        while let Ok(command) = receiver.recv() {
            match command {
                SenderSocketCommand::Stop => {
                    return;
                }
                SenderSocketCommand::SetPort(port) => {
                    to = format!("127.0.0.1:{}", port).to_socket_addrs().unwrap().next()
                }
                SenderSocketCommand::Send(payload) => {
                    if let Some(port_to) = to {
                        socket.send_to(&*bincode::serialize(&payload).unwrap(), port_to).unwrap();
                    }
                }
            }
        }
    });

    (handle, sender)
}
