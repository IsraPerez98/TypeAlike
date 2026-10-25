//! The tests of the examples that a prompt shows.

use super::*;
use crate::desktop::{Confidence, ScreenLine, ScreenRect};
use crate::history::store::session_texts;
use crate::history::test_support::{entry, mail, shared};

fn field_in(app_id: &str, domain: Option<&str>, before_caret: &str) -> Surroundings {
    Surroundings {
        app_id: Some(app_id.to_owned()),
        domain: domain.map(str::to_owned),
        field_text: Some(FieldText {
            before_caret: before_caret.to_owned(),
            after_caret: String::new(),
        }),
        ..Surroundings::default()
    }
}

fn examples_with(
    entries: &[HistoryEntry],
    surroundings: &Surroundings,
    screen: Option<&ScreenText>,
    matching_examples: usize,
) -> Vec<String> {
    let texts = session_texts(entries, surroundings, &shared());
    examples(&texts, surroundings, screen, matching_examples)
}

fn examples_in(entries: &[HistoryEntry], surroundings: &Surroundings) -> Vec<String> {
    examples_with(entries, surroundings, None, 2)
}

fn field_with_title(app_id: &str, before_caret: &str, window_title: Option<&str>) -> Surroundings {
    Surroundings {
        window_title: window_title.map(str::to_owned),
        ..field_in(app_id, None, before_caret)
    }
}

fn bluefin_entries() -> Vec<HistoryEntry> {
    vec![
        mail("Bluefin launch plan: the budget goes to the demo."),
        entry("Text of another app", "com.apple.Notes", None),
        mail("Weekend trip to the lake with the kids, back on Sunday."),
        mail("Bluefin budget review with Dana tomorrow morning."),
    ]
}

#[test]
fn examples_are_the_two_texts_of_the_app_that_match_the_text_near_the_caret_oldest_first() {
    let examples = examples_in(
        &bluefin_entries(),
        &field_in("com.apple.mail", None, "Update on the Bluefin budget"),
    );

    assert_eq!(
        examples,
        [
            "Bluefin launch plan: the budget goes to the demo.",
            "Bluefin budget review with Dana tomorrow morning."
        ]
    );
}

#[test]
fn examples_are_the_two_newest_texts_of_the_app_when_no_text_matches() {
    let examples = examples_in(&bluefin_entries(), &field_in("com.apple.mail", None, "Hi Sam"));

    assert_eq!(
        examples,
        [
            "Weekend trip to the lake with the kids, back on Sunday.",
            "Bluefin budget review with Dana tomorrow morning."
        ]
    );
}

fn large_mail_place() -> Vec<HistoryEntry> {
    let mut entries = bluefin_entries();
    entries.extend([
        mail("Dinner with Ana and Leo on Friday at eight."),
        mail("The printer on the second floor works again."),
        entry("Text of another mail app", "OUTLOOK.EXE", None),
    ]);
    entries
}

#[test]
fn one_wanted_match_gives_only_the_best_matching_text_of_a_large_place() {
    let surroundings = field_in("com.apple.mail", None, "Update on the Bluefin budget");

    let examples = examples_with(&large_mail_place(), &surroundings, None, 1);

    assert_eq!(examples, ["Bluefin budget review with Dana tomorrow morning."]);
}

#[test]
fn newest_texts_of_a_small_place_fill_the_second_example() {
    let surroundings = field_in("com.apple.mail", None, "Update on the Bluefin budget");

    let examples = examples_with(&bluefin_entries(), &surroundings, None, 1);

    assert_eq!(
        examples,
        [
            "Weekend trip to the lake with the kids, back on Sunday.",
            "Bluefin budget review with Dana tomorrow morning."
        ]
    );
}

fn notes(text: &str) -> HistoryEntry {
    entry(text, "com.apple.Notes", None)
}

const KESTREL_NOTE: &str = "Kestrel portal: carrier sign-in goes through the Pinwheel gateway.";

#[test]
fn matching_text_of_another_app_fills_a_free_example() {
    let mut entries = large_mail_place();
    entries.push(notes(KESTREL_NOTE));
    let surroundings = field_in(
        "com.apple.mail",
        None,
        "Update on the Kestrel portal: carrier sign-in",
    );

    let examples = examples_with(&entries, &surroundings, None, 1);

    assert_eq!(examples, [KESTREL_NOTE]);
}

#[test]
fn matching_text_of_another_app_never_takes_the_place_of_a_text_of_the_place() {
    let mut entries = large_mail_place();
    entries.push(notes(
        "Update: Bluefin budget review, the Bluefin budget goes to the launch demo.",
    ));
    let surroundings = field_in("com.apple.mail", None, "Update on the Bluefin budget");

    let examples = examples_with(&entries, &surroundings, None, 1);

    assert_eq!(examples, ["Bluefin budget review with Dana tomorrow morning."]);
}

#[test]
fn text_of_another_app_needs_three_topic_words_of_the_field() {
    let mut entries = large_mail_place();
    entries.push(notes("Kestrel portal ideas for the lake trip this week."));
    let surroundings = field_in(
        "com.apple.mail",
        None,
        "Update on the Kestrel portal: carrier sign-in",
    );

    let examples = examples_with(&entries, &surroundings, None, 1);

    assert_eq!(examples, Vec::<&str>::new());
}

#[test]
fn large_place_without_matching_texts_has_no_examples() {
    let examples = examples_in(&large_mail_place(), &field_in("com.apple.mail", None, "Hi Sam"));

    assert_eq!(examples, Vec::<&str>::new());
}

#[test]
fn title_of_the_window_tells_the_topic_and_the_newest_text_fills_the_second_place() {
    let surroundings = field_with_title("com.apple.mail", "Hi Sam", Some("Launch"));

    let examples = examples_in(&bluefin_entries(), &surroundings);

    assert_eq!(
        examples,
        [
            "Bluefin launch plan: the budget goes to the demo.",
            "Bluefin budget review with Dana tomorrow morning."
        ]
    );
}

#[test]
fn text_on_the_screen_tells_the_topic_and_the_newest_text_fills_the_second_place() {
    let screen = ScreenText {
        lines: vec![ScreenLine {
            text: "Launch".to_owned(),
            confidence: Confidence::CERTAIN,
            frame: ScreenRect::default(),
        }],
        ..ScreenText::default()
    };

    let examples = examples_with(
        &bluefin_entries(),
        &field_in("com.apple.mail", None, "Hi Sam"),
        Some(&screen),
        2,
    );

    assert_eq!(
        examples,
        [
            "Bluefin launch plan: the budget goes to the demo.",
            "Bluefin budget review with Dana tomorrow morning."
        ]
    );
}

#[test]
fn rare_word_counts_more_than_a_word_of_many_texts() {
    let entries = [
        mail("Meeting notes on the roof"),
        mail("Meeting notes on the garden"),
        mail("Meeting notes on the garage"),
        mail("A zebra at the zoo"),
        entry("Text of another app", "com.apple.Notes", None),
    ];

    let examples = examples_in(&entries, &field_in("com.apple.mail", None, "meeting zebra"));

    assert_eq!(examples, ["Meeting notes on the garage", "A zebra at the zoo"]);
}

#[test]
fn examples_in_a_browser_come_from_the_same_domain_in_any_browser() {
    let entries = [
        entry("Review from Safari", "com.apple.Safari", Some("github.com")),
        entry(
            "Mail in the same browser",
            "com.google.Chrome",
            Some("mail.google.com"),
        ),
    ];

    let examples = examples_in(
        &entries,
        &field_in("com.google.Chrome", Some("github.com"), "LGTM"),
    );

    assert_eq!(examples, ["Review from Safari"]);
}

#[test]
fn text_of_a_web_page_is_no_example_in_its_browser_outside_of_a_page() {
    let entries = [entry(
        "Review of the pull request",
        "com.google.Chrome",
        Some("github.com"),
    )];

    let examples = examples_in(&entries, &field_in("com.google.Chrome", None, "Hi"));

    assert_eq!(examples, Vec::<&str>::new());
}

#[test]
fn example_keeps_the_start_and_the_end_of_a_long_text_from_a_word() {
    let entries = [mail(&format!("Hola equipo, {}fin", "word ".repeat(120)))];

    let examples = examples_in(&entries, &field_in("com.apple.mail", None, ""));

    assert_eq!(
        examples,
        [format!(
            "Hola equipo, {} … {}fin",
            "word ".repeat(17).trim_end(),
            "word ".repeat(75)
        )]
    );
}

#[test]
fn example_of_a_text_with_wide_characters_keeps_fewer_characters() {
    let entries = [mail(&format!("Hola equipo, {}fin", "ñu ".repeat(200)))];

    let examples = examples_in(&entries, &field_in("com.apple.mail", None, ""));

    assert_eq!(
        examples,
        [format!(
            "Hola equipo, {} … {}fin",
            "ñu ".repeat(17).trim_end(),
            "ñu ".repeat(75)
        )]
    );
}

#[test]
fn example_keeps_all_of_a_text_that_fits() {
    let text = format!("Hola equipo, {}fin", "word ".repeat(90));
    let entries = [mail(&text)];

    let examples = examples_in(&entries, &field_in("com.apple.mail", None, ""));

    assert_eq!(examples, [text]);
}

#[test]
fn place_without_texts_gets_the_newest_text_of_another_app_with_the_same_format() {
    let entries = [
        entry("Chat text of the writer", "net.whatsapp.WhatsApp", None),
        entry("Mail text of the writer", "com.apple.mail", None),
        entry("Newest chat text of the writer", "com.hnc.Discord", None),
        entry("Newest mail text of the writer", "OUTLOOK.EXE", None),
    ];

    let examples = examples_in(&entries, &field_in("com.tinyspeck.slackmacgap", None, "Hi"));

    assert_eq!(examples, ["Newest chat text of the writer"]);
}

#[test]
fn place_without_texts_gets_no_text_of_another_format() {
    let entries = [entry("Mail text of the writer", "com.apple.mail", None)];

    let examples = examples_in(&entries, &field_in("com.tinyspeck.slackmacgap", None, "Hi"));

    assert_eq!(examples, Vec::<&str>::new());
}

#[test]
fn web_page_without_texts_gets_no_text_of_another_web_page() {
    let entries = [entry(
        "Review from Safari",
        "com.apple.Safari",
        Some("github.com"),
    )];

    let examples = examples_in(
        &entries,
        &field_in("com.google.Chrome", Some("news.example.com"), "Hi"),
    );

    assert_eq!(examples, Vec::<&str>::new());
}

#[test]
fn text_that_the_field_holds_is_no_example() {
    let earlier = "Dear Ana,\n\nThe report is ready and the numbers look good.";
    let entries = [mail(earlier), mail("Other text.")];

    let examples = examples_in(
        &entries,
        &field_in("com.apple.mail", None, &format!("{earlier}\n\nBest,")),
    );

    assert_eq!(examples, ["Other text."]);
}

#[test]
fn text_that_holds_a_long_field_is_no_example() {
    let field = "Dear Ana,\n\nThe report is ready and the numbers look";
    let entries = [mail(&format!("{field} good. Best, Tom")), mail("Other text.")];

    let examples = examples_in(&entries, &field_in("com.apple.mail", None, field));

    assert_eq!(examples, ["Other text."]);
}

#[test]
fn two_examples_never_hold_the_same_text() {
    let entries = [
        mail("Dear Ana, the report is ready."),
        mail("Dear Ana, the report"),
    ];

    let examples = examples_in(&entries, &field_in("com.apple.mail", None, "Hi"));

    assert_eq!(examples, ["Dear Ana, the report"]);
}

#[test]
fn place_without_entries_has_no_examples() {
    let entries = [entry("Text of another app", "com.apple.Notes", None)];

    let examples = examples_in(&entries, &field_in("com.apple.mail", None, "Hi"));

    assert_eq!(examples, Vec::<&str>::new());
}

fn chat(text: &str) -> HistoryEntry {
    entry(text, "net.whatsapp.WhatsApp", None)
}

fn chat_field(before_caret: &str) -> Surroundings {
    field_in("net.whatsapp.WhatsApp", None, before_caret)
}

const DINNER: &str = "Dinner with Ana and Leo on Friday at eight.";
const PRINTER: &str = "The printer on the second floor works again.";

#[test]
fn text_with_more_copies_is_the_first_example_among_texts_that_match_the_same() {
    let entries = [
        chat("Hola chiquillas, ¿cómo están?"),
        chat("Hola chiquillas, ¿cómo están?"),
        chat(DINNER),
        chat(PRINTER),
        chat("Hola chiquillas, nos vemos el jueves"),
    ];

    let examples = examples_with(&entries, &chat_field("Hola chiquillas, "), None, 1);

    assert_eq!(examples, ["Hola chiquillas, ¿cómo están?"]);
}

#[test]
fn near_copy_of_a_picked_example_is_no_example_and_the_next_match_takes_its_place() {
    let entries = [
        chat("Vale, nos vemos el lunes en la oficina"),
        chat(DINNER),
        chat(PRINTER),
        chat("Vale, perfecto, nos vemos mañana"),
        chat("Vale, perfeto, nos vemos mañana"),
    ];

    let examples = examples_in(&entries, &chat_field("Vale, "));

    assert_eq!(
        examples,
        [
            "Vale, nos vemos el lunes en la oficina",
            "Vale, perfeto, nos vemos mañana"
        ]
    );
}

#[test]
fn near_copy_with_more_copies_stays_also_when_the_other_text_is_newer() {
    let entries = [
        chat("Vale, perfecto, nos vemos mañana"),
        chat("Vale, perfecto, nos vemos mañana"),
        chat(DINNER),
        chat(PRINTER),
        chat("Vale, perfeto, nos vemos mañana"),
    ];

    let examples = examples_in(&entries, &chat_field("Vale, "));

    assert_eq!(examples, ["Vale, perfecto, nos vemos mañana"]);
}

#[test]
fn two_mails_with_the_same_long_closing_are_both_examples() {
    let closing = "\n\nQuedo atenta a cualquier duda o comentario. Un abrazo grande y que tengan muy buena semana,\nAna";
    let first = format!("Hola equipo, el taller del sábado cambia de sala y empieza a las diez.{closing}");
    let second = format!("Hola equipo, mañana llegan los esmaltes nuevos para la clase del jueves.{closing}");
    let entries = [mail(&first), mail(&second)];

    let examples = examples_in(&entries, &field_in("com.apple.mail", None, "Hi"));

    assert_eq!(examples, [first, second]);
}

#[test]
fn texts_that_differ_in_at_most_two_or_a_tenth_of_their_characters_are_near_copies() {
    let cases = [
        ("Hola chiquillas, ¿cómo están?", "Hola chiquillas, ¿cmo están?", 1),
        (
            "Vale, perfecto, nos vemos mañana",
            "Vale, perfeto, nos vemos mañana",
            1,
        ),
        (
            "thanks so much, see you tomorrow!",
            "thnaks so much, see you tomorow!",
            1,
        ),
        ("Call me at noon", "Call me at moon!", 1),
        ("Call me at noon", "Call me at nine", 2),
    ];

    let counts = cases.map(|(older, newer, _)| {
        examples_in(
            &[mail(older), mail(newer)],
            &field_in("com.apple.mail", None, "Hi"),
        )
        .len()
    });

    assert_eq!(counts, cases.map(|(_, _, count)| count));
}
