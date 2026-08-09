pub mod app;
pub mod bench;
pub mod benchlog;
pub mod configs;
pub mod dependent;
pub mod drawers;
pub mod filesystem;
pub mod options;
pub mod path_classification;
pub mod ui;
pub mod wml2_formats;

#[cfg(test)]
#[path = "../tests/support/src/test_support.rs"]
pub(crate) mod test_support;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(android_app: android_activity::AndroidApp) {
    if let Err(error) = app::run_android(android_app) {
        eprintln!("wml2viewer Android startup failed: {error}");
    }
}

#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wml2viewer_ios_main(
    app_support_dir: *const std::ffi::c_char,
    documents_dir: *const std::ffi::c_char,
    caches_dir: *const std::ffi::c_char,
) -> i32 {
    fn path_from_c_string(
        value: *const std::ffi::c_char,
    ) -> Result<std::path::PathBuf, std::io::Error> {
        if value.is_null() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "iOS path argument is null",
            ));
        }
        let value = unsafe { std::ffi::CStr::from_ptr(value) };
        let value = value.to_str().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "iOS path argument is not UTF-8",
            )
        })?;
        Ok(std::path::PathBuf::from(value))
    }

    let paths = (|| {
        Ok::<_, std::io::Error>((
            path_from_c_string(app_support_dir)?,
            path_from_c_string(documents_dir)?,
            path_from_c_string(caches_dir)?,
        ))
    })();
    match paths.and_then(|(app_support, documents, caches)| {
        app::run_ios(app_support, documents, caches)
            .map_err(|error| std::io::Error::other(format!("iOS startup failed: {error}")))
    }) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

pub fn get_version() -> String {
    format!("{}-lib{}", env!("CARGO_PKG_VERSION"), wml2::get_version())
}

pub fn get_author() -> String {
    env!("CARGO_PKG_AUTHORS").to_string()
}

pub fn get_copyright() -> String {
    "(C) 2026 MITH@mmk".to_string()
}

pub fn get_program_name() -> String {
    env!("CARGO_PKG_NAME").to_string()
}
