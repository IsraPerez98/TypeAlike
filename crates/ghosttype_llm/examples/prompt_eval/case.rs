//! The format of a case file and of the stores of the writers, and the field of a case.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

use anyhow::Context;
use ghosttype_core::desktop::{
    Confidence, CopiedText, Desktop, Field, FieldLabels, FieldText, ScreenLine, ScreenRect, ScreenText,
    Surroundings,
};
use ghosttype_core::history::HistoryEntry;
use ghosttype_core::key::Key;
use ghosttype_core::settings::{HistoryApp, HistoryScope, PersonalizationSettings, Settings};
use serde::Deserialize;

use crate::arguments::SharedHistory;
use crate::read_json_lines;
use crate::score::CaseKind;

#[derive(Debug, Deserialize)]
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
    history: Vec<CaseEntry>,
    #[serde(default)]
    pub words: BTreeMap<String, u32>,
    pub next: String,
    #[serde(default)]
    typo: bool,
    #[serde(default)]
    pub fixed: Option<String>,
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

    /// Whether the case has a text before Enter, and `before` has no line break after that text.
    fn is_without_new_line(&self) -> bool {
        self.before_enter.as_deref().is_some_and(|before_enter| {
            self.before
                .strip_prefix(before_enter)
                .is_none_or(|added| !added.contains('\n'))
        })
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
            },
            clipboard: self.clipboard.clone(),
        }
    }
}

/// An earlier text of the writer.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseEntry {
    text: String,
    app_id: String,
    domain: Option<String>,
}

impl CaseEntry {
    fn to_history_entry(&self) -> HistoryEntry {
        HistoryEntry {
            text: self.text.clone(),
            app_id: self.app_id.clone(),
            domain: self.domain.clone(),
        }
    }
}

/// Returns the earlier texts of `case`: the texts of its writer in `writers` without the text that
/// it leaves out, then its own.
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
        .filter(|&(index, _)| Some(index) != case.left_out)
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseScreen {
    field: CaseFrame,
    caret: Option<CaseFrame>,
    lines: Vec<CaseLine>,
}

#[derive(Debug, Deserialize)]
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
    fn history_of_a_case_without_a_writer_is_its_own() {
        let history = history_of(&case_of(None, &["only"]), &BTreeMap::new()).unwrap();

        assert_eq!(texts(&history), ["only"]);
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
