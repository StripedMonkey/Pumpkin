use std::{io, net::SocketAddr};

use pumpkin_protocol::{packet_codec::UncompressedPacketCodec, state};
use tokio::net::TcpStream;
use tokio_stream::StreamExt as _;
use tokio_util::codec::Framed;

use super::Connection;

/// An initial client connection
pub(crate) struct HandShakingConnection {
    connection: Connection,
}

impl HandShakingConnection {
    pub fn new(tcp_stream: TcpStream, socket_addr: SocketAddr) -> Self {
        Self {
            connection: Connection {
                socket_addr,
                tcp_stream,
            },
        }
    }
    pub async fn handshake(&mut self) -> io::Result<()> {
        // Handle the handshake logic here
        // self.connection.tcp_stream
        let mut framer = Framed::new(
            &mut self.connection.tcp_stream,
            UncompressedPacketCodec::default(),
        );
        let packet = framer
            .next()
            .await
            .expect("Failed to get packet from stream")
            .expect("Failed to decode packet");
        state::handshake::decode(packet)?;
        Ok(())
    }
}
