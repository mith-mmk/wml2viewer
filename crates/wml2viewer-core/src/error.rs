use std::error::Error;
use std::fmt::{self, Display, Formatter};

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreErrorKind {
    InvalidInput,
    Io,
    Archive,
    Decode,
    Cancelled,
    Encode,
    Limit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreError {
    kind: CoreErrorKind,
    message: String,
}

impl CoreError {
    pub fn new(kind: CoreErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(CoreErrorKind::InvalidInput, message)
    }

    pub fn decode(message: impl Into<String>) -> Self {
        Self::new(CoreErrorKind::Decode, message)
    }

    pub fn cancelled(message: impl Into<String>) -> Self {
        Self::new(CoreErrorKind::Cancelled, message)
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::new(CoreErrorKind::Io, message)
    }

    pub fn archive(message: impl Into<String>) -> Self {
        Self::new(CoreErrorKind::Archive, message)
    }

    pub fn encode(message: impl Into<String>) -> Self {
        Self::new(CoreErrorKind::Encode, message)
    }

    pub fn limit(message: impl Into<String>) -> Self {
        Self::new(CoreErrorKind::Limit, message)
    }

    pub const fn kind(&self) -> CoreErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for CoreError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for CoreError {}
