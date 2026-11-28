//! The head of a panel: its title and, at the right, its actions. In a
//! panel too narrow for both, the actions go under the title: they never
//! cover it.

use iced::{
    Element, Event, Length, Point, Rectangle, Size, Vector,
    advanced::{
        Clipboard, Layout, Renderer, Shell, Widget, layout, mouse, overlay, renderer,
        widget::{Operation, Tree},
    },
};

/// The height of a line of the head, that of its tallest action.
const LINE: f32 = 34.0;
/// Between the title and the actions on its line.
const GAP: f32 = 10.0;
/// Between the title and the actions under it.
const UNDER: f32 = 8.0;

pub struct Head<'a, M, R = iced::Renderer> {
    /// the title, then the actions
    parts: [Element<'a, M, iced::Theme, R>; 2],
}

impl<'a, M, R> Head<'a, M, R> {
    pub fn new(
        title: impl Into<Element<'a, M, iced::Theme, R>>,
        actions: impl Into<Element<'a, M, iced::Theme, R>>,
    ) -> Head<'a, M, R> {
        Head {
            parts: [title.into(), actions.into()],
        }
    }
}

/// Where the title and the actions go in a head `width` wide, and its
/// height.
fn place(width: f32, title: Size, actions: Size) -> (Point, Point, f32) {
    let right = (width - actions.width).max(0.0);
    match title.width + GAP + actions.width <= width {
        true => {
            let line = LINE.max(title.height).max(actions.height);
            (
                Point::new(0.0, (line - title.height) / 2.0),
                Point::new(right, (line - actions.height) / 2.0),
                line,
            )
        }
        false => {
            let line = LINE.max(title.height);
            (
                Point::new(0.0, (line - title.height) / 2.0),
                Point::new(right, line + UNDER),
                line + UNDER + actions.height,
            )
        }
    }
}

impl<M, R: Renderer> Widget<M, iced::Theme, R> for Head<'_, M, R> {
    fn children(&self) -> Vec<Tree> {
        self.parts.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.parts);
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Shrink,
        }
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &R, limits: &layout::Limits) -> layout::Node {
        let width = limits.max().width;
        let loose = limits.loose();
        let nodes: Vec<layout::Node> = self
            .parts
            .iter_mut()
            .zip(&mut tree.children)
            .map(|(part, tree)| part.as_widget_mut().layout(tree, renderer, &loose))
            .collect();
        let size = |i: usize| nodes.get(i).map_or(Size::ZERO, layout::Node::size);
        let (title, actions, height) = place(width, size(0), size(1));
        let placed = nodes
            .into_iter()
            .zip([title, actions])
            .map(|(node, at)| node.move_to(at))
            .collect();
        layout::Node::with_children(Size::new(width, height), placed)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &R,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            self.parts
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
                .for_each(|((part, tree), layout)| {
                    part.as_widget_mut()
                        .operate(tree, layout, renderer, operation);
                });
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &R,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        self.parts
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
            .for_each(|((part, tree), layout)| {
                part.as_widget_mut().update(
                    tree, event, layout, cursor, renderer, clipboard, shell, viewport,
                );
            });
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &R,
    ) -> mouse::Interaction {
        self.parts
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((part, tree), layout)| {
                part.as_widget()
                    .mouse_interaction(tree, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut R,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.parts
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .for_each(|((part, tree), layout)| {
                part.as_widget()
                    .draw(tree, renderer, theme, style, layout, cursor, viewport);
            });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &R,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, iced::Theme, R>> {
        overlay::from_children(
            &mut self.parts,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, M: 'a, R: Renderer + 'a> From<Head<'a, M, R>> for Element<'a, M, iced::Theme, R> {
    fn from(head: Head<'a, M, R>) -> Self {
        Element::new(head)
    }
}

#[cfg(test)]
mod tests {
    use {super::*, iced::widget::Space};

    fn block<'a>(width: f32, height: f32) -> Element<'a, (), iced::Theme, ()> {
        Space::new()
            .width(Length::Fixed(width))
            .height(Length::Fixed(height))
            .into()
    }

    /// The title and the actions laid out in `width`, and the head's height.
    fn laid_out(width: f32, actions: f32) -> (Rectangle, Rectangle, f32) {
        let mut head: Element<'_, (), iced::Theme, ()> =
            Head::new(block(90.0, 20.0), block(actions, 34.0)).into();
        let mut tree = Tree::new(&head);
        let node = head.as_widget_mut().layout(
            &mut tree,
            &(),
            &layout::Limits::new(Size::ZERO, Size::new(width, 600.0)),
        );
        let bounds = |i: usize| node.children()[i].bounds();
        (bounds(0), bounds(1), node.size().height)
    }

    #[test]
    fn the_actions_on_the_line_of_the_title_when_they_fit() {
        let (title, actions, height) = laid_out(300.0, 150.0);
        assert_eq!((title.x, title.y), (0.0, 7.0));
        assert_eq!((actions.x, actions.y), (150.0, 0.0));
        assert_eq!(height, LINE);
    }

    #[test]
    fn the_actions_under_a_title_they_would_cover() {
        let (title, actions, height) = laid_out(247.0, 200.0);
        assert_eq!(title.width, 90.0);
        assert!(!title.intersects(&actions), "{title:?} {actions:?}");
        assert_eq!((actions.x, actions.y), (47.0, LINE + UNDER));
        assert_eq!(height, LINE + UNDER + 34.0);
    }
}
