use std::io;

use thiserror::Error;
pub mod authentication;
mod client;
pub use client::*;
mod client_packet;
pub mod connection;
pub mod player_packet;

pub use client::Client;

#[derive(Error, Debug)]
pub enum EncryptionError {
    #[error("failed to decrypt shared secret")]
    FailedDecrypt,
    #[error("shared secret has the wrong length")]
    SharedWrongLength,
}

#[allow(dead_code)]
fn would_block(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::WouldBlock
}

pub fn interrupted(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::Interrupted
}
