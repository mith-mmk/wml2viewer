//! BMP-specific warning types.

use crate::warning::WarningKind;

#[derive(Debug)]
pub enum BMPWarningKind {
    OutOfMemory,
    DataCorruption,
    BufferOverrun,
}

#[allow(unused)]
impl WarningKind for BMPWarningKind {
    fn as_str(&self) -> &'static str {
        use BMPWarningKind::*;
        match self {
            OutOfMemory => "Out of memory",
            DataCorruption => "Data Corruption",
            BufferOverrun => "Buffer Overrun",
        }
    }
}
