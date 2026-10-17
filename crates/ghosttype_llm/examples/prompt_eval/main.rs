//! Scores the suggestions of a GGUF model on a file of typing cases, through the code that the app
//! uses: the prompt, the model runner and the ghost text.
//!
//! ```text
//! cargo run --release -p ghosttype_llm --example prompt_eval -- \
//!     MODEL.gguf CASES.jsonl [--baseline RESULTS.jsonl] [--save RESULTS.jsonl]
//! ```
//!
//! Each line of the case file is one JSON object with these keys:
//!
//! - `id`: a unique name.
//! - `app`, `window_title`, `field_label`: the place where the writer types, or `null`.
//! - `before`, `after`: the text of the field before and after the caret.
//! - `clipboard`: the text on the clipboard, or `null`.
//! - `screen`: the lines of text near the field, outside of it.
//! - `history`: earlier passages by the same writer.
//! - `next`: the text that the writer typed next.
//!
//! The prompt does not read `field_label`, `screen` and `history` yet. The eval uses the default
//! settings with the clipboard allowed. It opens a field that holds `before` without its last
//! character, types that character and waits for the ghost text. A case matches when the word that
//! the first Tab types is the first word of `next`. Signs after the word do not count. The report
//! gives the score of the cases with the caret between two words, the score of the cases with the
//! caret inside a word, and the median time from the key to the ghost text.
//!
//! `cases/tuning.jsonl` is for the work on the prompt. `cases/held_out.jsonl` only confirms a
//! change at the end. Do not tune the prompt against it.
//!
//! `--save` writes the result of each case. `--baseline` reads the results of an earlier run and
//! names the cases that the change helped or harmed. Compare only runs on the same computer and
//! backend. `baselines/` keeps the results of the main branch for each model.

mod score;

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::time::Instant;

use anyhow::{Context, bail};
use ghosttype_core::app::{App, Field};
use ghosttype_core::desktop::{Desktop, FieldText, Surroundings};
use ghosttype_core::engine::Reaction;
use ghosttype_core::hub::ModelFile;
use ghosttype_core::inference::{
    Completion, InferenceError, InferenceEvent, InferenceEventSink, InferenceRequest, ModelRunner,
};
use ghosttype_core::key::Key;
use ghosttype_core::settings::{ClipboardAccess, Settings};
use ghosttype_llm::LlamaCpp;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::score::{CaseKind, Comparison, Outcome, Summary, is_next_word};

const USAGE: &str =
    "usage: prompt_eval MODEL.gguf CASES.jsonl [--baseline RESULTS.jsonl] [--save RESULTS.jsonl]";

#[derive(Debug, PartialEq, Eq)]
struct Arguments {
    model: PathBuf,
    cases: PathBuf,
    baseline: Option<PathBuf>,
    save: Option<PathBuf>,
}

impl Arguments {
    /// Returns the arguments after the program name, or `None` when they do not follow [`USAGE`].
    fn parse(arguments: &[String]) -> Option<Self> {
        let [model, cases, options @ ..] = arguments else {
            return None;
        };
        let mut parsed = Self {
            model: PathBuf::from(model),
            cases: PathBuf::from(cases),
            baseline: None,
            save: None,
        };
        for option in options.chunks(2) {
            match option {
                [name, path] if name == "--baseline" => parsed.baseline = Some(PathBuf::from(path)),
                [name, path] if name == "--save" => parsed.save = Some(PathBuf::from(path)),
                _ => return None,
            }
        }
        Some(parsed)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    app: Option<String>,
    window_title: Option<String>,
    #[expect(dead_code, reason = "the prompt does not read the field label yet")]
    field_label: Option<String>,
    before: String,
    after: String,
    clipboard: Option<String>,
    #[expect(dead_code, reason = "the prompt does not read the text on the screen yet")]
    screen: Vec<String>,
    #[expect(dead_code, reason = "the prompt does not read earlier writing yet")]
    history: Vec<String>,
    next: String,
}

/// The focused field of a case, before the writer types the last character of `before`.
#[derive(Debug)]
struct CaseField {
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

    fn clipboard_text(&self) -> Option<String> {
        self.clipboard.clone()
    }
}

struct Model {
    runner: LlamaCpp,
    sink: InferenceEventSink,
    events: mpsc::Receiver<InferenceEvent>,
}

impl Model {
    fn load(path: &Path) -> anyhow::Result<Self> {
        let path = path.to_owned();
        let runner = LlamaCpp::new(move |_| Some(path.clone()));
        let (sender, events) = mpsc::channel();
        let sink: InferenceEventSink = Arc::new(move |event| {
            sender.send(event).ok();
        });
        let mut model = Self { runner, sink, events };
        let file = ModelFile {
            repo: "local/model".to_owned(),
            name: "model.gguf".to_owned(),
        };
        model
            .runner
            .submit(InferenceRequest::Load { file, backend: None }, &model.sink);
        match model.events.recv()? {
            InferenceEvent::Loaded { result: Ok(()), .. } => Ok(model),
            event => bail!("the model did not load: {event:?}"),
        }
    }

    fn complete(&mut self, request: InferenceRequest) -> anyhow::Result<Result<Completion, InferenceError>> {
        self.runner.submit(request, &self.sink);
        loop {
            match self.events.recv()? {
                InferenceEvent::Partial(_) => {}
                InferenceEvent::Completed(result) => return Ok(result),
                event @ InferenceEvent::Loaded { .. } => bail!("expected a completion, got {event:?}"),
            }
        }
    }
}

fn run_case(model: &mut Model, case: &Case) -> anyhow::Result<Outcome> {
    let mut before = case.before.clone();
    let last_character = before
        .pop()
        .with_context(|| format!("case {} has no text before the caret", case.id))?;
    let field = CaseField {
        surroundings: Surroundings {
            app_name: case.app.clone(),
            window_title: case.window_title.clone(),
            field_text: Some(FieldText {
                before_caret: before,
                after_caret: case.after.clone(),
            }),
        },
        clipboard: case.clipboard.clone(),
    };
    let mut app = App::new();
    app.use_settings(&Settings {
        clipboard_access: ClipboardAccess::Allowed,
        ..Settings::default()
    });

    let start = Instant::now();
    let reaction = match app.on_key(Key::Char(last_character), &field) {
        Reaction::Request(request) => app.on_completion(model.complete(request)?),
        reaction => reaction,
    };
    let millis = u64::try_from(start.elapsed().as_millis())?;
    let suggestion = match reaction {
        Reaction::Show(text) => Some(text),
        _ => None,
    };
    let accepted = match app.on_key(Key::Tab, &field) {
        Reaction::Accept { text, .. } => text,
        _ => String::new(),
    };

    Ok(Outcome {
        id: case.id.clone(),
        kind: CaseKind::of(&case.before, &case.next),
        suggestion,
        is_match: is_next_word(&accepted, &case.next),
        millis,
    })
}

fn read_json_lines<T: DeserializeOwned>(path: &Path) -> anyhow::Result<Vec<T>> {
    let text = fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            serde_json::from_str(line).with_context(|| format!("line {} of {}", index + 1, path.display()))
        })
        .collect()
}

fn write_json_lines(path: &Path, outcomes: &[Outcome]) -> anyhow::Result<()> {
    let lines = outcomes
        .iter()
        .map(|outcome| serde_json::to_string(outcome).map(|line| line + "\n"))
        .collect::<Result<String, _>>()?;
    fs::write(path, lines).with_context(|| format!("cannot write {}", path.display()))
}

fn write_report(
    output: &mut impl Write,
    summary: &Summary,
    comparison: Option<&Comparison>,
) -> io::Result<()> {
    writeln!(output, "next word:     {}", summary.next_word)?;
    writeln!(output, "partial word:  {}", summary.partial_word)?;
    writeln!(output, "no suggestion: {}", summary.without_suggestion)?;
    writeln!(output, "median time:   {} ms", summary.median_millis)?;
    let Some(comparison) = comparison else {
        return Ok(());
    };
    writeln!(
        output,
        "helped:        {} {:?}",
        comparison.helped.len(),
        comparison.helped
    )?;
    writeln!(
        output,
        "harmed:        {} {:?}",
        comparison.harmed.len(),
        comparison.harmed
    )?;
    writeln!(output, "unchanged:     {}", comparison.unchanged)
}

fn main() -> anyhow::Result<()> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let Some(arguments) = Arguments::parse(&arguments) else {
        bail!("{USAGE}");
    };
    let cases = read_json_lines::<Case>(&arguments.cases)?;
    let baseline = arguments
        .baseline
        .as_deref()
        .map(read_json_lines::<Outcome>)
        .transpose()?;

    let mut model = Model::load(&arguments.model)?;
    let outcomes = cases
        .iter()
        .map(|case| run_case(&mut model, case))
        .collect::<anyhow::Result<Vec<_>>>()?;

    if let Some(path) = &arguments.save {
        write_json_lines(path, &outcomes)?;
    }
    let comparison = baseline.map(|baseline| Comparison::of(&baseline, &outcomes));
    write_report(
        &mut io::stdout().lock(),
        &Summary::of(&outcomes),
        comparison.as_ref(),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Option<Arguments> {
        Arguments::parse(&arguments.iter().map(ToString::to_string).collect::<Vec<_>>())
    }

    #[test]
    fn arguments_without_options_name_the_model_and_the_cases() {
        let arguments = parse(&["model.gguf", "cases.jsonl"]);

        assert_eq!(
            arguments,
            Some(Arguments {
                model: PathBuf::from("model.gguf"),
                cases: PathBuf::from("cases.jsonl"),
                baseline: None,
                save: None,
            })
        );
    }

    #[test]
    fn arguments_with_options_name_the_baseline_and_the_results() {
        let arguments = parse(&[
            "model.gguf",
            "cases.jsonl",
            "--save",
            "new.jsonl",
            "--baseline",
            "old.jsonl",
        ]);

        assert_eq!(
            arguments.map(|arguments| (arguments.baseline, arguments.save)),
            Some((Some(PathBuf::from("old.jsonl")), Some(PathBuf::from("new.jsonl"))))
        );
    }

    #[test]
    fn arguments_that_do_not_follow_the_usage_are_rejected() {
        let cases: [&[&str]; 4] = [
            &["model.gguf"],
            &["model.gguf", "cases.jsonl", "--save"],
            &["model.gguf", "cases.jsonl", "--verbose", "yes"],
            &["model.gguf", "cases.jsonl", "extra.jsonl"],
        ];

        let parsed = cases.map(parse);

        assert_eq!(parsed, cases.map(|_| None));
    }
}
