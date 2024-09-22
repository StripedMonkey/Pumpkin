use std::fmt::Debug;

use pumpkin_macros::packet;
use serde::Deserialize;

use crate::{ConnectionState, VarInt};

#[packet(0x00)]
#[derive(Deserialize)]
pub struct SHandShake {
    pub protocol_version: VarInt,
    pub server_address: String, // 255
    pub server_port: u16,
    pub next_state: ConnectionState,
}

impl Debug for SHandShake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SHandShake")
            .field("protocol_version", &self.protocol_version.0)
            .field("server_address", &self.server_address)
            .field("server_port", &self.server_port)
            .field("next_state", &self.next_state)
            .finish()
    }
}
