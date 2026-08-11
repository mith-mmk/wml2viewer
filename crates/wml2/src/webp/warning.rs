//! WebP-specific warning types.

use crate::warning::{ImgWarning, WarningKind};
use std::fmt::*;

#[derive(Debug)]
pub enum WebpWarningKind {
    MetadataCorruption,
    MetadataEncoding,
}

pub struct WebpWarning {
    kind: WebpWarningKind,
    message: Option<String>,
}

impl ImgWarning for WebpWarning {}

impl Debug for WebpWarning {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Display::fmt(self, f)
    }
}

impl Display for WebpWarning {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match &self.message {
            Some(message) => write!(f, "{}: {}", self.kind.as_str(), message),
            None => write!(f, "{}", self.kind.as_str()),
        }
    }
}

impl WebpWarning {
    pub fn new(kind: WebpWarningKind) -> Self {
        Self {
            kind,
            message: None,
        }
    }

    pub fn new_const(kind: WebpWarningKind, message: String) -> Self {
        Self {
            kind,
            message: Some(message),
        }
    }
}

impl WarningKind for WebpWarningKind {
    fn as_str(&self) -> &'static str {
        match self {
            WebpWarningKind::MetadataCorruption => "Metadata corruption",
            WebpWarningKind::MetadataEncoding => "Metadata encoding",
        }
    }
}
