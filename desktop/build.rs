//! The icon of the executable on Windows.

#[cfg(target_os = "windows")]
fn main() -> std::io::Result<()> {
    let mut res = winres::WindowsResource::new();
    res.set_icon("assets/scout4all.ico");
    res.compile()
}

#[cfg(not(target_os = "windows"))]
fn main() {}
