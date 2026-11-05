//! A text field kept in hand: iced's writes the letter of a shortcut
//! (⌘Z writes "z" on macOS), and on a guided field (a date, an hour) the
//! cursor stays where the separators the form adds push it. The guard
//! keeps the shortcuts from the field, for the form to undo, and puts the
//! cursor back at the end after the keys that write (see `Follow`).

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

/// The keys after which the cursor goes back to the end of the text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Follow {
    /// none: the text is as the keys write it
    Never,
    /// the keys that write (a digit, ⌫), not the ones that move the
    /// cursor: the form adds the separators, ← → still go through the text
    Typing,
    /// ↑ ↓, which write a suggestion in place of the text
    Arrows,
}

impl Follow {
    fn after(self, key: &keyboard::Key) -> bool {
        use keyboard::key::Named;
        match (self, key) {
            (Follow::Never, _) => false,
            (
                Follow::Typing,
                keyboard::Key::Named(
                    Named::ArrowLeft
                    | Named::ArrowRight
                    | Named::ArrowUp
                    | Named::ArrowDown
                    | Named::Home
                    | Named::End
                    | Named::Tab
                    | Named::Shift
                    | Named::Enter
                    | Named::Escape,
                ),
            ) => false,
            (Follow::Typing, _) => true,
            (Follow::Arrows, keyboard::Key::Named(Named::ArrowUp | Named::ArrowDown)) => true,
            (Follow::Arrows, _) => false,
        }
    }
}

pub struct Guard<'a, M> {
    content: Element<'a, M>,
    follow: Follow,
}

#[derive(Default)]
struct State {
    /// a key went to the field: the cursor to move at the next frame,
    /// where the key left it (none: at the end)
    pending: Option<Option<usize>>,
}

type Paragraph = <iced::Renderer as iced::advanced::text::Renderer>::Paragraph;

/// Where the cursor of the field is in `text` (the start of what's
/// selected), as far as it's long.
fn cursor_of(tree: &Tree, text: &str) -> Option<usize> {
    use iced::widget::text_input::{self, cursor};
    let tree::State::Some(state) = &tree.state else {
        return None;
    };
    let state = state.downcast_ref::<text_input::State<Paragraph>>()?;
    match state.cursor().state(&text_input::Value::new(text)) {
        cursor::State::Index(at) => Some(at),
        cursor::State::Selection { start, end } => Some(start.min(end)),
    }
}

/// The text of the field.
struct TextOf(Option<String>);

impl Operation for TextOf {
    fn text_input(
        &mut self,
        _id: Option<&iced::widget::Id>,
        _bounds: Rectangle,
        state: &mut dyn operation::TextInput,
    ) {
        self.0 = Some(state.text().to_string());
    }

    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
}

pub fn guard<'a, M: 'a>(content: impl Into<Element<'a, M>>, follow: Follow) -> Guard<'a, M> {
    Guard {
        content: content.into(),
        follow,
    }
}

/// The cursor of the field moved where the key left it, as far as the text
/// the form wrote is long, or at the end.
struct Follow_(Option<usize>);

impl Operation for Follow_ {
    fn text_input(
        &mut self,
        _id: Option<&iced::widget::Id>,
        _bounds: Rectangle,
        state: &mut dyn operation::TextInput,
    ) {
        match self.0 {
            Some(at) => state.move_cursor_to(at.min(state.text().chars().count())),
            None => state.move_cursor_to_end(),
        }
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
            (Event::Window(window::Event::RedrawRequested(_)), Some(from)) => {
                tree.state.downcast_mut::<State>().pending = None;
                self.content.as_widget_mut().operate(
                    &mut tree.children[0],
                    layout,
                    renderer,
                    &mut Follow_(from),
                );
            }
            (Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }), _)
                if self.follow.after(key) && !modifiers.macos_command() =>
            {
                let mut text = TextOf(None);
                self.content.as_widget_mut().operate(
                    &mut tree.children[0],
                    layout,
                    renderer,
                    &mut text,
                );
                let text = text.0.unwrap_or_default();
                let before = cursor_of(&tree.children[0], &text);
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
                // a digit written in the text: the cursor after it, where
                // the key left it (room for one more); at the end, or a
                // whole suggestion written by the arrows: at the end
                let at_end = before.is_none_or(|at| at >= text.chars().count());
                let left = match (self.follow, at_end) {
                    (Follow::Typing, false) => cursor_of(&tree.children[0], &format!("{text} ")),
                    _ => None,
                };
                tree.state.downcast_mut::<State>().pending = Some(left);
                shell.request_redraw();
                return;
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

    #[test]
    fn the_cursor_follows_the_keys_that_write_not_the_ones_that_move_it() {
        use keyboard::{Key, key::Named};
        let digit = Key::Character("4".into());
        let left = Key::Named(Named::ArrowLeft);
        let down = Key::Named(Named::ArrowDown);
        assert!(Follow::Typing.after(&digit));
        assert!(Follow::Typing.after(&Key::Named(Named::Backspace)));
        assert!(!Follow::Typing.after(&left));
        assert!(!Follow::Never.after(&digit));
        assert!(Follow::Arrows.after(&down));
        assert!(!Follow::Arrows.after(&digit));
    }
}
