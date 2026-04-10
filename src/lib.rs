pub mod app;
pub mod bench;
pub mod configs;
pub mod dependent;
pub mod drawers;
pub mod filesystem;
pub mod options;
pub mod ui;

pub fn get_version() -> String {
    format!("{}-lib{}", env!("CARGO_PKG_VERSION"), wml2::get_version())
}

pub fn get_auther() -> String {
    env!("CARGO_PKG_AUTHORS").to_string()
}

pub fn get_copyright() -> String {
    "(C) 2026 MITH@mmk".to_string()
}

pub fn get_prograname() -> String {
    env!("CARGO_PKG_NAME").to_string()
}
