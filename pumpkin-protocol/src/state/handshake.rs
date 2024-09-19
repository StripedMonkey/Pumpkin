use std::io;

use serde::{Deserialize, Serialize};

use crate::{raw_packet::UncompressedPacket, server::handshake::SHandShake};

#[non_exhaustive]
pub enum SHandShakeMessage {
    SHandShake(SHandShake),
}

pub fn decode(packet: UncompressedPacket) -> Result<SHandShakeMessage, io::Error> {
    Err(io::Error::new(io::ErrorKind::Other, "not implemented"))
}
