use log::log;
use pumpkin_inventory::InventoryError;
use pumpkin_protocol::bytebuf::DeserializerError;
use std::fmt::Display;

pub trait PumpkinError: std::error::Error + Display {
    fn is_kick(&self) -> bool;

    fn log(&self) {
        log!(self.severity(), "{}", self.to_string());
    }

    fn severity(&self) -> log::Level;

    fn client_kick_reason(&self) -> Option<String>;
}

impl<ErrorType: PumpkinError + 'static> From<ErrorType> for Box<dyn PumpkinError> {
    fn from(error: ErrorType) -> Self {
        Box::new(error)
    }
}
impl PumpkinError for InventoryError {
    fn is_kick(&self) -> bool {
        use InventoryError::*;
        match self {
            InvalidSlot | ClosedContainerInteract(..) | InvalidPacket | PermissionError => true,
            LockError | OutOfOrderDragging | MultiplePlayersDragging => false,
        }
    }
    fn severity(&self) -> log::Level {
        use InventoryError::*;
        match self {
            LockError
            | InvalidSlot
            | ClosedContainerInteract(..)
            | InvalidPacket
            | PermissionError => log::Level::Error,
            OutOfOrderDragging => log::Level::Info,
            MultiplePlayersDragging => log::Level::Warn,
        }
    }

    fn client_kick_reason(&self) -> Option<String> {
        None
    }
}

impl PumpkinError for DeserializerError {
    fn is_kick(&self) -> bool {
        true
    }

    fn severity(&self) -> log::Level {
        log::Level::Error
    }

    fn client_kick_reason(&self) -> Option<String> {
        None
    }
}