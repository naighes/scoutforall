use crate::reporting::{align::Align, typst_content::TypstContent};

pub struct Cell {
    colspan: Option<u8>,
    align: Option<Align>,
    fill: Option<&'static str>,
    stroke: Option<&'static str>,
    content: Box<dyn TypstContent>,
}

impl Cell {
    pub fn new<C: TypstContent + 'static>(content: C) -> Self {
        Self {
            colspan: None,
            align: None,
            fill: None,
            stroke: None,
            content: Box::new(content),
        }
    }

    pub fn colspan(mut self, v: u8) -> Self {
        self.colspan = Some(v);
        self
    }

    pub fn align(mut self, a: Align) -> Self {
        self.align = Some(a);
        self
    }

    pub fn fill(mut self, c: &'static str) -> Self {
        self.fill = Some(c);
        self
    }

    pub fn stroke(mut self, s: &'static str) -> Self {
        self.stroke = Some(s);
        self
    }
}

impl TypstContent for Cell {
    fn render(&self) -> String {
        let colspan = self
            .colspan
            .map_or(String::new(), |v| format!("colspan: {},", v));
        let align = match self.align {
            Some(Align::Left) => "align: left + horizon,".to_string(),
            Some(Align::Center) => "align: center + horizon,".to_string(),
            Some(Align::Right) => "align: right + horizon,".to_string(),
            None => "".to_string(),
        };
        let fill = self
            .fill
            .map_or(String::new(), |c| format!("fill: rgb(\"{}\"),", c));
        let stroke = self
            .stroke
            .map_or(String::new(), |s| format!("stroke: {},", s));
        format!(
            r#"table.cell(
  {colspan}
  {align}
  {fill}
  {stroke}
  {},
),"#,
            self.content.render()
        )
    }
}

pub struct Row {
    cells: Vec<Cell>,
}

impl Row {
    pub fn new(cells: Vec<Cell>) -> Self {
        Self { cells }
    }

    pub fn render(&self) -> String {
        self.cells
            .iter()
            .map(|c| c.render())
            .collect::<Vec<_>>()
            .join("\n")
    }
}
