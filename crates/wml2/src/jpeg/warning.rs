//! JPEG-specific warning types.

/*
 * jpeg/Warning.rs  Mith@mmk (C) 2022
 * use MIT License
 */
use crate::warning::ImgWarning;
use crate::warning::WarningKind;
use std::fmt::*;

#[derive(Debug)]
pub enum JpegWarningKind {
    IlligalRSTMaker,
    UnfindEOIMaker,
    DataCorruption,
    BufferOverrun,
    UnexpectMarker,
    UnknowFormat,
    UnreadbleString,
}

#[allow(unused)]
pub struct JpegWarning {
    kind: JpegWarningKind,
    message: Option<String>,
}

impl ImgWarning for JpegWarning {}

impl Debug for JpegWarning {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let s;
        match &self.message {
            None => s = self.kind.as_str().to_owned(),
            Some(message) => {
                s = self.kind.as_str().to_owned() + ":" + message;
            }
        }
        std::fmt::Display::fmt(&s, f)
    }
}

impl Display for JpegWarning {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let s;
        match &self.message {
            None => s = self.kind.as_str().to_owned(),
            Some(message) => {
                s = self.kind.as_str().to_owned() + ":" + message;
            }
        }
        write!(f, "{}", &s)
    }
}

impl JpegWarning {
    pub fn new(kind: JpegWarningKind) -> Self {
        Self {
            kind,
            message: None,
        }
    }

    pub fn new_const(kind: JpegWarningKind, message: String) -> Self {
        Self {
            kind,
            message: Some(message),
        }
    }
}

#[allow(unused)]
#[allow(non_snake_case)]
impl WarningKind for JpegWarningKind {
    fn as_str(&self) -> &'static str {
        use JpegWarningKind::*;
        match &self {
            IlligalRSTMaker => "Illigal RST Maker",
            UnfindEOIMaker => "Unfind EOI Maker",
            DataCorruption => "Data Corruption",
            BufferOverrun => "Buffer Overrun",
            UnexpectMarker => "Unexpect Marker",
            UnknowFormat => "Unexpect Maker",
            UnreadbleString => "Unreadable String",
        }
    }
}
