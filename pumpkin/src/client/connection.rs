use pumpkin_protocol::{
    packet_decoder::PacketDecoder, packet_encoder::PacketEncoder, ClientPacket, PacketError,
};
use tokio::{io::AsyncWriteExt, net::TcpStream};

pub struct Connection {
    pub client: TcpStream,
    pub enc: PacketEncoder,
    pub dec: PacketDecoder,
}

impl Connection {
    pub fn new(client: TcpStream) -> Self {
        Self {
            client,
            enc: PacketEncoder::default(),
            dec: PacketDecoder::default(),
        }
    }
    pub async fn try_send_packet<P: ClientPacket>(
        &mut self,
        packet: &P,
    ) -> Result<(), PacketError> {
        self.enc.append_packet(packet)?;
        self.client
            .write_all(&self.enc.take())
            .await
            .map_err(|_| PacketError::ConnectionWrite)?;
        Ok(())
    }
}
