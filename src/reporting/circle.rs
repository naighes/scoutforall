use crate::reporting::typst_content::TypstContent;

pub struct Circle {
    stroke: Option<&'static str>,
    fill: Option<&'static str>,
    inset: Option<u8>,
    outset: Option<u8>,
    content: Option<Box<dyn TypstContent>>,
}

impl Circle {
    pub fn new() -> Self {
        Self {
            stroke: None,
            fill: None,
            inset: None,
            outset: None,
            content: None,
        }
    }

    pub fn stroke(mut self, s: &'static str) -> Self {
        self.stroke = Some(s);
        self
    }
    pub fn fill(mut self, f: &'static str) -> Self {
        self.fill = Some(f);
        self
    }
    pub fn inset(mut self, i: u8) -> Self {
        self.inset = Some(i);
        self
    }
    pub fn outset(mut self, o: u8) -> Self {
        self.outset = Some(o);
        self
    }
    pub fn with_content<C: TypstContent + 'static>(mut self, c: C) -> Self {
        self.content = Some(Box::new(c));
        self
    }
}

impl TypstContent for Circle {
    fn render(&self) -> String {
        let mut args = vec![];
        if let Some(s) = self.stroke {
            args.push(format!("stroke: {}", s));
        }
        if let Some(f) = self.fill {
            args.push(format!("fill: rgb(\"{}\")", f));
        }
        if let Some(i) = self.inset {
            args.push(format!("inset: {}pt", i));
        }
        if let Some(o) = self.outset {
            args.push(format!("outset: {}pt", o));
        }
        let content = match &self.content {
            Some(c) => format!("[#{}]", c.render()),
            None => "".into(),
        };
        format!("circle({}, {})", args.join(", "), content)
    }
}
