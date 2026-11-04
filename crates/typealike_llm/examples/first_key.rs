//! Measures the first completion after each load of a GGUF model, with and without the warm-up of
//! the load.
//!
//! ```text
//! cargo run --release -p typealike_llm --example first_key -- \
//!     MODEL.gguf [--backend cpu|vulkan|cuda|metal] [--cpu-cores PERFORMANCE,EFFICIENCY] \
//!     [--loads N] [--first queued|after-load]
//! ```
//!
//! The loads send their first request in two ways, in turns:
//!
//! - `queued`: with the load request. The request waits while the model loads, so the load does no
//!   warm-up, and the first completion makes the context and runs the first decodes.
//! - `after-load`: after the answer of the load, so that the load warms up the backend first.
//!
//! `--first` gives the way of the first load (default: `queued`). The first load of a process
//! also starts the backend, for example the shaders of Vulkan, as at the start of the app. Run the
//! example several times with each `--first` to measure that start. The later loads (default: 6
//! loads in all) are as a resume after a pause or a change of the model.
//!
//! Each load times four completions of 10 tokens, from the start of the completion in the runner:
//! the first prompt (`first`), the first prompt with more words (`next`), another prompt that does
//! not start as the first one (`other`), and that prompt with more words (`other next`). Each
//! prompt has about 200 tokens, as most prompts of the app. `other` decodes a whole prompt after
//! the first completion, so `first` less `other` is the cost of the first completion after a load.
//! The report gives each load, then the median of the loads after the first one for each way. The
//! `load` time goes from the load request to the answer of the load, so with `after-load` it also
//! holds the warm-up.

use std::io::{self, Write};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use anyhow::bail;
use typealike_core::hub::ModelFile;
use typealike_core::inference::{
    Backend, CpuCores, InferenceEvent, InferenceEventSink, InferenceRequest, ModelRunner, NextWordBias,
    Prompt, SentenceEnd, TokenCount, TypoCheck,
};
use typealike_llm::{BUILT_IN_BACKEND, LlamaCpp};

const USAGE: &str = "usage: first_key MODEL.gguf [--backend cpu|vulkan|cuda|metal] \
     [--cpu-cores PERFORMANCE,EFFICIENCY] [--loads N] [--first queued|after-load]";
const DEFAULT_LOADS: usize = 6;
/// The token limit of the medium suggestion length.
const MAX_TOKENS: TokenCount = TokenCount(10);
const FIRST_PROMPT: &str = "Hi Maria,\n\nThank you for the notes from the planning meeting last \
    week. I read them on the train this morning, and I agree with most of the points. The new \
    schedule for the release looks fine to me, but I think we need two more days for the tests on \
    the older laptops. Last time we found three problems there only after the release, and the \
    support team had a hard week. I also talked with Daniel about the budget for the next \
    quarter. He thinks that we can keep the same team if we move the training to the spring. That \
    sounds good to me, but I want to hear what you think first. The customers in the north region \
    also asked again about the offline mode. I told them that we cannot promise a date yet, but \
    that it is on our list. Could we meet on";
const OTHER_PROMPT: &str = "Packing list for the trip to the coast\n\nThe weather report says \
    rain on the first two days and sun after that, so I want to take the light jacket and the old \
    boots. The cabin has a small kitchen, but the shop in the village closes early, so we must buy \
    most of the food in town before we drive out. Anna wants to take the bikes, which means that \
    we need the rack and the spare tube. Last year the road to the lighthouse was closed after the \
    storm, and we lost a whole afternoon. This time I will call the visitor center before we go. \
    The ferry to the island leaves at nine and at two, and the tickets are cheaper online. We \
    should also book the boat tour for the second weekend, because it fills up fast in the \
    summer. I think we could leave on";
/// The words that the writer types after a prompt, for the completion of a later key.
const MORE_WORDS: &str = " Thursday a";

/// When a load sends its first request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FirstRequest {
    /// With the load request, so that the load does no warm-up.
    Queued,
    /// After the answer of the load.
    AfterLoad,
}

impl FirstRequest {
    fn named(name: &str) -> Option<Self> {
        match name {
            "queued" => Some(Self::Queued),
            "after-load" => Some(Self::AfterLoad),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::AfterLoad => "after-load",
        }
    }

    fn other(self) -> Self {
        match self {
            Self::Queued => Self::AfterLoad,
            Self::AfterLoad => Self::Queued,
        }
    }
}

#[derive(Debug)]
struct Arguments {
    model: PathBuf,
    backend: Backend,
    cpu_cores: Option<CpuCores>,
    loads: usize,
    first: FirstRequest,
}

impl Arguments {
    fn parse(arguments: &[String]) -> Option<Self> {
        let [model, options @ ..] = arguments else {
            return None;
        };
        let mut parsed = Self {
            model: PathBuf::from(model),
            backend: BUILT_IN_BACKEND,
            cpu_cores: None,
            loads: DEFAULT_LOADS,
            first: FirstRequest::Queued,
        };
        for option in options.chunks(2) {
            match option {
                [name, backend] if name == "--backend" => parsed.backend = backend_named(backend)?,
                [name, cores] if name == "--cpu-cores" => parsed.cpu_cores = Some(cpu_cores_of(cores)?),
                [name, loads] if name == "--loads" => parsed.loads = loads.parse().ok()?,
                [name, first] if name == "--first" => parsed.first = FirstRequest::named(first)?,
                _ => return None,
            }
        }
        Some(parsed)
    }
}

fn backend_named(name: &str) -> Option<Backend> {
    match name {
        "cpu" => Some(Backend::Cpu),
        "vulkan" => Some(Backend::Vulkan),
        "cuda" => Some(Backend::Cuda),
        "metal" => Some(Backend::Metal),
        _ => None,
    }
}

fn cpu_cores_of(text: &str) -> Option<CpuCores> {
    let (performance, efficiency) = text.split_once(',')?;
    Some(CpuCores {
        performance: performance.parse::<NonZeroUsize>().ok()?,
        efficiency: efficiency.parse().ok()?,
    })
}

/// The times of one load: the load itself and its four completions.
#[derive(Debug, Clone, Copy)]
struct LoadTimes {
    /// From the load request to the answer of the load.
    load: Duration,
    first: Duration,
    next: Duration,
    other: Duration,
    other_next: Duration,
}

/// The runner and the events of its requests.
struct Session {
    runner: LlamaCpp,
    sink: InferenceEventSink,
    events: mpsc::Receiver<InferenceEvent>,
}

impl Session {
    fn new(arguments: &Arguments) -> Self {
        let path = arguments.model.clone();
        let runner = LlamaCpp::new(move |_: &ModelFile| Some(path.clone()), arguments.cpu_cores);
        let (sender, events) = mpsc::channel();
        let sink: InferenceEventSink = Arc::new(move |event| {
            sender.send(event).ok();
        });
        Self { runner, sink, events }
    }

    fn submit(&mut self, request: InferenceRequest) {
        self.runner.submit(request, &self.sink);
    }

    fn submit_completion(&mut self, prompt: &str) {
        self.submit(InferenceRequest::Complete {
            prompt: Prompt(prompt.to_owned()),
            max_tokens: MAX_TOKENS,
            word_bias: None,
            next_word_bias: NextWordBias::default(),
            min_log_probability: None,
            typo_check: TypoCheck::Off,
            sentence_end: SentenceEnd::Stop,
            line_after: String::new(),
        });
    }

    fn wait_for_load(&self) -> anyhow::Result<()> {
        match self.events.recv()? {
            InferenceEvent::Loaded { result: Ok(()), .. } => Ok(()),
            event => bail!("the model did not load: {event:?}"),
        }
    }

    /// Waits for the next completion, and returns its time in the runner.
    fn wait_for_completion(&self) -> anyhow::Result<Duration> {
        loop {
            match self.events.recv()? {
                InferenceEvent::Partial(_) => {}
                InferenceEvent::Completed(Ok(completion)) => return Ok(completion.duration),
                event => bail!("expected a completion, got {event:?}"),
            }
        }
    }

    fn complete(&mut self, prompt: &str) -> anyhow::Result<Duration> {
        self.submit_completion(prompt);
        self.wait_for_completion()
    }

    /// Loads the model, times the completions, and unloads the model.
    fn time_load(&mut self, backend: Backend, first: FirstRequest) -> anyhow::Result<LoadTimes> {
        let file = ModelFile {
            repo: "local/model".to_owned(),
            name: "model.gguf".to_owned(),
        };
        let started = Instant::now();
        self.submit(InferenceRequest::Load { file, backend });
        if first == FirstRequest::Queued {
            self.submit_completion(FIRST_PROMPT);
        }
        self.wait_for_load()?;
        let load = started.elapsed();
        if first == FirstRequest::AfterLoad {
            self.submit_completion(FIRST_PROMPT);
        }
        let times = LoadTimes {
            load,
            first: self.wait_for_completion()?,
            next: self.complete(&format!("{FIRST_PROMPT}{MORE_WORDS}"))?,
            other: self.complete(OTHER_PROMPT)?,
            other_next: self.complete(&format!("{OTHER_PROMPT}{MORE_WORDS}"))?,
        };
        self.submit(InferenceRequest::Unload);
        Ok(times)
    }
}

fn median(mut times: Vec<Duration>) -> Option<Duration> {
    times.sort_unstable();
    times.get(times.len() / 2).copied()
}

fn write_times(output: &mut impl Write, label: &str, times: &LoadTimes) -> io::Result<()> {
    writeln!(
        output,
        "{label}: load {} ms, first {} ms, next {} ms, other {} ms, other next {} ms, first less other {} ms",
        times.load.as_millis(),
        times.first.as_millis(),
        times.next.as_millis(),
        times.other.as_millis(),
        times.other_next.as_millis(),
        times.first.saturating_sub(times.other).as_millis(),
    )
}

fn write_medians(
    output: &mut impl Write,
    way: FirstRequest,
    loads: &[(FirstRequest, LoadTimes)],
) -> io::Result<()> {
    let later = loads
        .iter()
        .skip(1)
        .filter(|(each_way, _)| *each_way == way)
        .map(|(_, times)| *times)
        .collect::<Vec<_>>();
    let of = |time: fn(&LoadTimes) -> Duration| median(later.iter().map(time).collect());
    let (Some(load), Some(first), Some(next), Some(other), Some(other_next)) = (
        of(|times| times.load),
        of(|times| times.first),
        of(|times| times.next),
        of(|times| times.other),
        of(|times| times.other_next),
    ) else {
        return Ok(());
    };
    let medians = LoadTimes {
        load,
        first,
        next,
        other,
        other_next,
    };
    write_times(
        output,
        &format!("median of {} later {} loads", later.len(), way.name()),
        &medians,
    )
}

fn main() -> anyhow::Result<()> {
    // SAFETY: no other thread runs yet.
    unsafe { typealike_llm::disable_metal_residency_sets() };
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let Some(arguments) = Arguments::parse(&arguments) else {
        bail!("{USAGE}");
    };
    let mut session = Session::new(&arguments);
    let mut output = io::stdout().lock();
    let mut loads = Vec::new();
    let mut way = arguments.first;
    for index in 1..=arguments.loads {
        let times = session.time_load(arguments.backend, way)?;
        write_times(&mut output, &format!("load {index} {}", way.name()), &times)?;
        loads.push((way, times));
        way = way.other();
    }
    write_medians(&mut output, FirstRequest::Queued, &loads)?;
    write_medians(&mut output, FirstRequest::AfterLoad, &loads)?;
    Ok(())
}
