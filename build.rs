#[cfg(target_os = "windows")]
fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("resources/wml2viwer.ico");
    let _ = res.compile();
}

#[cfg(not(target_os = "windows"))]
fn main() {}
