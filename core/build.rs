fn main() {
    // the analytics endpoint, baked into the binary at build time: rebuild
    // when it changes, not only when the sources do
    println!("cargo:rerun-if-env-changed=ANALYTICS_UPLOAD_URL");
    if let Ok(url) = std::env::var("ANALYTICS_UPLOAD_URL") {
        println!("cargo:rustc-env=ANALYTICS_UPLOAD_URL={}", url);
    }
}
