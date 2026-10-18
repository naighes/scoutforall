use crate::reporting::{typst_content::TypstContent, util::escape_text};

pub struct Text {
    value: String,
    size: Option<u8>,
    bold: bool,
    fill: Option<&'static str>,
}

impl Text {
    pub fn new<T: Into<String>>(value: T) -> Self {
        Self {
            value: value.into(),
            size: None,
            bold: false,
            fill: None,
        }
    }

    pub fn size(mut self, v: u8) -> Self {
        self.size = Some(v);
        self
    }

    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub fn fill(mut self, c: &'static str) -> Self {
        self.fill = Some(c);
        self
    }
}

impl TypstContent for Text {
    fn render(&self) -> String {
        let mut args = vec![];
        if let Some(s) = self.size {
            args.push(format!("size: {}pt", s));
        }
        if self.bold {
            args.push("weight: \"bold\"".to_string());
        }
        if let Some(f) = self.fill {
            args.push(format!("fill: rgb(\"{}\")", f));
        }
        format!(
            r#"text("{}", {})"#,
            escape_text(&self.value),
            args.join(", ")
        )
    }
}
