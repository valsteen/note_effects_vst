use std::net::{SocketAddr, UdpSocket};

pub struct DebugSocket {
    socket: UdpSocket,
    to: SocketAddr,
}

impl DebugSocket {
    fn create() {
        let socket_result = (10000..20000)
            .map(|port| UdpSocket::bind(format!("127.0.0.1:{}", port)))
            .filter_map(Result::ok)
            .next();

        let socket = socket_result.unwrap();
        socket.set_nonblocking(true).unwrap();

        unsafe {
            DEBUG_SOCKET = Some(DebugSocket {
                socket,
                to: "127.0.0.1:5555".parse().unwrap(),
            })
        }
    }

    pub fn send(debug_str: &str) {
        if debug_str.len() == 0 {
            return
        }
        let debug_string = debug_str.to_owned() + "\n";
        unsafe {
            if let None = DEBUG_SOCKET {
                Self::create()
            }

            if let Some(debug_socket) = &DEBUG_SOCKET {
                debug_socket
                    .socket
                    .send_to(debug_string.as_bytes(), debug_socket.to)
                    .unwrap();
            }
        }
    }
}

static mut DEBUG_SOCKET: Option<DebugSocket> = None;
