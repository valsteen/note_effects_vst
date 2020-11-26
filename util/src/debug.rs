use std::net::{SocketAddr, UdpSocket};

pub struct DebugSocket {
    socket: UdpSocket,
    to: SocketAddr,
}

impl DebugSocket {
    pub fn send(&mut self, debug_str: &str) {
        let debug_string = debug_str.to_owned() + "\n";
        self.socket
            .send_to(debug_string.as_bytes(), self.to)
            .unwrap();
    }
}

impl Default for DebugSocket {
    fn default() -> Self {
        let socket_result = (10000..20000)
            .map(|port| UdpSocket::bind(format!("127.0.0.1:{}", port)))
            .filter_map(Result::ok)
            .next();

        let socket = socket_result.unwrap();
        socket.set_nonblocking(true).unwrap();
        DebugSocket {
            socket,
            to: "127.0.0.1:5555".parse().unwrap(),
        }
    }
}
