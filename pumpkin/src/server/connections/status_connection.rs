use pumpkin_protocol::client::status::CStatusResponse;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::Connection;

pub(crate) struct StatusConnection<T> {
    connection: Connection<T>,
}

impl<T> StatusConnection<T>
where
    T: AsyncReadExt + AsyncWriteExt + Unpin,
{
    pub fn new(connection: Connection<T>) -> Self {
        Self { connection }
    }

    pub async fn submit_response(&mut self, response: CStatusResponse<'_>) {
        // let 
        // self.connection.write();
    }
}
