//! Tests that a suggestion does not show a copy of a frame line of a preface with all its parts.

use crate::desktop::{Confidence, FieldText, ScreenLine, ScreenRect, ScreenText, Surroundings};
use crate::inference::TokenCount;
use crate::prompt::{Example, LABELS, NotesOrigin, Preface, ScreenPart, Writer, preface_with_screen_lines};
use crate::settings::WriterNotes;

use super::{is_bad_suggestion, words};

/// The frame lines that do not hide in each form of a copy ([`copies_of`]), with the forms that
/// hide:
/// - The age of an earlier text of 1 word in brackets. A copy after its typed opening bracket
///   shows, because the check of the brackets does not look across the caret.
/// - The name line. A writer can type it, as in "Hi, my name is Sara Novak.", so it hides only
///   when it starts the field.
///
/// A new frame line of 1 or 2 words fails the tests until it comes here or gets a rule.
const KNOWN_EXCEPTIONS: [(&str, [bool; 4]); 4] = [
    ("(today) ", [true, true, true, false]),
    ("(yesterday) ", [true, true, true, false]),
    ("My name is Sara Novak.", [true, false, true, true]),
    ("My name is Sara.", [true, false, true, true]),
];
/// The text before a copy of a frame line after the end of a sentence.
const SENTENCE_BEFORE: &str = "Thanks. ";

/// The four forms of a copy of a frame line: the whole line at the start of the field, the whole
/// line after [`SENTENCE_BEFORE`], the line in upper case, and the rest of the line after its
/// typed first word. A line of one word has its first character as the typed part. Each form is
/// the text before the caret and the suggestion.
fn copies_of(line: &str) -> [(String, String); 4] {
    let typed_end = line
        .find(' ')
        .filter(|&space| space + 1 < line.trim_end().len())
        .map_or(1, |space| space + 1);
    let (typed, rest) = line.split_at(typed_end);
    [
        (String::new(), line.to_owned()),
        (SENTENCE_BEFORE.to_owned(), line.to_owned()),
        (String::new(), line.to_uppercase()),
        (typed.to_owned(), rest.to_owned()),
    ]
}

/// Whether each form of a copy of `line` ([`copies_of`]) hides with `preface`.
fn hidden_copies(line: &str, preface: &Preface) -> [bool; 4] {
    copies_of(line).map(|(context, suggestion)| is_bad_suggestion(&suggestion, &context, "", preface))
}

/// Returns the forms of a copy of `line` that hide: those of [`KNOWN_EXCEPTIONS`] for such a line,
/// else all.
fn expected_hidden_copies(line: &str) -> [bool; 4] {
    KNOWN_EXCEPTIONS
        .into_iter()
        .find(|&(known, _)| known == line)
        .map_or([true; 4], |(_, hidden)| hidden)
}

/// Returns the preface with each of its parts, earlier texts of each kind of age, and the writer
/// with the `name` and the default notes.
fn full_preface(name: &str) -> Preface {
    let notes = WriterNotes::default();
    let writer = Writer {
        notes: &notes.0,
        notes_origin: NotesOrigin::Example,
        name,
        examples: [0, 1, 4, 21]
            .map(|days_ago| Example {
                text: "See you on Monday.".to_owned(),
                days_ago: Some(days_ago),
            })
            .to_vec(),
    };
    let surroundings = Surroundings {
        app_name: Some("Mail".to_owned()),
        window_title: Some("Lunch on Friday".to_owned()),
        field_text: Some(FieldText {
            before_caret: "Hi ".to_owned(),
            after_caret: "Best, Ana".to_owned(),
        }),
        ..Surroundings::default()
    };
    let screen = ScreenText {
        lines: [
            ("From: Sam", 300),
            ("See you at 7?", 400),
            ("Sent from my phone", 600),
        ]
        .map(|(text, top)| ScreenLine {
            text: text.to_owned(),
            frame: ScreenRect {
                left: 0,
                top,
                right: 600,
                bottom: top + 20,
            },
            confidence: Confidence::CERTAIN,
        })
        .to_vec(),
        caret: None,
        field: ScreenRect {
            left: 0,
            top: 500,
            right: 600,
            bottom: 540,
        },
    };
    let screen_part = ScreenPart {
        text: &screen,
        max_tokens: TokenCount(300),
        shown_above: &[],
        typed: "",
    };
    preface_with_screen_lines(&writer, &surroundings, Some("1Z999"), Some(screen_part)).0
}

#[test]
fn full_preface_has_each_label_of_the_prompt_as_a_frame_line() {
    let preface = full_preface("Sara Novak");

    let missing = LABELS
        .into_iter()
        .filter(|label| !preface.frame_lines.iter().any(|line| line.trim_end() == *label))
        .collect::<Vec<_>>();

    assert_eq!(missing, Vec::<&str>::new());
}

#[test]
fn each_frame_line_hides_in_each_form_of_a_copy_unless_it_is_a_known_exception() {
    let prefaces = [full_preface("Sara Novak"), full_preface("Sara")];
    let lines = prefaces
        .iter()
        .flat_map(|preface| {
            preface
                .frame_lines
                .iter()
                .map(move |line| (line.as_str(), preface))
        })
        .collect::<Vec<_>>();

    let hidden = lines
        .iter()
        .map(|&(line, preface)| (line, hidden_copies(line, preface)))
        .collect::<Vec<_>>();

    assert_eq!(
        hidden,
        lines
            .iter()
            .map(|&(line, _)| (line, expected_hidden_copies(line)))
            .collect::<Vec<_>>()
    );
}

#[test]
fn each_frame_line_of_one_or_two_words_is_a_known_exception() {
    let preface = full_preface("Sara Novak");

    let unknown = preface
        .frame_lines
        .iter()
        .filter(|line| words(line).count() < 3 && !KNOWN_EXCEPTIONS.iter().any(|&(known, _)| known == *line))
        .collect::<Vec<_>>();

    assert_eq!(unknown, Vec::<&String>::new());
}

#[test]
fn suggestion_with_the_age_of_an_earlier_text_in_brackets_hides() {
    let preface = full_preface("Sara Novak");
    let cases = [
        ("Where:\n", "(today)"),
        ("I sent it ", "(Yesterday) to Tom."),
        ("Notes ", "(2 weeks ago) budget"),
    ];

    let hidden = cases.map(|(context, suggestion)| is_bad_suggestion(suggestion, context, "", &preface));

    assert_eq!(hidden, [true; 3]);
}

#[test]
fn suggestion_with_an_age_without_brackets_or_another_text_in_brackets_shows() {
    let preface = full_preface("Sara Novak");
    let cases = [
        ("I sent it ", "4 days ago to Tom."),
        ("See you ", "(Monday) at noon."),
        ("The report ", "(20 days ago) is late."),
    ];

    let hidden = cases.map(|(context, suggestion)| is_bad_suggestion(suggestion, context, "", &preface));

    assert_eq!(hidden, [false; 3]);
}

#[test]
fn colon_clause_that_shares_three_words_of_a_label_without_a_word_of_its_topic_shows() {
    let preface = full_preface("Sara Novak");
    let cases = [
        ("T", "he part of my plan:"),
        ("T", "he last line of the poem:"),
        ("H", "ere is the text that I am sending:"),
    ];

    let hidden = cases.map(|(context, suggestion)| is_bad_suggestion(suggestion, context, "", &preface));

    assert_eq!(hidden, [false; 3]);
}

#[test]
fn colon_clause_that_shares_three_words_of_a_label_with_a_word_of_its_topic_hides() {
    let preface = full_preface("Sara Novak");
    let cases = [
        ("The ", "text I am typing is:"),
        ("Below are the ", "field on my screen:"),
        ("Here are my ", "notes about me:"),
    ];

    let hidden = cases.map(|(context, suggestion)| is_bad_suggestion(suggestion, context, "", &preface));

    assert_eq!(hidden, [true; 3]);
}
