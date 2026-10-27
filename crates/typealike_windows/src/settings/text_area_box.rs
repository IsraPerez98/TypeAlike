//! The multi-line text box of a text area on a settings page.

use std::rc::Rc;

use windows_reactor::{Callback, Component, ComponentContext, TextBox, TextWrapping, View, ViewContext};

const TEXT_AREA_MIN_HEIGHT: f64 = 160.0;

/// The multi-line text box of a text area. It gets its text one render after it exists, because
/// reactor sets the text of a new `TextBox` before `AcceptsReturn`, and a single-line WinUI text
/// box keeps only the first line of the text that it gets.
pub(super) struct TextAreaBox {
    shows_text: bool,
}

#[derive(Clone, PartialEq)]
pub(super) struct TextAreaBoxInput {
    pub(super) text: String,
    pub(super) label: String,
    pub(super) on_text_changed: Callback<Rc<str>>,
}

impl Component for TextAreaBox {
    type Input = TextAreaBoxInput;
    type Message = ();

    fn create(_input: &TextAreaBoxInput, context: &ComponentContext<Self>) -> Self {
        if !context.sender().send(()) {
            tracing::warn!("the text area did not take a message");
        }
        Self { shows_text: false }
    }

    fn update(&mut self, (): (), _context: &ComponentContext<Self>) {
        self.shows_text = true;
    }

    fn view(&self, input: &TextAreaBoxInput, _context: &mut ViewContext<'_, Self>) -> View {
        let field = TextBox::new()
            .accepts_return(true)
            .text_wrapping(TextWrapping::Wrap)
            .min_height(TEXT_AREA_MIN_HEIGHT)
            .automation_name(&input.label)
            .on_text_changed(input.on_text_changed.clone());
        if self.shows_text {
            field.text(&input.text).into()
        } else {
            field.into()
        }
    }
}
