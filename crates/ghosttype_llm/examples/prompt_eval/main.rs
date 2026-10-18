//! Scores the suggestions of a GGUF model on a file of typing cases, through the code that the app
//! uses: the prompt, the model runner and the ghost text.
//!
//! ```text
//! cargo run --release -p ghosttype_llm --example prompt_eval -- \
//!     MODEL.gguf CASES.jsonl [--baseline RESULTS.jsonl] [--save RESULTS.jsonl] [--notes TEXT] \
//!     [--backend BACKEND]
//! ```
//!
//! Each line of the case file is one JSON object with these keys:
//!
//! - `id`: a unique name.
//! - `app`, `app_id`, `window_title`, `domain`, `field_label`: the place where the writer types,
//!   or `null`. `app_id` is a bundle identifier on macOS or a program file name on Windows.
//!   `domain` is the host of the web page.
//! - `before`, `after`: the text of the field before and after the caret.
//! - `clipboard`: the text on the clipboard, or `null`.
//! - `screen`: the text near the field on the screen, or `null`: `field` and `caret` (or `null`)
//!   are the frames of the field and the caret, and each of `lines` has a `text` outside of the
//!   field and its `frame`. A frame is `[left, top, right, bottom]` in pixels of the window.
//! - `history`: earlier passages by the same writer.
//! - `next`: the text that the writer typed next.
//!
//! The prompt does not read `history` yet. The eval uses the default settings with the clipboard
//! allowed, and `clipboard` is a copy that came just before. `--notes` gives the notes about the
//! writer, which are empty by default. It opens a field that holds `before`
//! without its last character, lets the input settle so that the screen text is read, types that
//! character and waits for the ghost text.
//! A case matches when the word that the first Tab types is the first word of `next`. Signs after
//! the word do not count. The report gives the score of the cases with the caret between two
//! words, the score of the cases with the caret inside a word, and the median time from the key to
//! the ghost text. It also gives the mean log probability of the first word of `next` after the
//! prompt. This value moves a little for a small change of the prompt, while the score often stays
//! the same.
//!
//! `cases/tuning.jsonl` is for the work on the prompt. `cases/held_out.jsonl` only confirms a
//! change at the end. Do not tune the prompt against it.
//!
//! `--save` writes the result of each case. `--baseline` reads the results of an earlier run and
//! names the cases that the change helped or harmed, and the mean change of the log probability.
//! Compare only runs on the same computer and backend. `baselines/` keeps the results of the main
//! branch for each model.
//!
//! `--backend` names the backend that runs the model: `cpu`, `vulkan`, `cuda` or `metal`. The
//! default is Metal on macOS, else the CPU. Vulkan and CUDA need a build with their features.

mod likelihood;
mod score;

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use ghosttype_core::app::{App, Field, SettledWork};
use ghosttype_core::desktop::{
    Confidence, CopiedText, Desktop, FieldText, ScreenLine, ScreenRead, ScreenRect, ScreenText, Surroundings,
};
use ghosttype_core::engine::Reaction;
use ghosttype_core::hub::ModelFile;
use ghosttype_core::inference::{
    Backend, Completion, InferenceError, InferenceEvent, InferenceEventSink, InferenceRequest, ModelRunner,
};
use ghosttype_core::key::Key;
use ghosttype_core::settings::{ClipboardAccess, Settings, WriterNotes};
use ghosttype_llm::{BUILT_IN_BACKEND, LlamaCpp};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::likelihood::Continuation;
use crate::score::{CaseKind, Comparison, Outcome, Summary, is_next_word, next_word};

const USAGE: &str = "usage: prompt_eval MODEL.gguf CASES.jsonl [--baseline RESULTS.jsonl] [--save RESULTS.jsonl] \
     [--notes TEXT] [--backend cpu|vulkan|cuda|metal]";

#[derive(Debug, PartialEq, Eq)]
struct Arguments {
    model: PathBuf,
    cases: PathBuf,
    baseline: Option<PathBuf>,
    save: Option<PathBuf>,
    notes: String,
    backend: Backend,
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
            notes: String::new(),
            backend: BUILT_IN_BACKEND,
        };
        for option in options.chunks(2) {
            match option {
                [name, path] if name == "--baseline" => parsed.baseline = Some(PathBuf::from(path)),
                [name, path] if name == "--save" => parsed.save = Some(PathBuf::from(path)),
                [name, text] if name == "--notes" => parsed.notes.clone_from(text),
                [name, backend] if name == "--backend" => parsed.backend = backend_named(backend)?,
                _ => return None,
            }
        }
        Some(parsed)
    }
}

/// Returns the backend with `name` in [`USAGE`], or `None` for another name.
fn backend_named(name: &str) -> Option<Backend> {
    match name {
        "cpu" => Some(Backend::Cpu),
        "vulkan" => Some(Backend::Vulkan),
        "cuda" => Some(Backend::Cuda),
        "metal" => Some(Backend::Metal),
        _ => None,
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    app: Option<String>,
    app_id: Option<String>,
    window_title: Option<String>,
    domain: Option<String>,
    field_label: Option<String>,
    before: String,
    after: String,
    clipboard: Option<String>,
    screen: Option<CaseScreen>,
    #[expect(dead_code, reason = "the prompt does not read earlier writing yet")]
    history: Vec<String>,
    next: String,
}

/// A frame as `[left, top, right, bottom]`.
type CaseFrame = [i32; 4];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseScreen {
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
    fn to_screen_text(&self) -> ScreenText {
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

    fn copied_text(&self) -> Option<CopiedText> {
        Some(CopiedText {
            text: self.clipboard.clone()?,
            age: Some(Duration::ZERO),
        })
    }
}

struct Model {
    runner: LlamaCpp,
    sink: InferenceEventSink,
    events: mpsc::Receiver<InferenceEvent>,
}

impl Model {
    fn load(path: &Path, backend: Backend) -> anyhow::Result<Self> {
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
            .submit(InferenceRequest::Load { file, backend }, &model.sink);
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

/// What one case gave, and the prompt with the next word for its log probability.
struct CaseRun {
    outcome: Outcome,
    continuation: Option<Continuation>,
}

fn run_case(model: &mut Model, case: &Case, settings: &Settings) -> anyhow::Result<CaseRun> {
    let mut before = case.before.clone();
    let last_character = before
        .pop()
        .with_context(|| format!("case {} has no text before the caret", case.id))?;
    let field = CaseField {
        surroundings: Surroundings {
            app_name: case.app.clone(),
            app_id: case.app_id.clone(),
            window_title: case.window_title.clone(),
            field_label: case.field_label.clone(),
            domain: case.domain.clone(),
            app_path: None,
            field_text: Some(FieldText {
                before_caret: before,
                after_caret: case.after.clone(),
            }),
        },
        clipboard: case.clipboard.clone(),
    };
    let mut app = App::new();
    app.use_settings(settings);
    if let Some(screen) = &case.screen {
        let Some(SettledWork::ReadScreen { read: id, .. }) = app.on_input_settled(&field) else {
            bail!("case {} reads no screen text", case.id);
        };
        let read = ScreenRead {
            id,
            text: Some(screen.to_screen_text()),
        };
        app.on_screen_read(read, &field);
    }

    let start = Instant::now();
    let (reaction, prompt) = match app.on_key(Key::Char(last_character), &field) {
        Reaction::Request(request) => {
            let prompt = match &request {
                InferenceRequest::Complete { prompt, .. } => Some(prompt.0.clone()),
                _ => None,
            };
            (app.on_completion(model.complete(request)?), prompt)
        }
        reaction => (reaction, None),
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

    Ok(CaseRun {
        outcome: Outcome {
            id: case.id.clone(),
            kind: CaseKind::of(&case.before, &case.next),
            suggestion,
            is_match: is_next_word(&accepted, &case.next),
            millis,
            log_probability: None,
        },
        continuation: prompt.map(|prompt| Continuation {
            prompt,
            text: next_word(&case.next).to_owned(),
        }),
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
    if let Some(mean) = summary.mean_log_probability {
        writeln!(output, "log prob:      {mean:.3}")?;
    }
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
    writeln!(output, "unchanged:     {}", comparison.unchanged)?;
    let Some(change) = comparison.log_probability else {
        return Ok(());
    };
    writeln!(
        output,
        "log prob diff: {:+.3} (higher {}, lower {})",
        change.mean, change.higher, change.lower
    )
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

    let settings = Settings {
        clipboard_access: ClipboardAccess::Allowed,
        writer_notes: WriterNotes(arguments.notes.clone()),
        ..Settings::default()
    };
    let mut model = Model::load(&arguments.model, arguments.backend)?;
    let runs = cases
        .iter()
        .map(|case| run_case(&mut model, case, &settings))
        .collect::<anyhow::Result<Vec<_>>>()?;
    drop(model);
    let continuations = runs
        .iter()
        .map(|run| run.continuation.as_ref())
        .collect::<Vec<_>>();
    let log_probabilities = likelihood::log_probabilities(&arguments.model, &continuations)?;
    let outcomes = runs
        .into_iter()
        .zip(log_probabilities)
        .map(|(run, log_probability)| Outcome {
            log_probability,
            ..run.outcome
        })
        .collect::<Vec<_>>();

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
                notes: String::new(),
                backend: BUILT_IN_BACKEND,
            })
        );
    }

    #[test]
    fn backend_option_names_the_backend_that_runs_the_model() {
        let names = ["cpu", "vulkan", "cuda", "metal"];

        let backends = names.map(|name| {
            parse(&["model.gguf", "cases.jsonl", "--backend", name]).map(|arguments| arguments.backend)
        });

        assert_eq!(
            backends,
            [
                Some(Backend::Cpu),
                Some(Backend::Vulkan),
                Some(Backend::Cuda),
                Some(Backend::Metal)
            ]
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
    fn notes_option_gives_the_writer_notes() {
        let arguments = parse(&["model.gguf", "cases.jsonl", "--notes", "I am a nurse."]);

        assert_eq!(
            arguments.map(|arguments| arguments.notes),
            Some("I am a nurse.".to_owned())
        );
    }

    #[test]
    fn arguments_that_do_not_follow_the_usage_are_rejected() {
        let cases: [&[&str]; 5] = [
            &["model.gguf"],
            &["model.gguf", "cases.jsonl", "--save"],
            &["model.gguf", "cases.jsonl", "--backend", "opencl"],
            &["model.gguf", "cases.jsonl", "--verbose", "yes"],
            &["model.gguf", "cases.jsonl", "extra.jsonl"],
        ];

        let parsed = cases.map(parse);

        assert_eq!(parsed, cases.map(|_| None));
    }
}
