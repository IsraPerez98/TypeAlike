//! The records of the suggestion log that the worker writes for each completion. They hold the
//! prompt and the text of the model, so only a build with the `suggestion-log` feature has them.

use std::time::Duration;

use tracing::span::EnteredSpan;
use typealike_core::SUGGESTION_LOG_TARGET;
use typealike_core::hub::ModelFile;
use typealike_core::inference::{Backend, Completion, InferenceError, InferenceRequest};

use super::generation::Stop;

/// The prompt tokens that a cache decoded for a completion, and the time that their decode took.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(super) struct Prefill {
    /// The tokens of the last prompt.
    prompt_tokens: usize,
    /// The tokens that the decodes added. The cache kept the other tokens of the prompt.
    decoded_tokens: usize,
    duration: Duration,
}

impl Prefill {
    /// Starts the decodes of a prompt of `prompt_tokens`. Its tokens in the cache need no decode.
    pub(super) fn start_prompt(&mut self, prompt_tokens: usize) {
        self.prompt_tokens = prompt_tokens;
    }

    /// Adds the decode of `decoded_tokens` of the prompt. Its `duration` must end after a read of
    /// the logits: a GPU backend decodes after the decode call returns, and the read waits for it.
    pub(super) fn add_batch(&mut self, decoded_tokens: usize, duration: Duration) {
        self.decoded_tokens += decoded_tokens;
        self.duration += duration;
    }

    /// The tokens of the prompt that came from the cache.
    fn cached_tokens(self) -> usize {
        self.prompt_tokens.saturating_sub(self.decoded_tokens)
    }
}

/// How long a completion waited in the queue and took, and its prompt decodes.
#[derive(Debug, Clone, Copy)]
pub(super) struct Timing {
    pub(super) queue_wait: Duration,
    /// The time from the start of the completion to its answer, also for a completion that a
    /// newer request replaced.
    pub(super) duration: Duration,
    pub(super) prefill: Prefill,
}

impl Timing {
    /// The time of the completion after its prompt decodes.
    fn decode(self) -> Duration {
        self.duration.saturating_sub(self.prefill.duration)
    }

    /// The tokens that the completion added in each second after its prompt decodes, or `None`
    /// without that time.
    fn tokens_per_second(self, tokens: u32) -> Option<f64> {
        let seconds = self.decode().as_secs_f64();
        (seconds > 0.0).then(|| f64::from(tokens) / seconds)
    }
}

/// Writes the load of the model of `file` on `backend`, which took `duration`, and its `error`.
pub(super) fn loaded(file: &ModelFile, backend: Backend, duration: Duration, error: Option<&InferenceError>) {
    tracing::info!(
        target: SUGGESTION_LOG_TARGET,
        repo = %file.repo,
        name = %file.name,
        ?backend,
        duration_ms = milliseconds(duration),
        error = error.map(tracing::field::debug),
        "loaded a model"
    );
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// What the records of the completions keep from one completion to the next.
#[derive(Debug, Default)]
pub(super) struct CompletionLog {
    /// The number of the last completion.
    completions: u64,
    /// The prompt of the last `completed` record.
    prompt: String,
}

impl CompletionLog {
    /// Starts the span of the next completion, with its number. Each record that the completion
    /// writes while the span is entered names that number, and so do the events of the completion
    /// that the platform gets while the span is entered.
    pub(super) fn start_completion(&mut self) -> EnteredSpan {
        self.completions += 1;
        tracing::info_span!(target: SUGGESTION_LOG_TARGET, "completion", completion = self.completions)
            .entered()
    }

    /// Writes the completion `request` and its `result`, when the log is on. The prompt of each
    /// record is the start of the prompt of the `completed` record before, up to
    /// `prompt_shared_chars` characters, and then `prompt_tail`. A request with a `max_tokens` of
    /// zero only prepares a prompt.
    pub(super) fn completed(
        &mut self,
        request: &InferenceRequest,
        result: Result<&(Completion, Stop), &InferenceError>,
        timing: Timing,
    ) {
        if !tracing::enabled!(target: SUGGESTION_LOG_TARGET, tracing::Level::INFO) {
            return;
        }
        let InferenceRequest::Complete {
            prompt,
            max_tokens,
            word_bias,
            next_word_bias,
            min_log_probability,
            typo_check,
            finished_word_fix,
            sentence_end,
            line_after,
        } = request
        else {
            return;
        };
        let shared_chars = shared_start_chars(&prompt.0, &self.prompt);
        let prompt_tail = prompt.0.chars().skip(shared_chars).collect::<String>();
        prompt.0.clone_into(&mut self.prompt);
        let completion = result.ok().map(|(completion, _)| completion);
        let stop = result.ok().map(|(_, stop)| stop);
        let fix_candidate = stop.and_then(|stop| stop.fix_candidate.as_ref());
        tracing::info!(
            target: SUGGESTION_LOG_TARGET,
            prompt_shared_chars = shared_chars,
            prompt_tail,
            max_tokens = max_tokens.0,
            min_log_probability = min_log_probability.map(|min| min.0),
            ?typo_check,
            finished_word_fix = finished_word_fix.map(|min| min.0),
            ?sentence_end,
            line_after = line_after.as_str(),
            word_bias_version = word_bias.as_ref().map(|bias| bias.version),
            next_words = ?next_word_bias.0,
            text = completion.map(|completion| completion.text.as_str()),
            tokens = completion.map(|completion| completion.tokens.0),
            queue_wait_ms = milliseconds(timing.queue_wait),
            duration_ms = milliseconds(timing.duration),
            prompt_tokens = timing.prefill.prompt_tokens,
            cached_tokens = timing.prefill.cached_tokens(),
            prefill_ms = milliseconds(timing.prefill.duration),
            decode_ms = milliseconds(timing.decode()),
            tokens_per_second = completion.and_then(|completion| timing.tokens_per_second(completion.tokens.0)),
            word_log_probabilities = completion.map(|completion| {
                tracing::field::debug(
                    completion.word_log_probabilities.iter().map(|word| word.0).collect::<Vec<_>>(),
                )
            }),
            typed_log_probability = completion.map(|completion| completion.typed_log_probability.0),
            correction_typed =
                completion.and_then(|completion| Some(completion.correction.as_ref()?.typed.as_str())),
            correction_fixed =
                completion.and_then(|completion| Some(completion.correction.as_ref()?.fixed.as_str())),
            stop = stop.map(|stop| tracing::field::debug(stop.reason)),
            chosen_tokens = stop.map(|stop| stop.chosen_tokens),
            dropped_text = stop.map(|stop| stop.dropped_text.as_str()),
            fix_candidate = fix_candidate.map(|fix| fix.letters.as_str()),
            fix_candidate_log_probability = fix_candidate.map(|fix| fix.log_probability.0),
            error = result.err().map(tracing::field::debug),
            "completed"
        );
    }
}

/// The number of characters at the start of `text` that a record takes from `earlier`: those that
/// both start with. Zero when they are less than half of `text`, so that the record of a new
/// session holds its full prompt, and a lost line does not change the prompts of later sessions.
fn shared_start_chars(text: &str, earlier: &str) -> usize {
    let shared = text
        .chars()
        .zip(earlier.chars())
        .take_while(|(character, earlier_character)| character == earlier_character)
        .count();
    if shared * 2 < text.chars().count() {
        0
    } else {
        shared
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timing(duration_ms: u64, prefill_ms: u64) -> Timing {
        let mut prefill = Prefill::default();
        prefill.start_prompt(100);
        prefill.add_batch(30, Duration::from_millis(prefill_ms));
        Timing {
            queue_wait: Duration::ZERO,
            duration: Duration::from_millis(duration_ms),
            prefill,
        }
    }

    #[test]
    fn prompt_tokens_that_no_decode_added_came_from_the_cache() {
        let mut decoded = Prefill::default();
        let mut cached = Prefill::default();

        decoded.start_prompt(100);
        decoded.add_batch(40, Duration::from_millis(5));
        decoded.add_batch(20, Duration::from_millis(5));
        cached.start_prompt(100);

        assert_eq!([decoded.cached_tokens(), cached.cached_tokens()], [40, 100]);
    }

    #[test]
    fn tokens_per_second_count_only_the_time_after_the_prompt_decodes() {
        let cases = [(timing(500, 100), Some(50.0)), (timing(100, 100), None)];

        let rates = cases.map(|(timing, _)| timing.tokens_per_second(20));

        assert_eq!(rates, cases.map(|(_, rate)| rate));
    }

    #[test]
    fn logged_prompt_takes_the_shared_start_only_when_it_is_at_least_half_of_the_prompt() {
        let earlier = "Notas: café con leche";
        let cases = [("Notas: café con pan", 16), ("Notas: otra cosa", 0), ("", 0)];

        let shared = cases.map(|(prompt, _)| shared_start_chars(prompt, earlier));

        assert_eq!(shared, cases.map(|(_, expected)| expected));
    }
}
