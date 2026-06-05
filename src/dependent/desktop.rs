use std::path::PathBuf;

pub fn pick_directory_dialog() -> Option<PathBuf> {
    rfd::FileDialog::new().pick_folder()
}

pub fn download_url_to_temp(url: &str) -> Option<PathBuf> {
    super::download_http_url(url)
}
