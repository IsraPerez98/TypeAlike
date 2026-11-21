//! The records of the suggestion log that the worker writes for each completion. They hold the
//! prompt and the text of the model, so only a build with the `suggestion-log` feature has them.

use tracing::span::EnteredSpan;
use typealike_core::SUGGESTION_LOG_TARGET;
use typealike_core::inference::{Completion, InferenceError, InferenceRequest};

use super::generation::Stop;

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
            ?sentence_end,
            line_after = line_after.as_str(),
            word_bias_version = word_bias.as_ref().map(|bias| bias.version),
            next_words = ?next_word_bias.0,
            text = completion.map(|completion| completion.text.as_str()),
            tokens = completion.map(|completion| completion.tokens.0),
            duration_ms = completion.map(|completion| completion.duration.as_secs_f64() * 1000.0),
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

    #[test]
    fn logged_prompt_takes_the_shared_start_only_when_it_is_at_least_half_of_the_prompt() {
        let earlier = "Notas: café con leche";
        let cases = [("Notas: café con pan", 16), ("Notas: otra cosa", 0), ("", 0)];

        let shared = cases.map(|(prompt, _)| shared_start_chars(prompt, earlier));

        assert_eq!(shared, cases.map(|(_, expected)| expected));
    }
}
