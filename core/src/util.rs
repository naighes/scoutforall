pub fn sanitize_filename(s: &str) -> String {
    s.to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// A setting baked into the binary at build time (see build.rs), which an
/// environment variable of the same name overrides, e.g. in development.
/// Pass `option_env!(name)` as `built_in`: release builds only have that.
pub fn config_var(name: &str, built_in: Option<&'static str>) -> Option<String> {
    std::env::var(name)
        .ok()
        .or_else(|| built_in.map(str::to_string))
        .filter(|v| !v.is_empty())
}
