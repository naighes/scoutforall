use crate::reporting::typst_content::TypstContent;

#[derive(Clone, Copy)]
pub enum StackDirection {
    LeftToRight,
}

impl StackDirection {
    fn as_str(&self) -> &'static str {
        match self {
            StackDirection::LeftToRight => "ltr",
        }
    }
}

pub struct Stack {
    dir: StackDirection,
    items: Vec<Box<dyn TypstContent>>,
}

impl Stack {
    pub fn new(dir: StackDirection) -> Self {
        Self { dir, items: vec![] }
    }

    pub fn push<C: TypstContent + 'static>(mut self, content: C) -> Self {
        self.items.push(Box::new(content));
        self
    }
}

impl TypstContent for Stack {
    fn render(&self) -> String {
        let dir = format!("dir: {}", self.dir.as_str());
        let items: String = self
            .items
            .iter()
            .map(|i| i.render())
            .collect::<Vec<_>>()
            .join(",\n  ");
        format!("stack(\n  {},\n  {}\n)", dir, items)
    }
}
