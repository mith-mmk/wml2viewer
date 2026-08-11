mod directories;
mod locale_config;
#[cfg(not(target_os = "ios"))]
pub use directories::*;
pub use locale_config::*;
