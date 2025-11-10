//! Core functionalities: serial I/O, logging buffer, settings.

pub mod serial_service;
pub mod logbuf;
pub mod encoding;

pub use serial_service::{SerialConfig, SerialEvent, SerialService, PortInfo, LineEnding, PinStates};
pub use logbuf::{LogStore, LogEntry, Direction};
pub use encoding::TextEncoding;

