use std::net::SocketAddr;

use tokio::net::TcpStream;

pub mod handshake_connection;

pub(crate) struct Connection {
    tcp_stream: TcpStream,
    socket_addr: SocketAddr,
}
