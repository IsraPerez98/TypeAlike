//! Builds the requests to the model runner, with the word bias and the phrases of the writer.

use crate::desktop::Desktop;
use crate::history::HistoryPool;
use crate::inference::{
    InferenceRequest, LogProbability, NextWordBias, Prompt, TokenCount, TypoCheck, WordBias,
};
use crate::place::Place;
use crate::prompt;

use super::offer::Offer;
use super::session::Session;
use super::{Engine, Reaction};

const PREPARATION_TOKENS: TokenCount = TokenCount(0);
/// How many times more tokens a suggestion inside a line can add. There the end that fits the text
/// after the caret ends the suggestion, so a sentence that the writer inserts can finish.
const INSIDE_LINE_TOKEN_FACTOR: u32 = 2;

impl Engine {
    /// Starts the session for the focused field of `desktop` when none runs, and returns the
    /// request that decodes its prompt before the next key. Its completion adds no text. `None`
    /// when a completion of the session runs or the field gets no suggestions.
    pub(crate) fn prepare(&mut self, desktop: &dyn Desktop) -> Option<InferenceRequest> {
        let is_completing = self.session.is_some() && self.unanswered_requests > 0;
        if is_completing {
            return None;
        }
        let session = self.start_session(desktop);
        if session.field.is_excluded {
            return None;
        }
        let prompt = prompt::prompt(&session.preface, &session.text_before);
        let line_after = session.line_after().to_owned();
        self.unanswered_requests = self.unanswered_requests.saturating_add(1);
        Some(InferenceRequest::Complete {
            prompt,
            max_tokens: PREPARATION_TOKENS,
            word_bias: None,
            next_word_bias: NextWordBias::default(),
            min_log_probability: None,
            typo_check: TypoCheck::Off,
            line_after,
        })
    }

    /// Shows the rest of a phrase that the writer repeats, else requests a completion.
    pub(super) fn suggest(&mut self) -> Reaction {
        if let Some(phrase) = self.repeated_phrase() {
            return self.show(phrase);
        }
        let Some(prompt) = self.prompt() else {
            self.offer = Offer::Idle;
            return Reaction::Hide;
        };
        self.offer = Offer::Growing {
            typed: String::new(),
            shown: String::new(),
        };
        self.unanswered_requests = self.unanswered_requests.saturating_add(1);
        let line_after = self
            .session
            .as_ref()
            .map(|session| session.line_after().to_owned())
            .unwrap_or_default();
        let (max_tokens, min_log_probability) = self.limits_before(&line_after);
        Reaction::Request(InferenceRequest::Complete {
            prompt,
            max_tokens,
            word_bias: self.session_word_bias(),
            next_word_bias: self.session_next_word_bias(),
            min_log_probability,
            typo_check: self.typo_check,
            line_after,
        })
    }

    /// The most tokens and the cut of the tail of a suggestion before `line_after`, the text after
    /// the caret on its line. Inside a line, the end that fits that text ends the suggestion, so
    /// the request has no cut and more tokens.
    fn limits_before(&self, line_after: &str) -> (TokenCount, Option<LogProbability>) {
        let max_tokens = self.suggestion_length.max_tokens();
        if line_after.is_empty() {
            (max_tokens, Some(self.suggestion_length.tail_cut()))
        } else {
            (
                TokenCount(max_tokens.0.saturating_mul(INSIDE_LINE_TOKEN_FACTOR)),
                None,
            )
        }
    }

    /// The pool of the earlier writing that the session uses, and the place of the session. `None`
    /// when no session runs or its app is off.
    fn session_pool(&self) -> Option<(&Session, HistoryPool, Place<&str>)> {
        let session = self.session.as_ref()?;
        let app = session.field.app.as_ref()?;
        let pool = HistoryPool::of(&app.app_id, &self.personalization)?;
        Some((
            session,
            pool,
            Place::of(&app.app_id, session.field.domain.as_deref()),
        ))
    }

    /// The word bias of the pool of the session.
    fn session_word_bias(&self) -> Option<WordBias> {
        let (_, pool, _) = self.session_pool()?;
        self.tables.word_bias(&pool)
    }

    /// The words that the writer wrote after the last words before the caret, in the pool of the
    /// session.
    fn session_next_word_bias(&self) -> NextWordBias {
        self.session_pool()
            .map(|(session, pool, place)| self.tables.next_word_bias(&pool, place, &session.text_before))
            .unwrap_or_default()
    }

    /// The rest of a phrase that the writer repeats after the text before the caret, in the pool
    /// of the session, when the line after the caret is empty.
    fn repeated_phrase(&self) -> Option<String> {
        let (session, pool, place) = self.session_pool()?;
        if session.field.is_excluded || !session.is_caret_at_line_end() {
            return None;
        }
        self.tables
            .continuation_of(&pool, place, &session.text_before)
            .map(str::to_owned)
    }

    /// Returns the prompt for the text before the caret, or `None` when that text has no words or
    /// the field gets no suggestions.
    fn prompt(&self) -> Option<Prompt> {
        let session = self
            .session
            .as_ref()
            .filter(|session| !session.field.is_excluded)?;
        let has_words = !session.text_before.trim().is_empty();
        has_words.then(|| prompt::prompt(&session.preface, &session.text_before))
    }
}
