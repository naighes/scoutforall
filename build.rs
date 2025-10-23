#[cfg(target_os = "windows")]
fn main() {
    // windows icon
    let mut res = winres::WindowsResource::new();
    res.set_icon("assets/scout4all.ico");
    res.compile().unwrap();

    // inject env vars
    inject_env_vars();
}

#[cfg(not(target_os = "windows"))]
fn main() {
    inject_env_vars();
}

fn inject_env_vars() {
    if let Ok(token) = std::env::var("LINEAR_TOKEN") {
        println!("cargo:rustc-env=LINEAR_TOKEN={}", token);
    }
    if let Ok(team_id) = std::env::var("LINEAR_TEAM_ID") {
        println!("cargo:rustc-env=LINEAR_TEAM_ID={}", team_id);
    }
    if let Ok(url) = std::env::var("ANALYTICS_UPLOAD_URL") {
        println!("cargo:rustc-env=ANALYTICS_UPLOAD_URL={}", url);
    }
}
