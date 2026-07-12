fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winres::WindowsResource::new();
    res.set_icon("resources/wml2viewer.ico");
    res.compile().expect("failed to compile Windows resources");
}
