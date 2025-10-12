#[cfg(target_os = "windows")]
fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("assets/scout4all.ico");
    res.compile().unwrap();
}

#[cfg(not(target_os = "windows"))]
fn main() {}
