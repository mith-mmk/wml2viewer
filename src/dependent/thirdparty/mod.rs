mod directories;
mod locale_config;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use directories::*;
pub use locale_config::*;
