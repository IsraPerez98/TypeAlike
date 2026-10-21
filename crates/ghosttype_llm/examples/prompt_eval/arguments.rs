//! The command line arguments of the eval.

use std::path::PathBuf;

use ghosttype_core::inference::{Backend, CpuCores};
use ghosttype_core::settings::{SuggestionLength, TypoRule, WordChoice};
use ghosttype_llm::BUILT_IN_BACKEND;

pub const USAGE: &str = "usage: prompt_eval MODEL.gguf CASES.jsonl [--baseline RESULTS.jsonl] [--save RESULTS.jsonl] \
     [--notes TEXT] [--name TEXT] [--backend cpu|vulkan|cuda|metal] [--cpu-cores PERFORMANCE,EFFICIENCY] \
     [--word-choice off|light|medium|strong] [--word-bias on|off] [--shared-history on|off] [--tail-cut on|off] \
     [--typo-rule on|off] [--length very_short|short|medium|long|very_long]";

#[derive(Debug, PartialEq, Eq)]
pub struct Arguments {
    pub model: PathBuf,
    pub cases: PathBuf,
    pub baseline: Option<PathBuf>,
    pub save: Option<PathBuf>,
    pub notes: String,
    pub name: String,
    pub backend: Backend,
    pub cpu_cores: Option<CpuCores>,
    pub word_choice: WordChoice,
    pub word_bias: WordBiasUse,
    pub shared_history: SharedHistory,
    pub tail_cut: TailCut,
    pub typo_rule: TypoRule,
    pub length: SuggestionLength,
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
            notes: String::new(),
            name: String::new(),
            backend: BUILT_IN_BACKEND,
            cpu_cores: None,
            word_choice: WordChoice::Off,
            word_bias: WordBiasUse::On,
            shared_history: SharedHistory::On,
            tail_cut: TailCut::On,
            typo_rule: TypoRule::default(),
            length: SuggestionLength::default(),
        };
        for option in options.chunks(2) {
            match option {
                [name, path] if name == "--baseline" => parsed.baseline = Some(PathBuf::from(path)),
                [name, path] if name == "--save" => parsed.save = Some(PathBuf::from(path)),
                [name, text] if name == "--notes" => parsed.notes.clone_from(text),
                [name, text] if name == "--name" => parsed.name.clone_from(text),
                [name, backend] if name == "--backend" => parsed.backend = backend_named(backend)?,
                [name, cores] if name == "--cpu-cores" => parsed.cpu_cores = Some(cpu_cores_of(cores)?),
                [name, choice] if name == "--word-choice" => parsed.word_choice = word_choice_named(choice)?,
                [name, bias] if name == "--word-bias" => parsed.word_bias = word_bias_named(bias)?,
                [name, shared] if name == "--shared-history" => {
                    parsed.shared_history = shared_history_named(shared)?;
                }
                [name, cut] if name == "--tail-cut" => parsed.tail_cut = tail_cut_named(cut)?,
                [name, rule] if name == "--typo-rule" => parsed.typo_rule = typo_rule_named(rule)?,
                [name, length] if name == "--length" => parsed.length = length_named(length)?,
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

/// Returns the use of the word bias with `name` in [`USAGE`], or `None` for another name.
fn word_bias_named(name: &str) -> Option<WordBiasUse> {
    match name {
        "on" => Some(WordBiasUse::On),
        "off" => Some(WordBiasUse::Off),
        _ => None,
    }
}

/// Returns the sharing of the earlier writing with `name` in [`USAGE`], or `None` for another name.
fn shared_history_named(name: &str) -> Option<SharedHistory> {
    match name {
        "on" => Some(SharedHistory::On),
        "off" => Some(SharedHistory::Off),
        _ => None,
    }
}

/// Returns the tail cut with `name` in [`USAGE`], or `None` for another name.
fn tail_cut_named(name: &str) -> Option<TailCut> {
    match name {
        "on" => Some(TailCut::On),
        "off" => Some(TailCut::Off),
        _ => None,
    }
}

/// Returns the typo rule with `name` in [`USAGE`], or `None` for another name.
fn typo_rule_named(name: &str) -> Option<TypoRule> {
    match name {
        "on" => Some(TypoRule::HideSuggestions),
        "off" => Some(TypoRule::Suggest),
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
                notes: String::new(),
                name: String::new(),
                backend: BUILT_IN_BACKEND,
                cpu_cores: None,
                word_choice: WordChoice::Off,
                word_bias: WordBiasUse::On,
                shared_history: SharedHistory::On,
                tail_cut: TailCut::On,
                typo_rule: TypoRule::HideSuggestions,
                length: SuggestionLength::Medium,
            })
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
