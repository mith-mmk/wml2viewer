//! PNG-specific warning types.

use crate::warning::ImgWarning;
use std::fmt::*;

#[allow(unused)]
pub struct PngWarning {
    message: String,
}

impl ImgWarning for PngWarning {}

impl Debug for PngWarning {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        std::fmt::Display::fmt(&self.message, f)
    }
}

impl Display for PngWarning {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", &self.message)
    }
}

impl PngWarning {
    pub fn new(message: String) -> Self {
        Self { message }
    }
}
