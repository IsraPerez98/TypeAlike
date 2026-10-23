//! Tests the fix of a typo before the caret and how Tab accepts it.

use crate::engine::{AfterAccept, Engine, Reaction, WordFix};
use crate::inference::{Completion, Correction, InferenceError};
use crate::key::Key;
use crate::settings::{Settings, TypoFix};
use crate::test_support::{ExpectedRequest, completion};

use super::support::{
    FakeDesktop, accept, engine_waiting_for, engine_waiting_with, engine_with, press, request, type_text,
};

#[expect(clippy::unnecessary_wraps, reason = "on_completion takes a Result")]
fn correction(typed: &str, fixed: &str, text: &str) -> Result<Completion, InferenceError> {
    Ok(Completion {
        correction: Some(Correction {
            typed: typed.to_owned(),
            fixed: fixed.to_owned(),
        }),
        ..completion(text).expect("the completion has text")
    })
}

fn word_fix(typo: &str, word: &str, rest: &str) -> WordFix {
    WordFix {
        typo: typo.to_owned(),
        word: word.to_owned(),
        rest: rest.to_owned(),
    }
}

fn engine_showing_a_fix() -> Engine {
    engine_showing_a_fix_with("very soon")
}

/// Returns an engine that shows the fix of "dlei" in "the dlei", kept through a typed space.
fn engine_showing_a_fix_after_a_space() -> Engine {
    let mut engine = engine_showing_a_fix();
    press(&mut engine, Key::Char(' '));
    engine
}

/// Presses Tab in a field whose text before the caret is `before_caret`.
fn press_tab_in_field(engine: &mut Engine, before_caret: &str) -> Reaction {
    engine.on_key(Key::Tab, &FakeDesktop::with_field_text(before_caret))
}

fn replace_after_the_space() -> Reaction {
    Reaction::Replace {
        deleted: 5,
        text: "delivery ".to_owned(),
        then: AfterAccept::Rest("soon".to_owned()),
    }
}

/// Returns an engine that shows the fix of "dlei" in "the dlei", with `text` after the fixed
/// letters "deli".
fn engine_showing_a_fix_with(text: &str) -> Engine {
    let mut engine = engine_waiting_for("the dlei");
    engine.on_completion(correction(" dlei", " deli", text));
    engine
}

#[test]
fn completion_that_fixes_a_typo_shows_the_fix() {
    let mut engine = engine_waiting_for("the dlei");

    let reaction = engine.on_completion(correction(" dlei", " deli", "very soon"));

    assert_eq!(reaction, Reaction::ShowFix(word_fix("dlei", "delivery", " soon")));
}

#[test]
fn fix_that_finishes_its_word_shows_the_rest_after_it() {
    let mut engine = engine_waiting_for("I will recieve");

    let reaction = engine.on_completion(correction(" recieve", " receive", " the parcel"));

    assert_eq!(
        reaction,
        Reaction::ShowFix(word_fix("recieve", "receive", " the parcel"))
    );
}

#[test]
fn tab_on_a_fix_replaces_the_typo_with_the_fixed_word() {
    let mut engine = engine_showing_a_fix();

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(
        reaction,
        Reaction::Replace {
            deleted: 4,
            text: "delivery".to_owned(),
            then: AfterAccept::Rest(" soon".to_owned()),
        }
    );
}

#[test]
fn tab_on_a_fix_without_a_rest_requests_the_next_suggestion() {
    let mut engine = engine_waiting_for("the dlei");
    engine.on_completion(correction(" dlei", " deli", "very"));

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(
        reaction,
        Reaction::Replace {
            deleted: 4,
            text: "delivery".to_owned(),
            then: AfterAccept::Next(Box::new(request("the delivery"))),
        }
    );
}

#[test]
fn tab_on_a_fix_leaves_the_closing_punctuation_for_the_next_tab() {
    let mut engine = engine_waiting_for("the dlei");
    engine.on_completion(correction(" dlei", " deli", "very, soon"));

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(
        reaction,
        Reaction::Replace {
            deleted: 4,
            text: "delivery".to_owned(),
            then: AfterAccept::Rest(", soon".to_owned()),
        }
    );
}

#[test]
fn tab_after_a_fix_types_only_its_closing_punctuation() {
    let mut engine = engine_waiting_for("the dlei");
    engine.on_completion(correction(" dlei", " deli", "very, soon then"));
    press(&mut engine, Key::Tab);

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(reaction, accept(",", " soon then"));
}

#[test]
fn text_before_the_caret_holds_the_fixed_word_after_tab() {
    let mut engine = engine_showing_a_fix();
    press(&mut engine, Key::Tab);
    press(&mut engine, Key::Escape);

    let reaction = press(&mut engine, Key::Backspace);

    assert_eq!(reaction, request("the deliver"));
}

#[test]
fn fix_does_not_show_again_after_a_backspace() {
    let mut engine = engine_showing_a_fix();
    press(&mut engine, Key::Char('x'));

    let reaction = press(&mut engine, Key::Backspace);

    assert_eq!(reaction, request("the dlei"));
}

#[test]
fn typing_while_a_fix_shows_asks_again() {
    let mut engine = engine_showing_a_fix();

    let reaction = press(&mut engine, Key::Char('v'));

    assert_eq!(reaction, request("the dleiv"));
}

#[test]
fn escape_hides_a_fix() {
    let mut engine = engine_showing_a_fix();

    let reaction = press(&mut engine, Key::Escape);

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn fix_with_a_bad_suggestion_after_it_hides() {
    let mut engine = engine_waiting_for("the dlei");

    let reaction = engine.on_completion(correction(" dlei", " deli", "very very very very"));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn fix_of_text_that_does_not_end_the_text_before_the_caret_hides() {
    let mut engine = engine_waiting_for("the dlei");

    let reaction = engine.on_completion(correction(" plea", " please", " help"));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn fix_with_typo_fixes_off_hides() {
    let mut engine = engine_with(&Settings {
        typo_fix: TypoFix::Off,
        ..Settings::default()
    });
    type_text(&mut engine, "the dlei");

    let reaction = engine.on_completion(correction(" dlei", " deli", "very soon"));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn space_after_a_fix_keeps_the_fix_with_the_space() {
    let mut engine = engine_showing_a_fix_with("very");

    let reaction = press(&mut engine, Key::Char(' '));

    assert_eq!(reaction, Reaction::ShowFix(word_fix("dlei ", "delivery ", "")));
}

#[test]
fn each_sign_after_a_fix_keeps_the_fix_with_the_sign() {
    let signs = [',', ';', ':', '!', '?'];

    let reactions = signs.map(|sign| press(&mut engine_showing_a_fix_with("very"), Key::Char(sign)));

    assert_eq!(
        reactions,
        [
            ("dlei,", "delivery,"),
            ("dlei;", "delivery;"),
            ("dlei:", "delivery:"),
            ("dlei!", "delivery!"),
            ("dlei?", "delivery?"),
        ]
        .map(|(typo, word)| Reaction::ShowFix(word_fix(typo, word, "")))
    );
}

#[test]
fn sign_and_then_a_space_after_a_fix_keep_the_fix() {
    let mut engine = engine_showing_a_fix_with("very");

    let reaction = type_text(&mut engine, ", ");

    assert_eq!(reaction, Reaction::ShowFix(word_fix("dlei, ", "delivery, ", "")));
}

#[test]
fn kept_fix_keeps_the_rest_after_the_typed_delimiter() {
    let mut engine = engine_showing_a_fix_with("very, soon");

    let reaction = type_text(&mut engine, ", ");

    assert_eq!(
        reaction,
        Reaction::ShowFix(word_fix("dlei, ", "delivery, ", "soon"))
    );
}

#[test]
fn kept_fix_drops_a_rest_that_does_not_start_with_the_typed_delimiter() {
    let mut engine = engine_showing_a_fix();

    let reaction = press(&mut engine, Key::Char(','));

    assert_eq!(reaction, Reaction::ShowFix(word_fix("dlei,", "delivery,", "")));
}

#[test]
fn tab_after_a_kept_delimiter_replaces_the_typo_and_the_delimiter() {
    let mut engine = engine_showing_a_fix();
    press(&mut engine, Key::Char(' '));

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(
        reaction,
        Reaction::Replace {
            deleted: 5,
            text: "delivery ".to_owned(),
            then: AfterAccept::Rest("soon".to_owned()),
        }
    );
}

#[test]
fn tab_after_a_kept_delimiter_without_a_rest_requests_the_next_suggestion() {
    let mut engine = engine_showing_a_fix_with("very");
    type_text(&mut engine, ", ");

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(
        reaction,
        Reaction::Replace {
            deleted: 6,
            text: "delivery, ".to_owned(),
            then: AfterAccept::Next(Box::new(request("the delivery, "))),
        }
    );
}

#[test]
fn second_space_after_a_fix_asks_again() {
    let mut engine = engine_showing_a_fix();

    let reaction = type_text(&mut engine, "  ");

    assert_eq!(reaction, request("the dlei  "));
}

#[test]
fn second_sign_after_a_fix_asks_again() {
    let mut engine = engine_showing_a_fix();

    let reaction = type_text(&mut engine, ",,");

    assert_eq!(reaction, request("the dlei,,"));
}

#[test]
fn sign_after_a_kept_space_asks_again() {
    let mut engine = engine_showing_a_fix();

    let reaction = type_text(&mut engine, " ,");

    assert_eq!(reaction, request("the dlei ,"));
}

#[test]
fn letter_after_a_kept_delimiter_asks_again() {
    let mut engine = engine_showing_a_fix();

    let reaction = type_text(&mut engine, " d");

    assert_eq!(reaction, request("the dlei d"));
}

#[test]
fn backspace_after_a_kept_delimiter_asks_again() {
    let mut engine = engine_showing_a_fix();
    press(&mut engine, Key::Char(' '));

    let reaction = press(&mut engine, Key::Backspace);

    assert_eq!(reaction, request("the dlei"));
}

#[test]
fn space_after_a_typo_with_typo_fixes_off_asks_again() {
    let settings = Settings {
        typo_fix: TypoFix::Off,
        ..Settings::default()
    };
    let mut engine = engine_waiting_with(&settings, "the dlei");
    engine.on_completion(correction(" dlei", " deli", "very soon"));

    let reaction = press(&mut engine, Key::Char(' '));

    let expected = ExpectedRequest {
        typo_check: settings.typo_check(),
        ..ExpectedRequest::framed(&[], "the dlei ")
    };
    assert_eq!(reaction, Reaction::from(expected));
}

#[test]
fn tab_on_a_kept_fix_replaces_the_typo_while_the_field_shows_it() {
    let mut engine = engine_showing_a_fix_after_a_space();

    let reaction = press_tab_in_field(&mut engine, "the dlei ");

    assert_eq!(reaction, replace_after_the_space());
}

#[test]
fn tab_on_a_kept_fix_replaces_the_typo_when_the_field_read_is_older_than_its_delimiter() {
    let reads = ["the dlei", "the dlei,"];

    let reactions = reads.map(|read| {
        let mut engine = engine_showing_a_fix_with("very");
        type_text(&mut engine, ", ");
        press_tab_in_field(&mut engine, read)
    });

    let replace = Reaction::Replace {
        deleted: 6,
        text: "delivery, ".to_owned(),
        then: AfterAccept::Next(Box::new(request("the delivery, "))),
    };
    assert_eq!(reactions, [replace.clone(), replace]);
}

#[test]
fn tab_on_a_kept_fix_takes_a_no_break_space_of_the_field_for_its_space() {
    let mut engine = engine_showing_a_fix_after_a_space();

    let reaction = press_tab_in_field(&mut engine, "the dlei\u{a0}");

    assert_eq!(reaction, replace_after_the_space());
}

#[test]
fn tab_on_a_kept_fix_replaces_the_typo_when_the_field_text_is_unknown() {
    let mut engine = engine_showing_a_fix_after_a_space();

    let reaction = engine.on_key(Key::Tab, &FakeDesktop::default());

    assert_eq!(reaction, replace_after_the_space());
}

#[test]
fn tab_on_a_kept_fix_that_the_app_changed_swallows_the_tab() {
    let mut engine = engine_showing_a_fix_after_a_space();

    let reaction = press_tab_in_field(&mut engine, "the deli ");

    assert_eq!(reaction, Reaction::Swallow);
}

#[test]
fn key_after_a_refused_fix_continues_the_text_of_the_field() {
    let mut engine = engine_showing_a_fix_after_a_space();
    press_tab_in_field(&mut engine, "the deli ");

    let reaction = engine.on_key(Key::Char('s'), &FakeDesktop::with_field_text("the deli "));

    assert_eq!(reaction, request("the deli s"));
}

#[test]
fn tab_on_a_fix_without_a_kept_delimiter_does_not_read_the_field() {
    let mut engine = engine_showing_a_fix();
    let desktop = FakeDesktop::with_field_text("the deli");

    engine.on_key(Key::Tab, &desktop);

    assert_eq!(desktop.surroundings_reads.get(), 0);
}
