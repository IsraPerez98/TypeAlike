//! The fake desktops and the requests that the app tests share.

use crate::app::App;
use crate::desktop::{CopiedText, Desktop, Field, FieldText, OcrAvailability, Surroundings};
use crate::engine::Reaction;
use crate::key::Key;
use crate::settings::{PromptPreparation, Settings};
use crate::test_support::suggestion_request;

pub(super) struct FakeDesktop(pub(super) Field);

impl Desktop for FakeDesktop {
    fn field(&self) -> Field {
        self.0
    }

    fn surroundings(&self) -> Surroundings {
        Surroundings::default()
    }

    fn copied_text(&self) -> Option<CopiedText> {
        None
    }
}

/// A text field of the app `app_id`, on the website `domain` if any.
pub(super) struct TextField {
    pub(super) app_id: &'static str,
    pub(super) domain: Option<&'static str>,
    pub(super) field: Field,
    pub(super) ocr: OcrAvailability,
    pub(super) has_readable_text: bool,
}

impl Desktop for TextField {
    fn field(&self) -> Field {
        self.field
    }

    fn surroundings(&self) -> Surroundings {
        Surroundings {
            app_id: Some(self.app_id.to_owned()),
            domain: self.domain.map(str::to_owned),
            field_text: self.has_readable_text.then(FieldText::default),
            ..Surroundings::default()
        }
    }

    fn copied_text(&self) -> Option<CopiedText> {
        None
    }

    fn ocr_availability(&self) -> OcrAvailability {
        self.ocr
    }
}

pub(super) const MAIL: TextField = TextField {
    app_id: "com.apple.mail",
    domain: None,
    field: Field::Open,
    ocr: OcrAvailability::Unsupported,
    has_readable_text: true,
};

/// A text field of a browser, on a website.
pub(super) const WEBMAIL: TextField = TextField {
    app_id: "com.apple.Safari",
    domain: Some("mail.example"),
    ..MAIL
};

pub(super) fn request(text_before: &str) -> Reaction {
    Reaction::Request(suggestion_request(&[], text_before))
}

pub(super) fn type_key(app: &mut App, character: char) -> Reaction {
    app.on_key(Key::Char(character), &FakeDesktop(Field::Open))
}

pub(super) fn app_preparing_on_focus() -> App {
    let mut app = App::new();
    app.use_settings(&Settings {
        prompt_preparation: PromptPreparation::OnFocus,
        ..Settings::default()
    });
    app
}
