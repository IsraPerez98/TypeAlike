//! Tests what Tab, typed keys and other keys do with the ghost text.

use crate::engine::{AfterAccept, Engine, Reaction};
use crate::key::Key;
use crate::test_support::completion;

use super::support::{
    FakeDesktop, accept, engine_showing, engine_waiting_for, press, request, show, type_text,
};

fn accept_last(text: &str, next: Reaction) -> Reaction {
    Reaction::Accept {
        text: text.to_owned(),
        then: AfterAccept::Next(Box::new(next)),
    }
}

#[test]
fn typing_next_suggested_character_shows_rest() {
    let mut engine = engine_showing("hel", "lo world");

    let reaction = press(&mut engine, Key::Char('l'));

    assert_eq!(reaction, show("o world"));
}

#[test]
fn typing_other_character_requests_new_completion() {
    let mut engine = engine_showing("hel", "lo");

    let reaction = press(&mut engine, Key::Char('p'));

    assert_eq!(reaction, request("help"));
}

#[test]
fn typing_whole_suggestion_requests_continuation() {
    let mut engine = engine_showing("hel", "lo");

    let reaction = type_text(&mut engine, "lo");

    assert_eq!(reaction, request("hello"));
}

#[test]
fn tab_accepts_next_word_of_suggestion() {
    let mut engine = engine_showing("hel", "lo big world");

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(reaction, accept("lo", " big world"));
}

#[test]
fn tab_types_the_word_before_its_closing_punctuation() {
    let cases = [
        (" regards, Sam", " regards", ", Sam"),
        (" you?! Yes", " you", "?! Yes"),
        (" \"yes.\" Then", " \"yes", ".\" Then"),
        (" (soon). Then", " (soon", "). Then"),
        (" so… Then", " so", "… Then"),
    ];

    let reactions = cases.map(|(suggestion, _, _)| press(&mut engine_showing("I see", suggestion), Key::Tab));

    assert_eq!(reactions, cases.map(|(_, word, rest)| accept(word, rest)));
}

#[test]
fn tab_after_a_word_types_its_closing_punctuation() {
    let mut engine = engine_showing("Best", " regards, Sam");
    press(&mut engine, Key::Tab);

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(reaction, accept(",", " Sam"));
}

#[test]
fn tab_types_a_word_with_signs_inside_it_whole() {
    let cases = [" don't know", " 3.5% more", " e-mail me", " U.S. law"];

    let reactions = cases.map(|suggestion| press(&mut engine_showing("I see", suggestion), Key::Tab));

    assert_eq!(
        reactions,
        [
            accept(" don't", " know"),
            accept(" 3.5%", " more"),
            accept(" e-mail", " me"),
            accept(" U.S", ". law"),
        ]
    );
}

#[test]
fn tab_types_a_part_of_only_punctuation_whole() {
    let cases = [(" ... maybe", " ...", " maybe"), (", see you", ",", " see you")];

    let reactions = cases.map(|(suggestion, _, _)| press(&mut engine_showing("I see", suggestion), Key::Tab));

    assert_eq!(reactions, cases.map(|(_, part, rest)| accept(part, rest)));
}

#[test]
fn tab_passes_to_the_app_while_the_ghost_text_is_not_on_the_screen() {
    let mut engine = engine_showing("hel", "lo big world");
    let desktop = FakeDesktop {
        is_ghost_hidden: true,
        ..FakeDesktop::default()
    };

    let reaction = engine.on_key(Key::Tab, &desktop);

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn tab_accepts_word_with_space_before_it() {
    let mut engine = engine_showing("hello", " big world");

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(reaction, accept(" big", " world"));
}

#[test]
fn tab_after_accepted_word_accepts_following_word() {
    let mut engine = engine_showing("hel", "lo big world");
    press(&mut engine, Key::Tab);

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(reaction, accept(" big", " world"));
}

#[test]
fn tab_on_the_last_part_requests_the_next_suggestion() {
    let mut engine = engine_showing("hel", "lo");

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(reaction, accept_last("lo", request("hello")));
}

#[test]
fn tab_on_the_last_shown_part_of_a_running_completion_waits_for_its_next_words() {
    let mut engine = engine_waiting_for("hel");
    engine.on_partial_completion("lo big");

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(reaction, accept_last("lo", Reaction::Hide));
}

#[test]
fn typing_after_accepted_word_follows_rest() {
    let mut engine = engine_showing("hel", "lo big world");
    press(&mut engine, Key::Tab);

    let reaction = press(&mut engine, Key::Char(' '));

    assert_eq!(reaction, show("big world"));
}

#[test]
fn tab_without_suggestion_passes_through() {
    let mut engine = engine_waiting_for("hel");

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn accepted_word_becomes_context() {
    let mut engine = engine_showing("hel", "lo big world");
    press(&mut engine, Key::Tab);

    let reaction = press(&mut engine, Key::Char('!'));

    assert_eq!(reaction, request("hello!"));
}

#[test]
fn suggestion_is_offered_once_after_accept() {
    let mut engine = engine_showing("hel", "lo");
    press(&mut engine, Key::Tab);

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn backspace_requests_completion_of_shorter_context() {
    let mut engine = engine_showing("hel", "lo");

    let reaction = press(&mut engine, Key::Backspace);

    assert_eq!(reaction, request("he"));
}

#[test]
fn backspace_on_empty_context_hides() {
    let mut engine = Engine::default();

    let reaction = press(&mut engine, Key::Backspace);

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn escape_hides_suggestion() {
    let mut engine = engine_showing("hel", "lo");

    let reaction = press(&mut engine, Key::Escape);

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn tab_after_escape_passes_through() {
    let mut engine = engine_showing("hel", "lo");
    press(&mut engine, Key::Escape);

    let reaction = press(&mut engine, Key::Tab);

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn escape_while_waiting_drops_completion() {
    let mut engine = engine_waiting_for("hel");
    press(&mut engine, Key::Escape);

    let reaction = engine.on_completion(completion("lo"));

    assert_eq!(reaction, Reaction::Hide);
}

#[test]
fn typing_after_escape_while_waiting_requests_again() {
    let mut engine = engine_waiting_for("hel");
    press(&mut engine, Key::Escape);

    let reaction = press(&mut engine, Key::Char('p'));

    assert_eq!(reaction, request("help"));
}

#[test]
fn other_key_clears_context() {
    let mut engine = engine_showing("he", "llo");
    press(&mut engine, Key::Other);

    let reaction = press(&mut engine, Key::Char('l'));

    assert_eq!(reaction, request("l"));
}

#[test]
fn control_character_clears_context() {
    let mut engine = engine_showing("he", "llo");
    press(&mut engine, Key::Char('\r'));

    let reaction = press(&mut engine, Key::Char('l'));

    assert_eq!(reaction, request("l"));
}
