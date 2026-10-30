//! The format of a case file and of the stores of the writers, and the field of a case.

use std::collections::{BTreeMap, BTreeSet};
use std::iter;
use std::path::Path;
use std::time::Duration;

use anyhow::Context;
use serde::Deserialize;
use typealike_core::desktop::{
    Confidence, CopiedText, Desktop, Field, FieldLabels, FieldText, ScreenLine, ScreenRect, ScreenText,
    Surroundings, Timestamp,
};
use typealike_core::history::{HistoryEntry, reply_words_of};
use typealike_core::key::Key;
use typealike_core::settings::{HistoryApp, HistoryScope, PersonalizationSettings, Settings};

use crate::arguments::{Replay, SharedHistory};
use crate::read_json_lines;
use crate::score::{CaseKind, WordCounts};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub id: String,
    app: Option<String>,
    app_id: Option<String>,
    window_title: Option<String>,
    domain: Option<String>,
    field_caption: Option<String>,
    field_name: Option<String>,
    field_placeholder: Option<String>,
    field_help: Option<String>,
    pub before: String,
    #[serde(default)]
    before_enter: Option<String>,
    after: String,
    clipboard: Option<String>,
    pub screen: Option<CaseScreen>,
    #[serde(default)]
    writer: Option<String>,
    #[serde(default)]
    left_out: Option<usize>,
    /// The seconds since the Unix epoch when the user types the case. The history leaves out the
    /// texts of the store that the writer wrote later. Without it, the case is at the time of the
    /// newest text of its history.
    #[serde(default)]
    now: Option<u64>,
    history: Vec<CaseEntry>,
    #[serde(default)]
    pub words: BTreeMap<String, u32>,
    pub next: String,
    #[serde(default)]
    typo: bool,
    #[serde(default)]
    pub fixed: Option<String>,
    /// The id of the case whose `next` a replay types up to this checkpoint, or `None` when the
    /// eval does not replay the case.
    #[serde(skip)]
    pub reference: Option<String>,
}

impl Case {
    pub fn kind(&self) -> CaseKind {
        if self.typo {
            CaseKind::Typo
        } else if self.is_without_new_line() {
            CaseKind::NoNewLine
        } else {
            CaseKind::of(&self.before, &self.next)
        }
    }

    /// Returns the words of `suggestion` against `next`. When text follows the caret on its line,
    /// `next` is all the text up to it, so a word after `next` is wrong.
    pub fn word_counts(&self, suggestion: &str) -> WordCounts {
        let is_caret_inside_line = self
            .after
            .lines()
            .next()
            .is_some_and(|line| !line.trim().is_empty());
        if is_caret_inside_line {
            WordCounts::in_gap(suggestion, &self.next)
        } else {
            WordCounts::of(suggestion, &self.next)
        }
    }

    /// Returns the checkpoints of the case for `replay`: the case itself, then a case for each
    /// place in `next` that `replay` asks at, with the text of `next` up to that place at the end
    /// of `before`. The id of such a case adds `@` and the count of the characters that it moved.
    /// Each checkpoint has text to type after it, and names the case in `reference`. A typo case, a
    /// case with a text before Enter and a case without a next word keep only the case itself,
    /// without a reference.
    pub fn checkpoints(&self, replay: Replay) -> Vec<Self> {
        let is_replayed = replay != Replay::Off
            && !self.typo
            && self.before_enter.is_none()
            && !self.next.trim().is_empty();
        if !is_replayed {
            return vec![self.clone()];
        }
        let reference = Some(self.id.clone());
        let checkpoint_at = |place: Place| Self {
            id: format!("{}@{}", self.id, place.typed),
            before: format!("{}{}", self.before, &self.next[..place.offset]),
            next: self.next[place.offset..].to_owned(),
            reference: reference.clone(),
            ..self.clone()
        };
        iter::once(Self {
            reference: reference.clone(),
            ..self.clone()
        })
        .chain(replay_places(&self.next, replay).map(checkpoint_at))
        .collect()
    }

    /// Whether the case has a text before Enter, and `before` has no line break after that text.
    fn is_without_new_line(&self) -> bool {
        self.before_enter.as_deref().is_some_and(|before_enter| {
            self.before
                .strip_prefix(before_enter)
                .is_none_or(|added| !added.contains('\n'))
        })
    }

    /// Returns the case at its time: its `now`, else the time of the newest text of `history`.
    pub fn at_time_of(&self, history: &[HistoryEntry]) -> Self {
        let newest = history.iter().filter_map(|entry| entry.written_at).max();
        Self {
            now: self.now.or(newest.map(|written_at| written_at.0)),
            ..self.clone()
        }
    }

    /// Returns the text before the caret before the last key, and the last key.
    pub fn last_key(&self) -> anyhow::Result<(String, Key)> {
        if let Some(before_enter) = &self.before_enter {
            return Ok((before_enter.clone(), Key::Enter));
        }
        if let Some(before_enter) = self.before.strip_suffix('\n') {
            return Ok((before_enter.to_owned(), Key::Enter));
        }
        let mut before = self.before.clone();
        let last_character = before
            .pop()
            .with_context(|| format!("case {} has no text before the caret", self.id))?;
        Ok((before, Key::Char(last_character)))
    }

    /// Returns the field of the case with `before_caret` as the text before the caret.
    pub fn field(&self, before_caret: String) -> CaseField {
        CaseField {
            surroundings: Surroundings {
                app_name: self.app.clone(),
                app_id: self.app_id.clone(),
                window_title: self.window_title.clone(),
                field_labels: FieldLabels {
                    caption: self.field_caption.clone(),
                    name: self.field_name.clone(),
                    placeholder: self.field_placeholder.clone(),
                    help: self.field_help.clone(),
                },
                domain: self.domain.clone(),
                app_path: None,
                field_text: Some(FieldText {
                    before_caret,
                    after_caret: self.after.clone(),
                }),
                is_caret_line_hidden: false,
                is_search_field: false,
            },
            clipboard: self.clipboard.clone(),
            now: self.now.map(Timestamp),
        }
    }
}

/// A place inside the `next` text of a case.
#[derive(Debug, Clone, Copy)]
struct Place {
    /// The characters of `next` before the place.
    typed: usize,
    /// The byte offset of the place in `next`.
    offset: usize,
}

/// Returns the places inside `next` where `replay` asks for a suggestion. Each place has text
/// after it that is not only whitespace.
fn replay_places(next: &str, replay: Replay) -> impl Iterator<Item = Place> {
    next.char_indices().zip(next.chars().skip(1)).zip(1..).filter_map(
        move |(((offset, before), after), typed)| {
            let place = Place {
                typed,
                offset: offset + before.len_utf8(),
            };
            let has_text_after = !next[place.offset..].trim().is_empty();
            (has_text_after && asks_between(replay, before, after)).then_some(place)
        },
    )
}

/// Whether `replay` asks for a suggestion between `before` and `after`, two characters next to
/// each other in `next`.
fn asks_between(replay: Replay, before: char, after: char) -> bool {
    match replay {
        Replay::Off => false,
        Replay::Word => before.is_whitespace() && !after.is_whitespace(),
        Replay::Character => true,
    }
}

/// An earlier text of the writer.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseEntry {
    text: String,
    app_id: String,
    domain: Option<String>,
    /// The seconds since the Unix epoch when the writer wrote the text, if known.
    #[serde(default)]
    written_at: Option<u64>,
    /// The message that the text answers, if any.
    #[serde(default)]
    reply_to: Option<String>,
}

impl CaseEntry {
    fn to_history_entry(&self) -> HistoryEntry {
        HistoryEntry {
            text: self.text.clone(),
            app_id: self.app_id.clone(),
            domain: self.domain.clone(),
            written_at: self.written_at.map(Timestamp),
            reply_to: self.reply_to.as_deref().map(reply_words_of).unwrap_or_default(),
        }
    }

    /// Whether the writer wrote the text after `now`.
    fn is_after(&self, now: Option<u64>) -> bool {
        self.written_at
            .zip(now)
            .is_some_and(|(written_at, now)| written_at > now)
    }
}

/// Returns the earlier texts of `case`: the texts of its writer in `writers` without the text that
/// it leaves out and the texts written after the time of the case, then its own.
pub fn history_of(
    case: &Case,
    writers: &BTreeMap<String, Vec<CaseEntry>>,
) -> anyhow::Result<Vec<HistoryEntry>> {
    let store = match &case.writer {
        Some(name) => writers
            .get(name)
            .with_context(|| format!("case {} names the unknown writer {name}", case.id))?
            .as_slice(),
        None => &[],
    };
    Ok(store
        .iter()
        .enumerate()
        .filter(|&(index, entry)| Some(index) != case.left_out && !entry.is_after(case.now))
        .map(|(_, entry)| entry)
        .chain(&case.history)
        .map(CaseEntry::to_history_entry)
        .collect())
}

/// Returns `settings` where the app of `case` and each app of `history` has the scope that `shared`
/// gives.
pub fn settings_for_case(
    settings: &Settings,
    case: &Case,
    history: &[HistoryEntry],
    shared: SharedHistory,
) -> Settings {
    let scope = match shared {
        SharedHistory::On => HistoryScope::AllApps,
        SharedHistory::Off => HistoryScope::ThisApp,
    };
    let app_ids = history
        .iter()
        .map(|entry| entry.app_id.as_str())
        .chain(case.app_id.as_deref())
        .collect::<BTreeSet<_>>();
    let apps = app_ids
        .into_iter()
        .map(|app_id| HistoryApp {
            app_id: app_id.to_owned(),
            app_name: app_id.to_owned(),
            app_path: None,
            scope,
        })
        .collect();
    Settings {
        personalization: PersonalizationSettings {
            apps,
            ..settings.personalization.clone()
        },
        ..settings.clone()
    }
}

/// Reads the store of each writer that `cases` name, from the `writers` folder in `directory`.
pub fn read_writers(directory: &Path, cases: &[Case]) -> anyhow::Result<BTreeMap<String, Vec<CaseEntry>>> {
    let mut writers = BTreeMap::new();
    for name in cases.iter().filter_map(|case| case.writer.as_ref()) {
        if !writers.contains_key(name) {
            let entries = read_json_lines(&directory.join("writers").join(format!("{name}.jsonl")))?;
            writers.insert(name.clone(), entries);
        }
    }
    Ok(writers)
}

/// A frame as `[left, top, right, bottom]`.
type CaseFrame = [i32; 4];

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseScreen {
    field: CaseFrame,
    caret: Option<CaseFrame>,
    lines: Vec<CaseLine>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseLine {
    text: String,
    frame: CaseFrame,
}

impl CaseScreen {
    pub fn to_screen_text(&self) -> ScreenText {
        ScreenText {
            lines: self
                .lines
                .iter()
                .map(|line| ScreenLine {
                    text: line.text.clone(),
                    confidence: Confidence::CERTAIN,
                    frame: screen_rect(line.frame),
                })
                .collect(),
            caret: self.caret.map(screen_rect),
            field: screen_rect(self.field),
        }
    }
}

fn screen_rect([left, top, right, bottom]: CaseFrame) -> ScreenRect {
    ScreenRect {
        left,
        top,
        right,
        bottom,
    }
}

/// The focused field of a case.
#[derive(Debug)]
pub struct CaseField {
    surroundings: Surroundings,
    clipboard: Option<String>,
    now: Option<Timestamp>,
}

impl Desktop for CaseField {
    fn field(&self) -> Field {
        Field::Open
    }

    fn surroundings(&self) -> Surroundings {
        self.surroundings.clone()
    }

    fn copied_text(&self) -> Option<CopiedText> {
        Some(CopiedText {
            text: self.clipboard.clone()?,
            age: Some(Duration::ZERO),
        })
    }

    fn now(&self) -> Option<Timestamp> {
        self.now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case_of(writer: Option<&str>, texts: &[&str]) -> Case {
        let history = texts
            .iter()
            .map(|text| format!(r#"{{"text": "{text}", "app_id": "mail", "domain": null}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let writer = writer.map_or("null".to_owned(), |name| format!(r#""{name}""#));
        serde_json::from_str(&format!(
            r#"{{"id": "case", "app": null, "app_id": null, "window_title": null, "domain": null,
                "before": "a", "after": "", "clipboard": null, "screen": null, "writer": {writer},
                "history": [{history}], "next": "b"}}"#
        ))
        .unwrap()
    }

    fn texts(entries: &[HistoryEntry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.text.as_str()).collect()
    }

    #[test]
    fn history_of_a_case_adds_its_texts_after_the_store_of_its_writer() {
        let store = case_of(None, &["old one", "old two"]).history;
        let writers = BTreeMap::from([("ana".to_owned(), store)]);

        let history = history_of(&case_of(Some("ana"), &["new"]), &writers).unwrap();

        assert_eq!(texts(&history), ["old one", "old two", "new"]);
    }

    #[test]
    fn history_of_a_case_leaves_out_the_text_of_the_store_that_it_names() {
        let store = case_of(None, &["old one", "old two", "old three"]).history;
        let writers = BTreeMap::from([("ana".to_owned(), store)]);
        let case = Case {
            left_out: Some(1),
            ..case_of(Some("ana"), &[])
        };

        let history = history_of(&case, &writers).unwrap();

        assert_eq!(texts(&history), ["old one", "old three"]);
    }

    #[test]
    fn history_of_a_case_leaves_out_the_texts_of_the_store_written_after_its_time() {
        let store = dated_texts(&[("before", 100), ("at", 200), ("after", 300)]);
        let writers = BTreeMap::from([("ana".to_owned(), store)]);
        let case = Case {
            now: Some(200),
            ..case_of(Some("ana"), &[])
        };

        let history = history_of(&case, &writers).unwrap();

        assert_eq!(texts(&history), ["before", "at"]);
    }

    #[test]
    fn case_without_a_time_is_at_the_time_of_the_newest_text_of_its_history() {
        let store = dated_texts(&[("older", 300), ("newer", 200)]);
        let writers = BTreeMap::from([("ana".to_owned(), store)]);
        let cases = [
            case_of(Some("ana"), &[]),
            Case {
                now: Some(250),
                ..case_of(Some("ana"), &[])
            },
        ];

        let times = cases.map(|case| case.at_time_of(&history_of(&case, &writers).unwrap()).now);

        assert_eq!(times, [Some(300), Some(250)]);
    }

    fn dated_texts(texts: &[(&str, u64)]) -> Vec<CaseEntry> {
        texts
            .iter()
            .map(|&(text, written_at)| CaseEntry {
                text: text.to_owned(),
                app_id: "mail".to_owned(),
                domain: None,
                written_at: Some(written_at),
                reply_to: None,
            })
            .collect()
    }

    #[test]
    fn earlier_text_keeps_the_words_of_its_answered_message() {
        let case: Case = serde_json::from_str(
            r#"{"id": "case", "app": null, "app_id": null, "window_title": null, "domain": null,
                "before": "a", "after": "", "clipboard": null, "screen": null, "next": "b",
                "history": [{"text": "omw", "app_id": "chat", "domain": null, "reply_to": "Standup in 5?"}]}"#,
        )
        .unwrap();

        let history = history_of(&case, &BTreeMap::new()).unwrap();

        assert_eq!(history[0].reply_to, ["standup"]);
    }

    #[test]
    fn history_of_a_case_without_a_writer_is_its_own() {
        let history = history_of(&case_of(None, &["only"]), &BTreeMap::new()).unwrap();

        assert_eq!(texts(&history), ["only"]);
    }

    #[test]
    fn suggestion_word_after_next_is_wrong_only_with_text_after_the_caret_on_its_line() {
        let cases = [(" of the shade?", 1), ("\nBest,\nNora", 0), ("", 0)];

        let wrong = cases.map(|(after, _)| {
            let case = Case {
                after: after.to_owned(),
                ..case_of(None, &[])
            };
            case.word_counts("b and").wrong
        });

        assert_eq!(wrong, cases.map(|(_, wrong)| wrong));
    }

    #[test]
    fn case_with_a_typo_is_a_typo_case_wherever_the_caret_is() {
        let case = Case {
            typo: true,
            ..case_of(None, &[])
        };

        assert_eq!(case.kind(), CaseKind::Typo);
    }

    fn case_before(before: &str, before_enter: Option<&str>) -> Case {
        Case {
            before: before.to_owned(),
            before_enter: before_enter.map(str::to_owned),
            ..case_of(None, &[])
        }
    }

    #[test]
    fn case_that_ends_with_a_line_break_presses_enter_after_the_text_before_it() {
        let case = case_before("Best regards,\n", None);

        assert_eq!(case.last_key().unwrap(), ("Best regards,".to_owned(), Key::Enter));
    }

    #[test]
    fn case_with_the_text_before_enter_presses_enter_after_that_text() {
        let case = case_before("- milk\n- ", Some("- milk"));

        assert_eq!(case.last_key().unwrap(), ("- milk".to_owned(), Key::Enter));
    }

    #[test]
    fn other_case_types_its_last_character() {
        let case = case_before("Best", None);

        assert_eq!(case.last_key().unwrap(), ("Bes".to_owned(), Key::Char('t')));
    }

    #[test]
    fn case_whose_enter_adds_no_line_break_expects_no_suggestion() {
        let cases = [
            case_before("", Some("See you at 8")),
            case_before("- milk\n- ", Some("- milk")),
        ];

        let kinds = cases.map(|case| case.kind());

        assert_eq!(kinds, [CaseKind::NoNewLine, CaseKind::NextWord]);
    }

    fn case_with_next(before: &str, next: &str) -> Case {
        Case {
            before: before.to_owned(),
            next: next.to_owned(),
            ..case_of(None, &[])
        }
    }

    fn places(checkpoints: &[Case]) -> Vec<(&str, &str, &str)> {
        checkpoints
            .iter()
            .map(|checkpoint| {
                (
                    checkpoint.id.as_str(),
                    checkpoint.before.as_str(),
                    checkpoint.next.as_str(),
                )
            })
            .collect()
    }

    #[test]
    fn word_replay_asks_at_the_caret_of_the_case_and_at_the_start_of_each_word_of_next() {
        let case = case_with_next("Thanks for the", " meeting at noon");

        let checkpoints = case.checkpoints(Replay::Word);

        assert_eq!(
            places(&checkpoints),
            [
                ("case", "Thanks for the", " meeting at noon"),
                ("case@1", "Thanks for the ", "meeting at noon"),
                ("case@9", "Thanks for the meeting ", "at noon"),
                ("case@12", "Thanks for the meeting at ", "noon"),
            ]
        );
    }

    #[test]
    fn character_replay_asks_after_each_character_of_next_that_leaves_text_to_type() {
        let case = case_with_next("Se", "ñor ");

        let checkpoints = case.checkpoints(Replay::Character);

        assert_eq!(
            places(&checkpoints),
            [
                ("case", "Se", "ñor "),
                ("case@1", "Señ", "or "),
                ("case@2", "Seño", "r "),
            ]
        );
    }

    #[test]
    fn each_checkpoint_of_a_replay_names_the_case_that_it_replays() {
        let case = case_with_next("a", " b c");

        let checkpoints = case.checkpoints(Replay::Word);

        assert_eq!(
            checkpoints
                .iter()
                .map(|checkpoint| checkpoint.reference.as_deref())
                .collect::<Vec<_>>(),
            [Some("case"), Some("case"), Some("case")]
        );
    }

    #[test]
    fn case_that_the_eval_does_not_replay_is_its_only_checkpoint() {
        let cases = [
            (Replay::Off, case_with_next("a", " b c")),
            (
                Replay::Character,
                Case {
                    typo: true,
                    ..case_with_next("the dle", "ivery soon")
                },
            ),
            (Replay::Character, case_before("- milk\n- ", Some("- milk"))),
            (Replay::Character, case_with_next("See you", " ")),
        ];

        let checkpoints = cases.each_ref().map(|(replay, case)| {
            case.checkpoints(*replay)
                .into_iter()
                .map(|checkpoint| (checkpoint.id, checkpoint.reference))
                .collect::<Vec<_>>()
        });

        assert_eq!(
            checkpoints,
            cases.each_ref().map(|_| vec![("case".to_owned(), None)])
        );
    }

    #[test]
    fn case_with_an_unknown_writer_is_an_error() {
        let history = history_of(&case_of(Some("bob"), &[]), &BTreeMap::new());

        assert!(history.is_err());
    }

    #[test]
    fn each_app_of_a_case_and_of_its_history_gets_the_scope_of_the_option() {
        let mut case = case_of(None, &["one", "two"]);
        case.app_id = Some("com.apple.mail".to_owned());
        let history = [
            HistoryEntry {
                app_id: "com.apple.Notes".to_owned(),
                ..case.history[0].to_history_entry()
            },
            case.history[1].to_history_entry(),
        ];

        let settings = settings_for_case(&Settings::default(), &case, &history, SharedHistory::Off);

        assert_eq!(
            settings
                .personalization
                .apps
                .iter()
                .map(|app| (app.app_id.as_str(), app.scope))
                .collect::<Vec<_>>(),
            [
                ("com.apple.Notes", HistoryScope::ThisApp),
                ("com.apple.mail", HistoryScope::ThisApp),
                ("mail", HistoryScope::ThisApp)
            ]
        );
    }
}
