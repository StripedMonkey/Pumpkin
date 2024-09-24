use log::info;
use std::net::SocketAddr;
use tokio::{
    io,
    net::{TcpListener, TcpStream},
};

use crate::server::connections::HandShakingConnection;

use super::connections::Connection;


struct ConnectionListener {
    listener: TcpListener,
}

struct ClientConnection {
    client_state: Connection<TcpStream>,

}

impl ConnectionListener {
    pub fn new(listener: TcpListener) -> Self {
        Self { listener }
    }
    async fn generate_listener(&self) -> io::Result<()> {
        let listener = TcpListener::bind("TODO").await?;
        self.connection_handler(listener.accept().await?).await?;
        Ok(())
    }

    async fn connection_handler(
        &self,
        (tcp_stream, socket_addr): (TcpStream, SocketAddr),
    ) -> io::Result<()> {
        info!("New connection received from {}", socket_addr);
        let mut handshake = HandShakingConnection::new(tcp_stream, socket_addr);

        match handshake.handshake().await {
            Ok(_) => {
                // Proceed to the next state
                info!("Handshake successful for {}", socket_addr);
                // Handle the next state
            }
            Err(e) => {
                info!("Handshake failed for {}: {:?}", socket_addr, e);
                // Handle the error
            }
        }
        // Assume

        Ok(())
    }
}
