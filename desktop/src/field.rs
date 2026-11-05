//! A text field kept in hand: iced's writes the letter of a shortcut
//! (⌘Z writes "z" on macOS), and on a guided field (a date, an hour) the
//! cursor stays where the separators the form adds push it. The guard
//! keeps the shortcuts from the field, for the form to undo, and puts the
//! cursor back at the end after each key.

use iced::{
    Element, Event, Length, Rectangle, Size, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
        widget::{Operation, Tree, operation, tree},
    },
    keyboard, window,
};

/// The shortcuts the field keeps: copy, paste, cut, select all.
const KEPT: [&str; 4] = ["c", "v", "x", "a"];

pub struct Guard<'a, M> {
    content: Element<'a, M>,
    /// the cursor back at the end after each key
    cursor_at_end: bool,
}

#[derive(Default)]
struct State {
    /// a key went to the field: the cursor to move at the next frame
    pending: bool,
}

pub fn guard<'a, M: 'a>(content: impl Into<Element<'a, M>>, cursor_at_end: bool) -> Guard<'a, M> {
    Guard {
        content: content.into(),
        cursor_at_end,
    }
}

/// The cursor of the field at the end of its text.
struct ToTheEnd;

impl Operation for ToTheEnd {
    fn text_input(
        &mut self,
        _id: Option<&iced::widget::Id>,
        _bounds: Rectangle,
        state: &mut dyn operation::TextInput,
    ) {
        state.move_cursor_to_end();
    }

    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
}

/// A shortcut of the system the field would take as a letter.
fn is_shortcut(event: &Event) -> bool {
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Character(c),
            modifiers,
            ..
        }) => modifiers.macos_command() && !KEPT.contains(&c.to_lowercase().as_str()),
        _ => false,
    }
}

impl<M> Widget<M, iced::Theme, iced::Renderer> for Guard<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        let pending = tree.state.downcast_ref::<State>().pending;
        match (event, pending) {
            // not captured: the app reads it, the form undoes
            (event, _) if is_shortcut(event) => return,
            (Event::Window(window::Event::RedrawRequested(_)), true) => {
                tree.state.downcast_mut::<State>().pending = false;
                self.content.as_widget_mut().operate(
                    &mut tree.children[0],
                    layout,
                    renderer,
                    &mut ToTheEnd,
                );
            }
            (Event::Keyboard(keyboard::Event::KeyPressed { .. }), _) if self.cursor_at_end => {
                tree.state.downcast_mut::<State>().pending = true;
                shell.request_redraw();
            }
            _ => {}
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, iced::Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, M: 'a> From<Guard<'a, M>> for Element<'a, M> {
    fn from(guard: Guard<'a, M>) -> Element<'a, M> {
        Element::new(guard)
    }
}

#[cfg(test)]
mod tests {
    use {super::*, iced::keyboard::Modifiers};

    fn pressed(c: &str, modifiers: Modifiers) -> Event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Character(c.into()),
            modified_key: keyboard::Key::Character(c.into()),
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers,
            text: Some(c.into()),
            repeat: false,
        })
    }

    #[test]
    fn the_shortcuts_are_kept_from_the_field_but_copy_and_paste() {
        let command = match cfg!(target_os = "macos") {
            true => Modifiers::LOGO,
            false => Modifiers::CTRL,
        };
        // on the other systems ⌃Z is no letter: the field writes nothing
        assert_eq!(
            is_shortcut(&pressed("z", command)),
            cfg!(target_os = "macos")
        );
        assert!(!is_shortcut(&pressed("v", command)));
        assert!(!is_shortcut(&pressed("z", Modifiers::empty())));
    }
}
