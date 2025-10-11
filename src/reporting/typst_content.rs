pub trait TypstContent {
    fn render(&self) -> String;
}

impl TypstContent for Box<dyn TypstContent> {
    fn render(&self) -> String {
        (**self).render()
    }
}
