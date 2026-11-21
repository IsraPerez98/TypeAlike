//! Builds the requests to the model runner, with the word bias and the phrases of the writer.

use std::mem;

use crate::desktop::{AfterCaret, Desktop};
use crate::history::HistoryPool;
use crate::inference::{
    Correction, InferenceRequest, LogProbability, MIN_FIXED_CHARACTERS, NextWordBias, Prompt, SentenceEnd,
    TokenCount, TypoCheck, WordBias,
};
use crate::key::Press;
use crate::place::Place;
use crate::prompt;

use super::offer::{KnownWord, Offer};
use super::session::Session;
use super::spelling::accent_fix;
use super::suggestion::through_finished_word;
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
        let prompt = prompt::prompt(&session.preface.text, &session.text_before);
        let line_after = session.line_after().to_owned();
        self.unanswered_requests = self.unanswered_requests.saturating_add(1);
        Some(InferenceRequest::Complete {
            prompt,
            max_tokens: PREPARATION_TOKENS,
            word_bias: None,
            next_word_bias: NextWordBias::default(),
            min_log_probability: None,
            typo_check: TypoCheck::Off,
            finished_word_fix: None,
            sentence_end: SentenceEnd::Continue,
            line_after,
        })
    }

    /// Suggests after a key that changed the text. A key that the OS repeats while the user holds
    /// it only hides the ghost text, and the suggestion waits for
    /// [`suggest_when_due`](Self::suggest_when_due).
    pub(super) fn suggest_after(&mut self, press: Press) -> Reaction {
        match press {
            Press::First => self.suggest(),
            Press::Repeat => {
                self.is_suggestion_due = true;
                self.dismiss()
            }
        }
    }

    /// Suggests when the input settles after the text before the caret changed without a
    /// suggestion: the OS repeated a held key, or a read of the field changed the text of an
    /// untouched suggestion. `None` when no suggestion waits.
    pub(crate) fn suggest_when_due(&mut self) -> Option<Reaction> {
        mem::take(&mut self.is_suggestion_due).then(|| self.suggest())
    }

    /// Shows the rest of a suggestion that showed in the session, else the rest of a phrase that
    /// the writer repeats, else requests a completion. A paused field gets nothing.
    pub(super) fn suggest(&mut self) -> Reaction {
        self.is_suggestion_due = false;
        if self.is_field_paused {
            return self.dismiss();
        }
        if let Some(rest) = self.rest_of_shown_suggestion().or_else(|| self.repeated_phrase()) {
            return self.show(rest);
        }
        self.request_at_caret()
    }

    /// Requests the suggestion for the text before the caret again, for when the preface of the
    /// session changed after the request of the suggestion that runs or shows.
    pub(crate) fn request_again(&mut self) -> Reaction {
        self.request_at_caret()
    }

    /// Requests a completion of the text before the caret with the typo checks that it needs.
    fn request_at_caret(&mut self) -> Reaction {
        let typo_check = self.typo_check_at_caret();
        let finished_word_fix = self.finished_word_fix_at_caret();
        self.request_completion(typo_check, finished_word_fix)
    }

    /// The typo check of a request for the text before the caret. With a dictionary, the typed
    /// characters of the last word get no typo rule when they are fewer than
    /// [`MIN_FIXED_CHARACTERS`], or when a dictionary of the language of the text knows them as a
    /// word. After a stop for a typo there, core always asks again without the rule, because the
    /// dictionary does not check such letters or knows them, and the runner gives so few letters
    /// no fix.
    fn typo_check_at_caret(&mut self) -> TypoCheck {
        let (Some(dictionary), Some(session)) = (self.dictionary.as_mut(), self.session.as_ref()) else {
            return self.typo_check;
        };
        let typed = session
            .text_before
            .rsplit(|character: char| character.is_ascii_whitespace())
            .next()
            .unwrap_or_default();
        let is_too_short = (1..MIN_FIXED_CHARACTERS).contains(&typed.chars().count());
        if is_too_short || dictionary.needs_no_typo_check(&session.text_before) {
            TypoCheck::Off
        } else {
            self.typo_check
        }
    }

    /// The minimum for the finished word before the last typed space of the text before the caret,
    /// when a request checks that word for a typo: the request asks for fixes, only that space or
    /// the typed start of the next word follows the word ([`through_finished_word`]), no completed
    /// request checked the word, and no dictionary of the language of the text knows it. The user
    /// often types the next word before the request after the space completes, and then no
    /// request checked it. Core shows no fix of a known word there.
    fn finished_word_fix_at_caret(&mut self) -> Option<LogProbability> {
        let TypoCheck::Fix(min) = self.typo_check else {
            return None;
        };
        let session = self.session.as_ref()?;
        let through_word = through_finished_word(&session.text_before)?;
        let is_known = self
            .dictionary
            .as_mut()
            .is_some_and(|dictionary| dictionary.needs_no_typo_check(through_word));
        (through_word != session.checked_word && !is_known).then_some(min)
    }

    /// Requests a completion of the text before the caret with `typo_check` and
    /// `finished_word_fix`. When `typo_check` asks for fixes and the dictionary fixes the accents
    /// of the finished word before a typed space ([`accent_fix`]), the prompt holds the fixed word,
    /// and the request has no typo rule and no check of that word.
    pub(super) fn request_completion(
        &mut self,
        typo_check: TypoCheck,
        finished_word_fix: Option<LogProbability>,
    ) -> Reaction {
        let accent_fix = self.accent_fix_at_caret(typo_check);
        let (typo_check, finished_word_fix) = if accent_fix.is_some() {
            (TypoCheck::Off, None)
        } else {
            (typo_check, finished_word_fix)
        };
        let Some(prompt) = self.prompt(accent_fix.as_ref()) else {
            self.offer = Offer::Idle;
            return Reaction::Hide;
        };
        if let Some(session) = &mut self.session {
            session.has_untouched_request = true;
        }
        self.offer = Offer::Growing {
            typed: String::new(),
            shown: String::new(),
            added: String::new(),
            typo_check,
            finished_word_fix,
            known_word: KnownWord::First,
            accent_fix,
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
            typo_check,
            finished_word_fix,
            sentence_end: self.sentence_end,
            line_after,
        })
    }

    /// The fix of the accents of the finished word before a typed space at the caret, when
    /// `typo_check` asks for fixes and a dictionary gives one.
    fn accent_fix_at_caret(&mut self, typo_check: TypoCheck) -> Option<Correction> {
        let TypoCheck::Fix(_) = typo_check else {
            return None;
        };
        let session = self.session.as_ref()?;
        accent_fix(self.dictionary.as_mut()?, &session.text_before)
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
    pub(super) fn session_word_bias(&self) -> Option<WordBias> {
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

    /// Whether the earlier writing gives a phrase or next words after the text before the caret.
    /// A new request uses them, but a running completion of an earlier text does not.
    pub(super) fn has_writer_words(&self) -> bool {
        self.repeated_phrase().is_some() || !self.session_next_word_bias().0.is_empty()
    }

    /// The rest of a suggestion that showed in the session, which the text before the caret
    /// continues.
    fn rest_of_shown_suggestion(&self) -> Option<String> {
        let session = self.session.as_ref()?;
        session
            .shown_suggestions
            .rest_after(&session.text_before)
            .map(str::to_owned)
    }

    /// The rest of a phrase that the writer repeats after the text before the caret, in the pool
    /// of the session, when the line after the caret is empty.
    fn repeated_phrase(&self) -> Option<String> {
        let (session, pool, place) = self.session_pool()?;
        if session.field.is_excluded || !session.is_caret_at_line_end(AfterCaret::Unknown) {
            return None;
        }
        self.tables
            .continuation_of(&pool, place, &session.text_before)
            .map(str::to_owned)
    }

    /// Returns the prompt for the text before the caret, with the fixed text of `accent_fix` in
    /// place of its typed text, if any. `None` when that text has no words or the field gets no
    /// suggestions.
    fn prompt(&self, accent_fix: Option<&Correction>) -> Option<Prompt> {
        let session = self
            .session
            .as_ref()
            .filter(|session| !session.field.is_excluded)?;
        let has_words = !session.text_before.trim().is_empty();
        let text_before = match accent_fix {
            Some(fix) => format!(
                "{}{}",
                session.text_before.strip_suffix(fix.typed.as_str())?,
                fix.fixed
            ),
            None => session.text_before.clone(),
        };
        has_words.then(|| prompt::prompt(&session.preface.text, &text_before))
    }
}
