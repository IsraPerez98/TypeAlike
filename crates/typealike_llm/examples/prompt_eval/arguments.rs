//! The command line arguments of the eval.

use std::num::NonZeroU32;
use std::path::PathBuf;

use typealike_core::inference::{Backend, CpuCores, SentenceEnd};
use typealike_core::settings::{
    ClipboardAccess, HistoryAccess, PersonalizationSettings, PromptPreparation, ScreenTextAccess,
    ScreenTextSettings, Settings, SuggestionLength, TypoRule, WordChoice, WriterName, WriterNotes,
};
use typealike_llm::BUILT_IN_BACKEND;

pub const USAGE: &str = "usage: prompt_eval MODEL.gguf CASES.jsonl [--baseline RESULTS.jsonl] [--save RESULTS.jsonl] \
     [--prompts PROMPTS.jsonl] [--notes TEXT] [--name TEXT] [--backend cpu|vulkan|cuda|metal] [--cpu-cores PERFORMANCE,EFFICIENCY] \
     [--batch TOKENS] [--word-choice off|light|medium|strong] [--word-bias on|off] [--shared-history on|off] [--tail-cut on|off] \
     [--typo-rule on|off] [--sentence-stop on|off] [--length very_short|short|medium|long|very_long] [--replay off|word|character] \
     [--screen on|off] [--clipboard on|off] [--history on|off]";

#[derive(Debug, PartialEq, Eq)]
pub struct Arguments {
    pub model: PathBuf,
    pub cases: PathBuf,
    pub baseline: Option<PathBuf>,
    pub save: Option<PathBuf>,
    pub prompts: Option<PathBuf>,
    pub notes: String,
    pub name: String,
    pub backend: Backend,
    pub cpu_cores: Option<CpuCores>,
    /// The most tokens of each prompt decode, or `None` for the batch of the backend.
    pub batch: Option<NonZeroU32>,
    pub word_choice: WordChoice,
    pub word_bias: WordBiasUse,
    pub shared_history: SharedHistory,
    pub tail_cut: TailCut,
    pub typo_rule: TypoRule,
    pub sentence_end: SentenceEnd,
    pub length: SuggestionLength,
    pub replay: Replay,
    pub screen: ScreenTextAccess,
    pub clipboard: ClipboardAccess,
    pub history: HistoryAccess,
}

/// Whether the apps of a case share their earlier writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedHistory {
    On,
    Off,
}

/// Whether a request of the word choice has the word bias, next to the next-word bias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordBiasUse {
    On,
    Off,
}

/// Where the eval asks for a suggestion in the text that the writer typed next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Replay {
    /// Only at the caret of each case.
    Off,
    /// Also at the start of each word of `next`.
    Word,
    /// Also after each character of `next`.
    Character,
}

/// Whether the suggestions stop when the text after the first word gets unlikely, as in the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailCut {
    On,
    Off,
}

impl Arguments {
    /// Returns the arguments after the program name, or `None` when they do not follow [`USAGE`].
    pub fn parse(arguments: &[String]) -> Option<Self> {
        let [model, cases, options @ ..] = arguments else {
            return None;
        };
        let mut parsed = Self {
            model: PathBuf::from(model),
            cases: PathBuf::from(cases),
            baseline: None,
            save: None,
            prompts: None,
            notes: String::new(),
            name: String::new(),
            backend: BUILT_IN_BACKEND,
            cpu_cores: None,
            batch: None,
            word_choice: WordChoice::Off,
            word_bias: WordBiasUse::On,
            shared_history: SharedHistory::On,
            tail_cut: TailCut::On,
            typo_rule: TypoRule::default(),
            sentence_end: SentenceEnd::default(),
            length: SuggestionLength::default(),
            replay: Replay::Off,
            screen: ScreenTextAccess::Allowed,
            clipboard: ClipboardAccess::Allowed,
            history: HistoryAccess::On,
        };
        for option in options.chunks(2) {
            match option {
                [name, path] if name == "--baseline" => parsed.baseline = Some(PathBuf::from(path)),
                [name, path] if name == "--save" => parsed.save = Some(PathBuf::from(path)),
                [name, path] if name == "--prompts" => parsed.prompts = Some(PathBuf::from(path)),
                [name, text] if name == "--notes" => parsed.notes.clone_from(text),
                [name, text] if name == "--name" => parsed.name.clone_from(text),
                [name, backend] if name == "--backend" => parsed.backend = backend_named(backend)?,
                [name, cores] if name == "--cpu-cores" => parsed.cpu_cores = Some(cpu_cores_of(cores)?),
                [name, tokens] if name == "--batch" => parsed.batch = Some(tokens.parse().ok()?),
                [name, choice] if name == "--word-choice" => parsed.word_choice = word_choice_named(choice)?,
                [name, bias] if name == "--word-bias" => {
                    parsed.word_bias = switch_named(bias, WordBiasUse::On, WordBiasUse::Off)?;
                }
                [name, shared] if name == "--shared-history" => {
                    parsed.shared_history = switch_named(shared, SharedHistory::On, SharedHistory::Off)?;
                }
                [name, cut] if name == "--tail-cut" => {
                    parsed.tail_cut = switch_named(cut, TailCut::On, TailCut::Off)?;
                }
                [name, rule] if name == "--typo-rule" => {
                    parsed.typo_rule = switch_named(rule, TypoRule::HideSuggestions, TypoRule::Suggest)?;
                }
                [name, stop] if name == "--sentence-stop" => {
                    parsed.sentence_end = switch_named(stop, SentenceEnd::Stop, SentenceEnd::Continue)?;
                }
                [name, length] if name == "--length" => parsed.length = length_named(length)?,
                [name, replay] if name == "--replay" => parsed.replay = replay_named(replay)?,
                [name, screen] if name == "--screen" => {
                    parsed.screen =
                        switch_named(screen, ScreenTextAccess::Allowed, ScreenTextAccess::Denied)?;
                }
                [name, clipboard] if name == "--clipboard" => {
                    parsed.clipboard =
                        switch_named(clipboard, ClipboardAccess::Allowed, ClipboardAccess::Denied)?;
                }
                [name, history] if name == "--history" => {
                    parsed.history = switch_named(history, HistoryAccess::On, HistoryAccess::Off)?;
                }
                _ => return None,
            }
        }
        Some(parsed)
    }

    /// Returns the settings of each case before its earlier writing: the default settings with the
    /// options of the arguments, and the prompt prepared on focus.
    pub fn settings(&self) -> Settings {
        Settings {
            backend: self.backend,
            clipboard_access: self.clipboard,
            screen_text: ScreenTextSettings {
                access: self.screen,
                ..ScreenTextSettings::default()
            },
            writer_notes: WriterNotes(self.notes.clone()),
            writer_name: WriterName(self.name.clone()),
            prompt_preparation: PromptPreparation::OnFocus,
            typo_rule: self.typo_rule,
            sentence_end: self.sentence_end,
            suggestion_length: self.length,
            personalization: PersonalizationSettings {
                history: self.history,
                word_choice: self.word_choice,
                ..PersonalizationSettings::default()
            },
            ..Settings::default()
        }
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

/// Returns the value of a switch with `name` in [`USAGE`]: `on` for "on" and `off` for "off", or
/// `None` for another name.
fn switch_named<T>(name: &str, on: T, off: T) -> Option<T> {
    match name {
        "on" => Some(on),
        "off" => Some(off),
        _ => None,
    }
}

/// Returns the suggestion length with `name` in [`USAGE`], or `None` for another name.
fn length_named(name: &str) -> Option<SuggestionLength> {
    match name {
        "very_short" => Some(SuggestionLength::VeryShort),
        "short" => Some(SuggestionLength::Short),
        "medium" => Some(SuggestionLength::Medium),
        "long" => Some(SuggestionLength::Long),
        "very_long" => Some(SuggestionLength::VeryLong),
        _ => None,
    }
}

/// Returns the replay with `name` in [`USAGE`], or `None` for another name.
fn replay_named(name: &str) -> Option<Replay> {
    match name {
        "off" => Some(Replay::Off),
        "word" => Some(Replay::Word),
        "character" => Some(Replay::Character),
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

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

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
                prompts: None,
                notes: String::new(),
                name: String::new(),
                backend: BUILT_IN_BACKEND,
                cpu_cores: None,
                batch: None,
                word_choice: WordChoice::Off,
                word_bias: WordBiasUse::On,
                shared_history: SharedHistory::On,
                tail_cut: TailCut::On,
                typo_rule: TypoRule::HideSuggestions,
                sentence_end: SentenceEnd::Stop,
                length: SuggestionLength::Medium,
                replay: Replay::Off,
                screen: ScreenTextAccess::Allowed,
                clipboard: ClipboardAccess::Allowed,
                history: HistoryAccess::On,
            })
        );
    }

    #[test]
    fn screen_option_allows_or_denies_the_screen_text() {
        let names = ["on", "off", "maybe"];

        let accesses = names.map(|name| {
            parse(&["model.gguf", "cases.jsonl", "--screen", name]).map(|arguments| arguments.screen)
        });

        assert_eq!(
            accesses,
            [
                Some(ScreenTextAccess::Allowed),
                Some(ScreenTextAccess::Denied),
                None
            ]
        );
    }

    #[test]
    fn clipboard_option_allows_or_denies_the_clipboard() {
        let names = ["on", "off", "maybe"];

        let accesses = names.map(|name| {
            parse(&["model.gguf", "cases.jsonl", "--clipboard", name]).map(|arguments| arguments.clipboard)
        });

        assert_eq!(
            accesses,
            [
                Some(ClipboardAccess::Allowed),
                Some(ClipboardAccess::Denied),
                None
            ]
        );
    }

    #[test]
    fn history_option_turns_the_earlier_writing_on_or_off() {
        let names = ["on", "off", "maybe"];

        let accesses = names.map(|name| {
            parse(&["model.gguf", "cases.jsonl", "--history", name]).map(|arguments| arguments.history)
        });

        assert_eq!(
            accesses,
            [Some(HistoryAccess::On), Some(HistoryAccess::Off), None]
        );
    }

    #[test]
    fn settings_use_the_screen_text_the_clipboard_and_the_earlier_writing_of_the_options() {
        let cases: [(&[&str], _); 2] = [
            (
                &[],
                (
                    ScreenTextAccess::Allowed,
                    ClipboardAccess::Allowed,
                    HistoryAccess::On,
                ),
            ),
            (
                &["--screen", "off", "--clipboard", "off", "--history", "off"],
                (
                    ScreenTextAccess::Denied,
                    ClipboardAccess::Denied,
                    HistoryAccess::Off,
                ),
            ),
        ];

        let accesses = cases.map(|(options, _)| {
            let settings = parse(&[["model.gguf", "cases.jsonl"].as_slice(), options].concat())
                .unwrap()
                .settings();
            (
                settings.screen_text.access,
                settings.clipboard_access,
                settings.personalization.history,
            )
        });

        assert_eq!(accesses, cases.map(|(_, accesses)| accesses));
    }

    #[test]
    fn replay_option_asks_at_each_word_or_at_each_character_of_the_next_text() {
        let names = ["off", "word", "character", "letter"];

        let replays = names.map(|name| {
            parse(&["model.gguf", "cases.jsonl", "--replay", name]).map(|arguments| arguments.replay)
        });

        assert_eq!(
            replays,
            [
                Some(Replay::Off),
                Some(Replay::Word),
                Some(Replay::Character),
                None
            ]
        );
    }

    #[test]
    fn typo_rule_option_turns_the_typo_rule_on_or_off() {
        let names = ["on", "off", "maybe"];

        let rules = names.map(|name| {
            parse(&["model.gguf", "cases.jsonl", "--typo-rule", name]).map(|arguments| arguments.typo_rule)
        });

        assert_eq!(
            rules,
            [Some(TypoRule::HideSuggestions), Some(TypoRule::Suggest), None]
        );
    }

    #[test]
    fn sentence_stop_option_stops_each_suggestion_at_the_end_of_a_sentence_or_not() {
        let names = ["on", "off", "maybe"];

        let ends = names.map(|name| {
            parse(&["model.gguf", "cases.jsonl", "--sentence-stop", name])
                .map(|arguments| arguments.sentence_end)
        });

        assert_eq!(ends, [Some(SentenceEnd::Stop), Some(SentenceEnd::Continue), None]);
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
    fn batch_option_sets_the_most_tokens_of_each_prompt_decode() {
        let arguments = parse(&["model.gguf", "cases.jsonl", "--batch", "512"]);

        assert_eq!(
            arguments.map(|arguments| arguments.batch),
            Some(Some(NonZeroU32::new(512).unwrap()))
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
    fn word_bias_option_turns_only_the_word_bias_on_or_off() {
        let names = ["on", "off", "maybe"];

        let switches = names.map(|name| {
            parse(&["model.gguf", "cases.jsonl", "--word-bias", name]).map(|arguments| arguments.word_bias)
        });

        assert_eq!(switches, [Some(WordBiasUse::On), Some(WordBiasUse::Off), None]);
    }

    #[test]
    fn shared_history_option_shares_the_texts_of_the_apps_or_keeps_them_apart() {
        let names = ["on", "off", "maybe"];

        let switches = names.map(|name| {
            parse(&["model.gguf", "cases.jsonl", "--shared-history", name])
                .map(|arguments| arguments.shared_history)
        });

        assert_eq!(
            switches,
            [Some(SharedHistory::On), Some(SharedHistory::Off), None]
        );
    }

    #[test]
    fn tail_cut_option_turns_the_cut_of_the_tail_on_or_off() {
        let names = ["on", "off", "maybe"];

        let cuts = names.map(|name| {
            parse(&["model.gguf", "cases.jsonl", "--tail-cut", name]).map(|arguments| arguments.tail_cut)
        });

        assert_eq!(cuts, [Some(TailCut::On), Some(TailCut::Off), None]);
    }

    #[test]
    fn length_option_sets_the_suggestion_length() {
        let names = ["very_short", "short", "medium", "long", "very_long", "huge"];

        let lengths = names.map(|name| {
            parse(&["model.gguf", "cases.jsonl", "--length", name]).map(|arguments| arguments.length)
        });

        assert_eq!(
            lengths,
            [
                Some(SuggestionLength::VeryShort),
                Some(SuggestionLength::Short),
                Some(SuggestionLength::Medium),
                Some(SuggestionLength::Long),
                Some(SuggestionLength::VeryLong),
                None,
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
    fn prompts_option_names_the_file_of_the_prompts() {
        let arguments = parse(&["model.gguf", "cases.jsonl", "--prompts", "prompts.jsonl"]);

        assert_eq!(
            arguments.map(|arguments| arguments.prompts),
            Some(Some(PathBuf::from("prompts.jsonl")))
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
    fn name_option_gives_the_writer_name() {
        let arguments = parse(&["model.gguf", "cases.jsonl", "--name", "Sara Novak"]);

        assert_eq!(
            arguments.map(|arguments| arguments.name),
            Some("Sara Novak".to_owned())
        );
    }

    #[test]
    fn arguments_that_do_not_follow_the_usage_are_rejected() {
        let cases: [&[&str]; 10] = [
            &["model.gguf"],
            &["model.gguf", "cases.jsonl", "--save"],
            &["model.gguf", "cases.jsonl", "--backend", "opencl"],
            &["model.gguf", "cases.jsonl", "--cpu-cores", "4"],
            &["model.gguf", "cases.jsonl", "--cpu-cores", "0,6"],
            &["model.gguf", "cases.jsonl", "--cpu-cores", "four,six"],
            &["model.gguf", "cases.jsonl", "--batch", "0"],
            &["model.gguf", "cases.jsonl", "--batch", "many"],
            &["model.gguf", "cases.jsonl", "--verbose", "yes"],
            &["model.gguf", "cases.jsonl", "extra.jsonl"],
        ];

        let parsed = cases.map(parse);

        assert_eq!(parsed, cases.map(|_| None));
    }
}
