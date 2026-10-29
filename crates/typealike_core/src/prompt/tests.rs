//! Tests of the prompt and its preface.

use super::*;
use crate::desktop::{Confidence, FieldLabels, FieldText, ScreenLine, ScreenRect};

fn notes(text: &str) -> Writer<'_> {
    Writer {
        notes: text,
        ..Writer::default()
    }
}

fn examples<'text>(examples: &[&str]) -> Writer<'text> {
    Writer {
        examples: examples.iter().map(|text| example(text)).collect(),
        ..Writer::default()
    }
}

fn example(text: &str) -> Example {
    Example {
        text: text.to_owned(),
        days_ago: None,
    }
}

/// Returns the preface that has `lines` after the frame.
fn framed(lines: &str) -> String {
    format!("{FRAME}\n{lines}")
}

/// Returns surroundings whose place line leaves `tokens` of the preface for the parts after
/// it.
fn surroundings_leaving(tokens: u32) -> Surroundings {
    let place_tokens = PREFACE_TOKENS.0 - estimated_tokens(FRAME).0 - tokens;
    let app_name = "a".repeat(3 * usize::try_from(place_tokens).unwrap() - "App: ".len());
    surroundings(Some(&app_name), None)
}

fn named_field(name: &str) -> FieldLabels {
    FieldLabels {
        name: Some(name.to_owned()),
        ..FieldLabels::default()
    }
}

fn surroundings(app_name: Option<&str>, window_title: Option<&str>) -> Surroundings {
    Surroundings {
        app_name: app_name.map(str::to_owned),
        window_title: window_title.map(str::to_owned),
        ..Surroundings::default()
    }
}

/// Returns `character` as many times as fit in `tokens`, then `extra` times more.
fn ascii_text(character: char, tokens: TokenCount, extra: usize) -> String {
    character
        .to_string()
        .repeat(3 * usize::try_from(tokens.0).unwrap() + extra)
}

fn with_text_after(text: &str) -> Surroundings {
    Surroundings {
        field_text: Some(FieldText {
            before_caret: "Hi ".to_owned(),
            after_caret: text.to_owned(),
        }),
        ..Surroundings::default()
    }
}

#[test]
fn debug_of_the_writer_hides_the_text() {
    let writer = Writer {
        notes: "I am a nurse.",
        name: "Sara Novak",
        examples: vec![example("Cheers,\nTom")],
    };

    let debug = format!("{writer:?}");

    assert_eq!(debug, "Writer { notes: 13, name: 10, examples: 1 }");
}

#[test]
fn preface_goes_from_the_frame_to_text_after_the_caret() {
    let surroundings = Surroundings {
        app_id: Some("com.apple.mail".to_owned()),
        field_labels: named_field("Message"),
        field_text: Some(FieldText {
            before_caret: "Hi ".to_owned(),
            after_caret: "\n\n> Priya wrote\n".to_owned(),
        }),
        ..surroundings(Some("Mail"), Some("Re: Budget"))
    };

    let preface = preface(&notes(" I am a nurse. "), &surroundings, Some(" 1Z999 "), None);

    assert_eq!(
        preface,
        framed(
            "Notes about me: “I am a nurse.”\nWhere I am typing: Format: email; App: Mail; Title: Re: Budget; Field: Message\nText on my clipboard: “1Z999”\nThe text that comes right after the part that I am typing now: “> Priya wrote”"
        )
    );
}

#[test]
fn empty_parts_are_left_out() {
    let writer = Writer {
        notes: " \n ",
        name: " ",
        ..Writer::default()
    };

    let preface = preface(&writer, &with_text_after(" \n "), Some(" "), None);

    assert_eq!(preface, FRAME);
}

#[test]
fn name_of_the_writer_comes_after_the_notes_on_one_line() {
    let writer = Writer {
        notes: "I am a nurse.",
        name: "  Sara \n Novak ",
        ..Writer::default()
    };

    let preface = preface(&writer, &Surroundings::default(), None, None);

    assert_eq!(
        preface,
        framed("Notes about me: “I am a nurse.”\nMy name is Sara Novak.")
    );
}

#[test]
fn notes_stay_on_one_line() {
    let preface = preface(
        &notes("I am a nurse.\n\n  I write in Spanish."),
        &Surroundings::default(),
        None,
        None,
    );

    assert_eq!(
        preface,
        framed("Notes about me: “I am a nurse. I write in Spanish.”")
    );
}

#[test]
fn long_notes_keep_their_start() {
    let long_notes = ascii_text('n', NOTES_TOKENS, 1);

    let preface = preface(&notes(&long_notes), &Surroundings::default(), None, None);

    assert_eq!(
        preface,
        framed(&format!("Notes about me: “{}”", ascii_text('n', NOTES_TOKENS, 0)))
    );
}

#[test]
fn place_names_the_app_and_the_window() {
    let cases = [
        (
            Some("Mail"),
            Some("Re: Budget"),
            "Where I am typing: App: Mail; Title: Re: Budget",
        ),
        (
            Some("Slack"),
            Some("#design - Acme - Slack"),
            "Where I am typing: Title: #design - Acme - Slack",
        ),
        (Some("Notes"), None, "Where I am typing: App: Notes"),
        (None, Some("Re: Budget"), "Where I am typing: Title: Re: Budget"),
        (
            Some(" "),
            Some("Re: Budget"),
            "Where I am typing: Title: Re: Budget",
        ),
    ];

    let prefaces =
        cases.map(|(app, window, _)| preface(&Writer::default(), &surroundings(app, window), None, None));

    assert_eq!(prefaces, cases.map(|(_, _, place)| framed(place)));
}

#[test]
fn web_page_gives_the_format_of_the_place() {
    let surroundings = Surroundings {
        app_id: Some("com.google.Chrome".to_owned()),
        domain: Some("mail.google.com".to_owned()),
        field_labels: named_field("Message Body"),
        ..surroundings(Some("Google Chrome"), Some("Inbox - Gmail - Google Chrome"))
    };

    let preface = preface(&Writer::default(), &surroundings, None, None);

    assert_eq!(
        preface,
        framed("Where I am typing: Format: email; Title: Inbox - Gmail - Google Chrome; Field: Message Body")
    );
}

#[test]
fn other_labels_of_the_field_give_a_hint() {
    let surroundings = Surroundings {
        field_labels: FieldLabels {
            caption: Some("Message".to_owned()),
            name: Some("message".to_owned()),
            placeholder: Some("Message Priya Shah".to_owned()),
            help: Some("Shift+Enter adds a line".to_owned()),
        },
        ..surroundings(None, Some("Priya Shah"))
    };

    let preface = preface(&Writer::default(), &surroundings, None, None);

    assert_eq!(
        preface,
        framed(
            "Where I am typing: Title: Priya Shah; Field: Message; Hint: Message Priya Shah, Shift+Enter adds a line"
        )
    );
}

#[test]
fn place_stays_on_one_line() {
    let surroundings = Surroundings {
        field_labels: named_field(" Message\n  body "),
        ..surroundings(None, Some("Re:\tBudget"))
    };

    let preface = preface(&Writer::default(), &surroundings, None, None);

    assert_eq!(
        preface,
        framed("Where I am typing: Title: Re: Budget; Field: Message body")
    );
}

#[test]
fn long_parts_keep_their_start() {
    let title = ascii_text('t', WINDOW_TITLE_TOKENS, 1);
    let label = ascii_text('l', FIELD_LABEL_TOKENS, 1);
    let hint = ascii_text('h', FIELD_LABEL_TOKENS, 1);
    let clipboard = ascii_text('c', CLIPBOARD_TOKENS, 1);
    let after = ascii_text('a', AFTER_CARET_TOKENS, 1);
    let surroundings = Surroundings {
        window_title: Some(title),
        field_labels: FieldLabels {
            name: Some(label),
            placeholder: Some(hint),
            ..FieldLabels::default()
        },
        ..with_text_after(&after)
    };

    let preface = preface(&Writer::default(), &surroundings, Some(&clipboard), None);

    assert_eq!(
        preface,
        framed(&format!(
            "Where I am typing: Title: {}; Field: {}; Hint: {}\nText on my clipboard: “{}”\nThe text that comes right after the part that I am typing now: “{}”",
            ascii_text('t', WINDOW_TITLE_TOKENS, 0),
            ascii_text('l', FIELD_LABEL_TOKENS, 0),
            ascii_text('h', FIELD_LABEL_TOKENS, 0),
            ascii_text('c', CLIPBOARD_TOKENS, 0),
            ascii_text('a', AFTER_CARET_TOKENS, 0)
        ))
    );
}

fn screen(lines: &[(&str, i32)]) -> ScreenText {
    let line = |&(text, top): &(&str, i32)| ScreenLine {
        text: text.to_owned(),
        confidence: Confidence::CERTAIN,
        frame: ScreenRect {
            left: 0,
            top,
            right: 600,
            bottom: top + 20,
        },
    };
    ScreenText {
        lines: lines.iter().map(line).collect(),
        caret: None,
        field: ScreenRect {
            left: 0,
            top: 500,
            right: 600,
            bottom: 540,
        },
    }
}

#[test]
fn text_after_the_caret_goes_last_after_the_screen_text_below_and_above_the_field() {
    let screen = screen(&[
        ("See you at 7?", 400),
        ("Sent from my phone", 600),
        ("From: Sam", 300),
    ]);

    let preface = preface(
        &Writer::default(),
        &with_text_after("Best, Ana"),
        Some("1Z999"),
        Some(&screen),
    );

    assert_eq!(
        preface,
        framed(
            "Text on my clipboard: “1Z999”\nBelow the field on my screen: “Sent from my phone”\nAbove the field on my screen: “From: Sam\nSee you at 7?”\nThe text that comes right after the part that I am typing now: “Best, Ana”"
        )
    );
}

#[test]
fn screen_text_fits_in_what_the_preface_has_left() {
    let screen = screen(&[("Dinner on Saturday?", 300), ("Hi", 480)]);

    let preface = preface(&Writer::default(), &surroundings_leaving(2), None, Some(&screen));

    assert!(
        preface.ends_with("\nAbove the field on my screen: “Hi”"),
        "{preface}"
    );
}

#[test]
fn screen_text_keeps_whole_lines() {
    let screen = screen(&[("abc", 400), ("def", 420)]);

    let preface = preface(&Writer::default(), &surroundings_leaving(2), None, Some(&screen));

    assert!(
        preface.ends_with("\nAbove the field on my screen: “abc\ndef”"),
        "{preface}"
    );
}

#[test]
fn screen_without_lines_adds_nothing() {
    let preface = preface(
        &Writer::default(),
        &Surroundings::default(),
        None,
        Some(&screen(&[])),
    );

    assert_eq!(preface, FRAME);
}

#[test]
fn earlier_writing_goes_between_the_place_and_the_clipboard() {
    let surroundings = Surroundings {
        field_text: Some(FieldText {
            before_caret: "Hi ".to_owned(),
            after_caret: "Best, Ana".to_owned(),
        }),
        ..surroundings(Some("Mail"), None)
    };

    let preface = preface(
        &examples(&["Cheers,\nTom", "See you soon!"]),
        &surroundings,
        Some("1Z999"),
        None,
    );

    assert_eq!(
        preface,
        framed(
            "Where I am typing: App: Mail\nMy earlier writing:\n“Cheers,\nTom”\n“See you soon!”\nText on my clipboard: “1Z999”\nThe text that comes right after the part that I am typing now: “Best, Ana”"
        )
    );
}

#[test]
fn example_tells_how_many_days_ago_the_writer_wrote_it() {
    let writer = Writer {
        examples: [0, 1, 4, 20]
            .map(|days_ago| Example {
                text: "See you soon!".to_owned(),
                days_ago: Some(days_ago),
            })
            .to_vec(),
        ..Writer::default()
    };

    let preface = preface(&writer, &Surroundings::default(), None, None);

    assert_eq!(
        preface,
        framed(
            "My earlier writing:\n(today) “See you soon!”\n(yesterday) “See you soon!”\n(4 days ago) “See you soon!”\n(2 weeks ago) “See you soon!”"
        )
    );
}

#[test]
fn earlier_writing_drops_the_older_examples_that_do_not_fit() {
    let newest = ascii_text('n', HISTORY_TOKENS, 0);
    let older = format!("{} end", "word ".repeat(30));

    let preface = preface(
        &examples(&[&older, "first words", &newest]),
        &Surroundings::default(),
        None,
        None,
    );

    assert_eq!(preface, framed(&format!("My earlier writing:\n“{newest}”")));
}

#[test]
fn earlier_writing_cuts_an_example_at_the_start_of_a_word() {
    let newest = ascii_text('n', TokenCount(HISTORY_TOKENS.0 - 4), 0);

    let preface = preface(
        &examples(&["one two three", "end of the example", &newest]),
        &Surroundings::default(),
        None,
        None,
    );

    assert_eq!(
        preface,
        framed(&format!("My earlier writing:\n“the example”\n“{newest}”"))
    );
}

#[test]
fn prompt_puts_a_blank_line_and_a_heading_between_preface_and_text() {
    let cases = [
        (
            "Dear Sir",
            "Formal.\n\nThe part of my writing that I am typing now:\nDear Sir",
        ),
        ("", "Formal.\n\nThe part of my writing that I am typing now:\n"),
    ];

    let prompts = cases.map(|(text_before, _)| prompt("Formal.", text_before));

    assert_eq!(prompts, cases.map(|(_, prompt)| Prompt(prompt.to_owned())));
}
