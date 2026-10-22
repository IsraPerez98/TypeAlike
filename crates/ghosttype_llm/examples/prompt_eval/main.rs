//! Scores the suggestions of a GGUF model on a file of typing cases, through the code that the app
//! uses: the prompt, the model runner and the ghost text.
//!
//! ```text
//! cargo run --release -p ghosttype_llm --example prompt_eval -- \
//!     MODEL.gguf CASES.jsonl [--baseline RESULTS.jsonl] [--save RESULTS.jsonl] [--notes TEXT] \
//!     [--name TEXT] [--backend BACKEND] [--cpu-cores PERFORMANCE,EFFICIENCY] [--word-choice CHOICE] \
//!     [--word-bias on|off] [--shared-history on|off] [--tail-cut on|off] [--typo-rule on|off] \
//!     [--length very_short|short|medium|long|very_long]
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
//! - `before_enter`: the text before the caret just before the writer pressed Enter, when Enter
//!   did not add only a line break, for example when the app adds a bullet, sends the message or
//!   keeps the text of a search box. It can be left out.
//! - `clipboard`: the text on the clipboard, or `null`.
//! - `screen`: the text near the field on the screen, or `null`: `field` and `caret` (or `null`)
//!   are the frames of the field and the caret, and each of `lines` has a `text` outside of the
//!   field and its `frame`. A frame is `[left, top, right, bottom]` in pixels of the window.
//! - `writer`: the name of a store of earlier texts, or `null`. The store is the file
//!   `writers/NAME.jsonl` next to the case file. Each line is one earlier text with its `text`,
//!   `app_id` and `domain`, oldest first. The eval loads the store, then adds `history`.
//! - `left_out`: the index of a text of the store, from 0, that the history leaves out: the text
//!   that the case comes from. It can be left out.
//! - `history`: earlier texts of the same writer, oldest first, after the texts of the store. Each
//!   one has its `text`, and the `app_id` and `domain` of the place where the writer typed it.
//! - `words`: the words that the writer often writes, with their counts. It can be left out. When
//!   the case has words, the word bias comes from them, and else from `history`.
//! - `next`: the text that the writer typed next. It is empty when Enter did not start a new line.
//! - `typo`: `true` when the last word of `before` has a typo. It can be left out.
//! - `fixed`: for a typo case, `before` without the typo. It can be left out.
//!
//! The eval uses the default settings with the backend of `--backend`, the clipboard and the
//! earlier writing on, and `clipboard` is a copy that came just before. `--notes` gives the notes
//! about the writer, and `--name` the name of the writer. Both are empty by default. It opens a
//! field that holds `before` without its last character, lets the input settle so that the screen
//! text is read, types that character and waits for the ghost text. The model does not decode the
//! prompt before the key, as on the first key in a field. Then the eval opens the field again, lets
//! the model decode the prompt when the input settles, as the app does when it prepares the prompt
//! on focus, and types the character again for the warm time.
//! When `before` ends with a line break or the case has `before_enter`, the last key is Enter. The
//! field holds `before_enter`, or else `before` without its last line break. After Enter, the field
//! holds `before`, and the input settles again before the ghost text.
//! A case matches when the word that the first Tab types is the first word of `next`. Signs after
//! the word do not count. A typo case matches when no suggestion finishes the word with the typo. A
//! case where `before` does not add a line break to `before_enter` matches when no suggestion
//! shows.
//! The report gives the score of the cases with the caret between two words, the score of the
//! cases with the caret inside a word, the score of the typo cases, the score of the cases without a
//! new line, the median time from the key to the ghost text, and the median warm time. It also
//! gives the median tokens of a prompt, and the time for each token of the prompt, which tells
//! whether a change adds time through more tokens. It also gives the mean log probability of the
//! first word of `next` after the prompt. This value moves a little for a small change of the
//! prompt, while the score often stays the same.
//!
//! For the cases with the caret between two words or inside a word, the report counts the shown
//! words before the first word that differs from `next` (right), and the shown words from that
//! word on (wrong). A word after the end of `next` counts only after a wrong word. When `after`
//! has text on the line of the caret, `next` is all the text up to it, and each word after the end
//! of `next` is wrong, because the field already holds the text that follows. It gives the
//! mean log probability of the first shown word, as the model runner found it. Then it gives two
//! tables from the same run. The first table gives, for each minimum probability of the first word,
//! the share of the suggestions that still show and the share of them that match. The second table
//! gives, for each cut of the tail, the words that still show, the right words and the wrong
//! words. `--tail-cut off` turns off the cut of the app, so that the second table starts from the
//! full suggestions. Use it to choose a cut.
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
//! `cases/left_out.jsonl` has cases from the stores of `writers/`, each with one text of its store
//! left out of the history. The case starts at the start of a word of that text: a random word in
//! the `start` cases, and a word after a run of 2 or 3 words that the rest of the store repeats in
//! the `run` cases. The phrase rule shows nothing in these cases. Run it with each `--word-choice`
//! to measure the word bias, and compare each run with the run without it.
//!
//! `cases/chat.jsonl` has chats, emails and documents with only one or two characters in the field,
//! with the names and the times of a chat on the screen, and with a time inside a message. Run it
//! also with `--notes`. `cases/typos.jsonl` has cases of `tuning.jsonl` and `held_out.jsonl` with
//! the second and the third letter of the last word swapped. `cases/new_line.jsonl` has cases that
//! end with Enter: closings before the name of the writer, greetings, lists and paragraphs, in
//! several languages, and fields where Enter sends the text, submits a search or confirms an input
//! method. Run it with `--name "Sara Novak"`. `cases/mid_line.jsonl` has cases with the caret
//! inside a line: the writer adds words before text that the field already holds.
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
//! has no log probability. `--word-bias off` takes only the word bias out of each request, and
//! keeps the next-word bias and the phrase rule of the word choice.
//!
//! `--shared-history on` (the default) gives each app of a case and of its history the scope "All
//! apps", so that each app uses the texts of the other apps. `off` gives each app the scope "Only
//! this app".
//!
//! `--typo-rule` turns the rule for typos of the app `on` (the default) or `off`. The report gives
//! a table for each cut of the typed letters of the last word: the typo cases that it catches, and
//! the cases with a next word that it hides. Run it with `--typo-rule off` to choose a cut. With the
//! rule on, the app also fixes typos. The report counts the fixes of the typo cases that write the
//! word of `fixed` and `next` (`typo fixes`), and the fixes of the other cases (`other fixes`),
//! which each change a word without a typo.
//!
//! `--length` sets the suggestion length: `very_short`, `short`, `medium` (the default), `long` or
//! `very_long`. The length sets the token limit and the cut of the tail of each request. As in the
//! app, the CPU backend allows at most `short`.

mod arguments;
mod case;
mod likelihood;
mod report;
mod run;
mod score;

use std::fs;
use std::io;
use std::path::Path;

use anyhow::{Context, bail};
use serde::de::DeserializeOwned;

use crate::arguments::{Arguments, USAGE};
use crate::case::{Case, history_of, read_writers, settings_for_case};
use crate::report::write_report;
use crate::run::{CaseSetup, ModelSession, run_case, with_measures};
use crate::score::{Comparison, Outcome, Summary};

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

    let settings = arguments.settings();
    let mut model = ModelSession::load(&arguments.model, arguments.backend, arguments.cpu_cores)?;
    let runs = cases
        .iter()
        .zip(1..)
        .map(|(case, version)| {
            let history = history_of(case, &writers)?;
            let settings = settings_for_case(&settings, case, &history, arguments.shared_history);
            let setup = CaseSetup {
                case,
                history: &history,
                settings: &settings,
                version,
            };
            run_case(&mut model, setup, &arguments)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    drop(model);
    let continuations = runs
        .iter()
        .map(|run| run.continuation.as_ref())
        .collect::<Vec<_>>();
    let measures = likelihood::measure(&arguments.model, &continuations)?;
    let outcomes = with_measures(runs, measures);

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
