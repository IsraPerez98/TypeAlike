//! Scores the suggestions of a GGUF model on a file of typing cases, through the code that the app
//! uses: the prompt, the model runner and the ghost text.
//!
//! ```text
//! cargo run --release -p ghosttype_llm --example prompt_eval -- \
//!     MODEL.gguf CASES.jsonl [--baseline RESULTS.jsonl] [--save RESULTS.jsonl] [--notes TEXT] \
//!     [--backend BACKEND] [--cpu-cores PERFORMANCE,EFFICIENCY] [--word-choice CHOICE]
//! ```
//!
//! Each line of the case file is one JSON object with these keys:
//!
//! - `id`: a unique name.
//! - `app`, `app_id`, `window_title`, `domain`: the place where the writer types, or `null`.
//!   `app_id` is a bundle identifier on macOS or a program file name on Windows. `domain` is the
//!   host of the web page.
//! - `field_caption`, `field_name`, `field_placeholder`, `field_help`: the labels of the field, as
//!   in `FieldLabels`. Each key can be left out.
//! - `before`, `after`: the text of the field before and after the caret.
//! - `clipboard`: the text on the clipboard, or `null`.
//! - `screen`: the text near the field on the screen, or `null`: `field` and `caret` (or `null`)
//!   are the frames of the field and the caret, and each of `lines` has a `text` outside of the
//!   field and its `frame`. A frame is `[left, top, right, bottom]` in pixels of the window.
//! - `writer`: the name of a store of earlier texts, or `null`. The store is the file
//!   `writers/NAME.jsonl` next to the case file. Each line is one earlier text with its `text`,
//!   `app_id` and `domain`, oldest first. The eval loads the store, then adds `history`.
//! - `history`: earlier texts of the same writer, oldest first, after the texts of the store. Each
//!   one has its `text`, and the `app_id` and `domain` of the place where the writer typed it.
//! - `words`: the words that the writer often writes, with their counts. It can be left out. When
//!   the case has words, the word bias comes from them, and else from `history`.
//! - `next`: the text that the writer typed next.
//!
//! The eval uses the default settings with the clipboard and the earlier writing on, and
//! `clipboard` is a copy that came just before. `--notes` gives the notes about the
//! writer, which are empty by default. It opens a field that holds `before`
//! without its last character, lets the input settle so that the screen text is read, types that
//! character and waits for the ghost text. The model does not decode the prompt before the key, as
//! on the first key in a field. Then the eval opens the field again, lets the model decode the
//! prompt when the input settles, as the app does when it prepares the prompt on focus, and types
//! the character again for the warm time.
//! A case matches when the word that the first Tab types is the first word of `next`. Signs after
//! the word do not count. The report gives the score of the cases with the caret between two
//! words, the score of the cases with the caret inside a word, the median time from the key to
//! the ghost text, and the median warm time. It also gives the median tokens of a prompt, and the
//! time for each token of the prompt, which tells whether a change adds time through more tokens.
//! It also gives the mean log probability of the first word of `next` after the
//! prompt. This value moves a little for a small change of the prompt, while the score often stays
//! the same.
//!
//! `cases/tuning.jsonl` is for the work on the prompt. `cases/held_out.jsonl` only confirms a
//! change at the end. Do not tune the prompt against it.
//!
//! `cases/stores_tuning.jsonl` and `cases/stores_held_out.jsonl` have cases with a `writer`: a
//! store of 100 to 160 earlier texts in 5 or 6 places. They measure how the prompt picks its
//! examples. The id of a case starts with its group: `topic` (the next word is in one text of the
//! place, among texts on other topics), `othertopic` (it is in no text), `sameformat` (the place
//! has no text, and a place of the same format has it), `otherformat` (the place has no text, and
//! only other formats exist), `greeting` (it starts a long earlier text), `names` (a name that
//! the writer writes often) and `phrase` (the last 3 words start a phrase of the writer: the writer
//! repeats it, or the writer changes it this time). The held-out file has its own writers.
//!
//! `--save` writes the result of each case. `--baseline` reads the results of an earlier run and
//! names the cases that the change helped or harmed, and the mean change of the log probability.
//! Compare only runs on the same computer and backend. `baselines/` keeps the results of the main
//! branch for each model.
//!
//! `--backend` names the backend that runs the model: `cpu`, `vulkan`, `cuda` or `metal`. The
//! default is Metal on macOS, else the CPU. Vulkan and CUDA need a build with their features.
//! `--cpu-cores` gives the physical performance and efficiency cores of the CPU, for example
//! `4,6`, so that the CPU backend uses the threads of the app on Windows. Without it, llama.cpp
//! keeps its threads, as the app on macOS does.
//!
//! `--word-choice` sets how much suggestions prefer the words of the writer: `off` (the default),
//! `light`, `medium` or `strong`. The log probability does not use the word bias. With a word
//! choice that is not off, a phrase that the writer repeats shows without the model, and its case
//! has no log probability.

mod likelihood;
mod score;

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use ghosttype_core::app::{App, Field, SettledWork};
use ghosttype_core::desktop::{
    Confidence, CopiedText, Desktop, FieldLabels, FieldText, ScreenLine, ScreenRead, ScreenRect, ScreenText,
    Surroundings,
};
use ghosttype_core::engine::Reaction;
use ghosttype_core::history::{HistoryEntry, word_bias_of_counts};
use ghosttype_core::hub::ModelFile;
use ghosttype_core::inference::{
    Backend, Completion, CpuCores, InferenceError, InferenceEvent, InferenceEventSink, InferenceRequest,
    ModelRunner, WordBias,
};
use ghosttype_core::key::Key;
use ghosttype_core::settings::{
    ClipboardAccess, HistoryAccess, PersonalizationSettings, PromptPreparation, Settings, WordChoice,
    WriterNotes,
};
use ghosttype_llm::{BUILT_IN_BACKEND, LlamaCpp};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::likelihood::Continuation;
use crate::score::{CaseKind, Comparison, Outcome, Summary, is_next_word, next_word};

const USAGE: &str = "usage: prompt_eval MODEL.gguf CASES.jsonl [--baseline RESULTS.jsonl] [--save RESULTS.jsonl] \
     [--notes TEXT] [--backend cpu|vulkan|cuda|metal] [--cpu-cores PERFORMANCE,EFFICIENCY] \
     [--word-choice off|light|medium|strong]";

#[derive(Debug, PartialEq, Eq)]
struct Arguments {
    model: PathBuf,
    cases: PathBuf,
    baseline: Option<PathBuf>,
    save: Option<PathBuf>,
    notes: String,
    backend: Backend,
    cpu_cores: Option<CpuCores>,
    word_choice: WordChoice,
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
            cpu_cores: None,
            word_choice: WordChoice::Off,
        };
        for option in options.chunks(2) {
            match option {
                [name, path] if name == "--baseline" => parsed.baseline = Some(PathBuf::from(path)),
                [name, path] if name == "--save" => parsed.save = Some(PathBuf::from(path)),
                [name, text] if name == "--notes" => parsed.notes.clone_from(text),
                [name, backend] if name == "--backend" => parsed.backend = backend_named(backend)?,
                [name, cores] if name == "--cpu-cores" => parsed.cpu_cores = Some(cpu_cores_of(cores)?),
                [name, choice] if name == "--word-choice" => parsed.word_choice = word_choice_named(choice)?,
                _ => return None,
            }
        }
        Some(parsed)
    }
}

/// Returns the word choice with `name` in [`USAGE`], or `None` for another name.
fn word_choice_named(name: &str) -> Option<WordChoice> {
    match name {
        "off" => Some(WordChoice::Off),
        "light" => Some(WordChoice::Light),
        "medium" => Some(WordChoice::Medium),
        "strong" => Some(WordChoice::Strong),
        _ => None,
    }
}

/// Returns the cores of `text`, the performance cores and the efficiency cores with a comma
/// between them, or `None` when it has another form or no performance core.
fn cpu_cores_of(text: &str) -> Option<CpuCores> {
    let (performance, efficiency) = text.split_once(',')?;
    Some(CpuCores {
        performance: performance.parse().ok()?,
        efficiency: efficiency.parse().ok()?,
    })
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
    field_caption: Option<String>,
    field_name: Option<String>,
    field_placeholder: Option<String>,
    field_help: Option<String>,
    before: String,
    after: String,
    clipboard: Option<String>,
    screen: Option<CaseScreen>,
    #[serde(default)]
    writer: Option<String>,
    history: Vec<CaseEntry>,
    #[serde(default)]
    words: BTreeMap<String, u32>,
    next: String,
}

/// An earlier text of the writer.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseEntry {
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

/// Returns the earlier texts of `case`: the texts of its writer in `writers`, then its own.
fn history_of(case: &Case, writers: &BTreeMap<String, Vec<CaseEntry>>) -> anyhow::Result<Vec<HistoryEntry>> {
    let store = match &case.writer {
        Some(name) => writers
            .get(name)
            .with_context(|| format!("case {} names the unknown writer {name}", case.id))?
            .as_slice(),
        None => &[],
    };
    Ok(store
        .iter()
        .chain(&case.history)
        .map(CaseEntry::to_history_entry)
        .collect())
}

/// Reads the store of each writer that `cases` name, from the `writers` folder in `directory`.
fn read_writers(directory: &Path, cases: &[Case]) -> anyhow::Result<BTreeMap<String, Vec<CaseEntry>>> {
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
    fn load(path: &Path, backend: Backend, cpu_cores: Option<CpuCores>) -> anyhow::Result<Self> {
        let path = path.to_owned();
        let runner = LlamaCpp::new(move |_| Some(path.clone()), cpu_cores);
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

/// Returns `request` with the word bias of the `words` of `case`, or else with the bias that the
/// app built from the history of the case. The bias gets `version`, because each case starts a new
/// app, and the runner prepares each version one time.
fn with_case_bias(
    request: InferenceRequest,
    case: &Case,
    choice: WordChoice,
    version: u64,
) -> InferenceRequest {
    let InferenceRequest::Complete {
        prompt,
        max_tokens,
        word_bias,
    } = request
    else {
        return request;
    };
    let word_bias = if case.words.is_empty() {
        word_bias.map(|bias| WordBias { version, ..bias })
    } else {
        word_bias_of_counts(case.words.clone(), choice, version)
    };
    InferenceRequest::Complete {
        prompt,
        max_tokens,
        word_bias,
    }
}

/// What the eval does with a prompt that the app prepares before the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Preparation {
    /// The model decodes the prompt, as when the user waits before the first key.
    Decode,
    /// The model does not decode the prompt, as when the user types at once.
    Skip,
}

/// What one key in the field of a case gave.
struct KeyRun {
    reaction: Reaction,
    prompt: Option<String>,
    millis: u64,
}

/// A case before its last key, in a new app each time.
struct Typing<'case> {
    case: &'case Case,
    history: &'case [HistoryEntry],
    settings: &'case Settings,
    field: CaseField,
    last_character: char,
    version: u64,
}

impl Typing<'_> {
    /// Opens the field in a new app, lets the input settle with the `preparation`, types the last
    /// character and waits for the ghost text. Returns the app, for the keys after it.
    fn type_last_character(
        &self,
        model: &mut Model,
        preparation: Preparation,
    ) -> anyhow::Result<(App, KeyRun)> {
        let mut app = App::new();
        app.use_settings(self.settings);
        app.use_history(self.history.to_vec());
        self.settle(model, &mut app, preparation)?;
        let start = Instant::now();
        let (reaction, prompt) = match app.on_key(Key::Char(self.last_character), &self.field) {
            Reaction::Request(request) => {
                let prompt = match &request {
                    InferenceRequest::Complete { prompt, .. } => Some(prompt.0.clone()),
                    _ => None,
                };
                let choice = self.settings.personalization.word_choice;
                let request = with_case_bias(request, self.case, choice, self.version);
                (app.on_completion(model.complete(request)?), prompt)
            }
            reaction => (reaction, None),
        };
        let millis = u64::try_from(start.elapsed().as_millis())?;
        Ok((
            app,
            KeyRun {
                reaction,
                prompt,
                millis,
            },
        ))
    }

    /// Lets the input settle in `app`: gives the screen of the case to each read of the screen,
    /// and does the `preparation` with each prompt that the app prepares.
    fn settle(&self, model: &mut Model, app: &mut App, preparation: Preparation) -> anyhow::Result<()> {
        let mut has_read_screen = false;
        let mut work = app.on_input_settled(&self.field);
        while let Some(next) = work {
            work = match next {
                SettledWork::ReadScreen { read, .. } => {
                    has_read_screen = true;
                    let text = self.case.screen.as_ref().map(CaseScreen::to_screen_text);
                    app.on_screen_read(ScreenRead { id: read, text }, &self.field)
                }
                SettledWork::Prepare(request) => {
                    let result = match preparation {
                        Preparation::Decode => model.complete(request)?,
                        Preparation::Skip => Err(InferenceError::Superseded),
                    };
                    app.on_completion(result);
                    None
                }
            };
        }
        if self.case.screen.is_some() && !has_read_screen {
            bail!("case {} reads no screen text", self.case.id);
        }
        Ok(())
    }
}

fn run_case(
    model: &mut Model,
    case: &Case,
    history: &[HistoryEntry],
    settings: &Settings,
    version: u64,
) -> anyhow::Result<CaseRun> {
    let mut before = case.before.clone();
    let last_character = before
        .pop()
        .with_context(|| format!("case {} has no text before the caret", case.id))?;
    let field = CaseField {
        surroundings: Surroundings {
            app_name: case.app.clone(),
            app_id: case.app_id.clone(),
            window_title: case.window_title.clone(),
            field_labels: FieldLabels {
                caption: case.field_caption.clone(),
                name: case.field_name.clone(),
                placeholder: case.field_placeholder.clone(),
                help: case.field_help.clone(),
            },
            domain: case.domain.clone(),
            app_path: None,
            field_text: Some(FieldText {
                before_caret: before,
                after_caret: case.after.clone(),
            }),
        },
        clipboard: case.clipboard.clone(),
    };
    let typing = Typing {
        case,
        history,
        settings,
        field,
        last_character,
        version,
    };
    let (mut app, cold) = typing.type_last_character(model, Preparation::Skip)?;
    let accepted = match app.on_key(Key::Tab, &typing.field) {
        Reaction::Accept { text, .. } => text,
        _ => String::new(),
    };
    let (_, warm) = typing.type_last_character(model, Preparation::Decode)?;

    Ok(CaseRun {
        outcome: Outcome {
            id: case.id.clone(),
            kind: CaseKind::of(&case.before, &case.next),
            suggestion: match cold.reaction {
                Reaction::Show(text) => Some(text),
                _ => None,
            },
            is_match: is_next_word(&accepted, &case.next),
            millis: cold.millis,
            warm_millis: Some(warm.millis),
            prompt_tokens: None,
            log_probability: None,
        },
        continuation: cold.prompt.map(|prompt| Continuation {
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
    if let Some(millis) = summary.median_warm_millis {
        writeln!(output, "warm time:     {millis} ms")?;
    }
    if let Some(tokens) = summary.median_prompt_tokens {
        writeln!(output, "prompt tokens: {tokens}")?;
    }
    if let Some(millis) = summary.millis_per_token {
        writeln!(output, "ms per token:  {millis:.2}")?;
    }
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
    let writers = read_writers(arguments.cases.parent().unwrap_or(Path::new(".")), &cases)?;
    let baseline = arguments
        .baseline
        .as_deref()
        .map(read_json_lines::<Outcome>)
        .transpose()?;

    let settings = Settings {
        clipboard_access: ClipboardAccess::Allowed,
        writer_notes: WriterNotes(arguments.notes.clone()),
        prompt_preparation: PromptPreparation::OnFocus,
        personalization: PersonalizationSettings {
            history: HistoryAccess::On,
            word_choice: arguments.word_choice,
            ..PersonalizationSettings::default()
        },
        ..Settings::default()
    };
    let mut model = Model::load(&arguments.model, arguments.backend, arguments.cpu_cores)?;
    let runs = cases
        .iter()
        .zip(1..)
        .map(|(case, version)| run_case(&mut model, case, &history_of(case, &writers)?, &settings, version))
        .collect::<anyhow::Result<Vec<_>>>()?;
    drop(model);
    let continuations = runs
        .iter()
        .map(|run| run.continuation.as_ref())
        .collect::<Vec<_>>();
    let measures = likelihood::measure(&arguments.model, &continuations)?;
    let outcomes = runs
        .into_iter()
        .zip(measures)
        .map(|(run, measured)| Outcome {
            prompt_tokens: measured.map(|measured| measured.prompt_tokens),
            log_probability: measured.and_then(|measured| measured.log_probability),
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
    use std::num::NonZeroUsize;

    use super::*;

    fn parse(arguments: &[&str]) -> Option<Arguments> {
        Arguments::parse(&arguments.iter().map(ToString::to_string).collect::<Vec<_>>())
    }

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
    fn history_of_a_case_without_a_writer_is_its_own() {
        let history = history_of(&case_of(None, &["only"]), &BTreeMap::new()).unwrap();

        assert_eq!(texts(&history), ["only"]);
    }

    #[test]
    fn case_with_an_unknown_writer_is_an_error() {
        let history = history_of(&case_of(Some("bob"), &[]), &BTreeMap::new());

        assert!(history.is_err());
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
                cpu_cores: None,
                word_choice: WordChoice::Off,
            })
        );
    }

    #[test]
    fn cpu_cores_option_gives_the_performance_and_the_efficiency_cores() {
        let arguments = parse(&["model.gguf", "cases.jsonl", "--cpu-cores", "4,6"]);

        assert_eq!(
            arguments.map(|arguments| arguments.cpu_cores),
            Some(Some(CpuCores {
                performance: NonZeroUsize::new(4).unwrap(),
                efficiency: 6,
            }))
        );
    }

    #[test]
    fn word_choice_option_sets_the_strength_of_the_word_bias() {
        let names = ["off", "light", "medium", "strong", "huge"];

        let choices = names.map(|name| {
            parse(&["model.gguf", "cases.jsonl", "--word-choice", name])
                .map(|arguments| arguments.word_choice)
        });

        assert_eq!(
            choices,
            [
                Some(WordChoice::Off),
                Some(WordChoice::Light),
                Some(WordChoice::Medium),
                Some(WordChoice::Strong),
                None
            ]
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
        let cases: [&[&str]; 8] = [
            &["model.gguf"],
            &["model.gguf", "cases.jsonl", "--save"],
            &["model.gguf", "cases.jsonl", "--backend", "opencl"],
            &["model.gguf", "cases.jsonl", "--cpu-cores", "4"],
            &["model.gguf", "cases.jsonl", "--cpu-cores", "0,6"],
            &["model.gguf", "cases.jsonl", "--cpu-cores", "four,six"],
            &["model.gguf", "cases.jsonl", "--verbose", "yes"],
            &["model.gguf", "cases.jsonl", "extra.jsonl"],
        ];

        let parsed = cases.map(parse);

        assert_eq!(parsed, cases.map(|_| None));
    }
}
