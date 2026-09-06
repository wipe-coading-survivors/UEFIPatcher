use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParserError {
    #[error("invalid header: {0}")]
    InvalidHeader(String),
    #[error("unknown type")]
    UnknownType,
    #[error("end of buffer")]
    EndOfBuffer,
}

pub mod file;
pub mod image;
pub mod section;
pub mod target;
pub mod volume;
