mod error;
mod reader;
mod streaming;

#[cfg(test)]
mod hardening_tests;

pub use error::{IoError, IoResult};
pub use reader::{StdfReader, StdfSource};
pub use streaming::{RecordEvent, StreamingRecordReader};
