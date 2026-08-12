//! Platform-independent contracts shared by the desktop and mobile frontends.

pub mod archive;
mod error;
pub mod image;
#[path = "reading_model.rs"]
pub mod reading;

pub use error::{CoreError, CoreErrorKind, CoreResult};

#[cfg(test)]
mod archive_tests;
#[cfg(test)]
mod image_tests;
#[cfg(test)]
#[path = "reading_model_tests.rs"]
mod reading_tests;
