pub fn escape_text(input: &str) -> String {
    input.replace('"', "\\\"")
}
