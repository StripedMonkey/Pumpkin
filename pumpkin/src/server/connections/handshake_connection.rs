use std::{io, net::SocketAddr};

use log::debug;
use pumpkin_protocol::{packet_codec::UncompressedPacketCodec, state};
use tokio::net::TcpStream;
use tokio_stream::StreamExt as _;
use tokio_util::codec::Framed;

use super::Connection;

/// An initial client connection
pub(crate) struct HandShakingConnection {
    connection: Connection<TcpStream>,
}

impl HandShakingConnection {
    pub fn new(tcp_stream: TcpStream, socket_addr: SocketAddr) -> Self {
        Self {
            connection: Connection::new(tcp_stream, socket_addr),
        }
    }

    /// Perform the initial handshake with the client by waiting for the client to tell us what it wants.
    /// 
    /// The only action taken here, is the client tells us what it would like to do.
    pub async fn handshake(&mut self) -> io::Result<()> {
        // Handle the handshake logic here

        // self.connection.tcp_stream
        let mut framer = Framed::new(
            &mut self.connection.stream,
            UncompressedPacketCodec::default(),
        );
        let packet = framer
            .next()
            .await
            .expect("Failed to get packet from stream")
            .expect("Failed to decode packet");
        match state::handshake::decode(packet)? {
            state::handshake::SHandShakeMessage::SHandShake(handshake) => {
                debug!("Received handshake from client: {:?}", handshake);
            },
            _ => todo!(),
        }
        Ok(())
    }
}
