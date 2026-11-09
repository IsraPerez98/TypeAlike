//! Grows the suggestion from the completions and tracks what the ghost text offers.

use std::mem;

use crate::inference::{Completion, Correction, InferenceError, TypoCheck};
use crate::settings::TypoFix;

use super::spelling::{WrittenWords, known_start};
use super::suggestion::{
    complete_words, is_bad_suggestion, suggestion_after, suggestion_after_typed, word_fix,
};
use super::{Engine, Reaction, WordFix};

#[derive(Default)]
pub(super) enum Offer {
    #[default]
    Idle,
    /// The completion of the newest request runs, and its parts grow the ghost text.
    Growing {
        /// The text that the user typed or accepted after the request, which continues `added`.
        typed: String,
        /// The ghost text after `typed`, which is empty before the first part.
        shown: String,
        /// The text that the completion added so far after the text of the request, also the
        /// text that the ghost text does not show yet.
        added: String,
        /// The typo check of the request.
        typo_check: TypoCheck,
    },
    Shown(String),
    /// The fix of a typo before the caret.
    Fix(WordFix),
}

impl Offer {
    /// Returns the ghost text, which is empty when none shows.
    pub(super) fn shown(&self) -> &str {
        match self {
            Self::Idle | Self::Fix(_) => "",
            Self::Growing { shown, .. } | Self::Shown(shown) => shown,
        }
    }

    /// Records that `taken`, the start of the ghost text, is now before the caret, and that
    /// `rest` stays as the ghost text.
    pub(super) fn take(&mut self, taken: &str, rest: &str) {
        match self {
            Self::Growing { typed, shown, .. } => {
                typed.push_str(taken);
                rest.clone_into(shown);
            }
            Self::Idle | Self::Shown(_) | Self::Fix(_) => {
                *self = if rest.is_empty() {
                    Self::Idle
                } else {
                    Self::Shown(rest.to_owned())
                };
            }
        }
    }

    /// Returns the reaction that shows the ghost text, or hides it when it is empty.
    pub(super) fn reaction(&self) -> Reaction {
        if let Self::Fix(fix) = self {
            return Reaction::ShowFix(fix.clone());
        }
        match self.shown() {
            "" => Reaction::Hide,
            shown => Reaction::Show(shown.to_owned()),
        }
    }

    /// Returns what stays after the newest completion ends: the ghost text or the fix, else
    /// nothing.
    fn settle(self) -> Self {
        match self {
            Self::Growing { shown, .. } | Self::Shown(shown) if !shown.is_empty() => Self::Shown(shown),
            Self::Fix(fix) => Self::Fix(fix),
            Self::Idle | Self::Growing { .. } | Self::Shown(_) => Self::Idle,
        }
    }
}

impl Engine {
    /// Handles the completion of the oldest [`Reaction::Request`] that has no completion yet.
    /// Only the completion of the newest request can show. When the user typed or accepted text
    /// after the newest request and its completion gives no ghost text after that text, it returns
    /// what a typed key gets, because the completion ended before the text that the user waits
    /// for. A failed completion gets nothing.
    pub(crate) fn on_completion(&mut self, result: Result<Completion, InferenceError>) -> Reaction {
        self.unanswered_requests = self.unanswered_requests.saturating_sub(1);
        if self.unanswered_requests > 0 {
            return self.offer.reaction();
        }
        if result
            .as_ref()
            .is_ok_and(|completion| self.has_typo_in_known_letters(completion))
        {
            return self.request_completion(TypoCheck::Off);
        }
        let has_typed = matches!(&self.offer, Offer::Growing { typed, .. } if !typed.is_empty());
        let is_answered = result.is_ok();
        match result {
            Ok(Completion {
                correction: None,
                text,
                ..
            }) => {
                self.grow(&text);
            }
            Ok(Completion {
                correction: Some(correction),
                text,
                ..
            }) => {
                if let Some(fix) = self.fix_of(&correction, &text) {
                    self.offer = Offer::Fix(fix);
                }
            }
            Err(_) => {}
        }
        self.offer = mem::take(&mut self.offer).settle();
        if is_answered && has_typed && matches!(self.offer, Offer::Idle) {
            return self.suggest();
        }
        self.offer.reaction()
    }

    /// Whether the `completion` of the newest request, with nothing typed since, stopped for a typo
    /// in typed letters that the dictionary knows as a word or the start of a word, or fixed a
    /// known word.
    fn has_typo_in_known_letters(&mut self, completion: &Completion) -> bool {
        let Offer::Growing {
            typed, typo_check, ..
        } = &self.offer
        else {
            return false;
        };
        let Some(min) = typo_check.min_log_probability().filter(|_| typed.is_empty()) else {
            return false;
        };
        let (Some(dictionary), Some(session)) = (self.dictionary.as_mut(), self.session.as_ref()) else {
            return false;
        };
        let text_before = &session.text_before;
        match completion.correction {
            Some(_) => dictionary.knows_typed_word(text_before),
            None => completion.typed_log_probability < min && dictionary.knows_typed_start(text_before),
        }
    }

    /// Returns the fix that the `correction` and the added `text` of the newest request give, when
    /// the user typed nothing since the request, the text before the caret ends with the typo and
    /// the suggestion after the fixed letters is not bad. The dictionary cuts that suggestion
    /// before a made-up word, and a made-up fixed word gives no fix.
    fn fix_of(&mut self, correction: &Correction, text: &str) -> Option<WordFix> {
        let is_untouched = matches!(&self.offer, Offer::Growing { typed, .. } if typed.is_empty());
        if !is_untouched || self.typo_fix == TypoFix::Off {
            return None;
        }
        let word_bias = self.session_word_bias();
        let session = self.session.as_ref()?;
        let before_typo = session.text_before.strip_suffix(correction.typed.as_str())?;
        let fixed_before = format!("{before_typo}{}", correction.fixed);
        let suggestion = suggestion_after(&fixed_before, text, &session.text_after).unwrap_or_default();
        if !suggestion.is_empty()
            && is_bad_suggestion(&suggestion, &fixed_before, &session.text_after, &session.preface)
        {
            return None;
        }
        let written = WrittenWords {
            session,
            word_bias: word_bias.as_ref(),
        };
        let fixed_text = format!("{}{suggestion}", correction.fixed);
        let known = known_start(self.dictionary.as_mut(), before_typo, &fixed_text, &written);
        let rest = known.strip_prefix(correction.fixed.as_str())?;
        Some(word_fix(correction, rest))
    }

    /// Handles `text`, the text that the completion of the oldest [`Reaction::Request`] added so
    /// far. Only its complete words show, and only for the newest request. Returns the reaction
    /// when the ghost text changes, or `None`.
    pub(crate) fn on_partial_completion(&mut self, text: &str) -> Option<Reaction> {
        if self.unanswered_requests != 1 {
            return None;
        }
        self.record_partial_completion(text);
        self.grow(complete_words(text)).then(|| self.offer.reaction())
    }

    /// Records `text`, the text that the completion of the oldest [`Reaction::Request`] added so
    /// far, when that request is the newest. The ghost text does not change. While the typed keys
    /// continue this text, the request keeps running.
    pub(crate) fn record_partial_completion(&mut self, text: &str) {
        if self.unanswered_requests != 1 {
            return;
        }
        if let Offer::Growing { added, .. } = &mut self.offer {
            text.clone_into(added);
        }
    }

    /// Whether the completion of the newest request keeps running after `character`, the last
    /// character before the caret: its added text continues the typed text with `character`, and
    /// the earlier writing gives a new request no phrase and no next words.
    pub(super) fn keeps_completion_after(&self, character: char) -> bool {
        self.is_continued_by(character) && !self.has_writer_words()
    }

    /// Whether the completion of the newest request runs and its added text, as ghost text, starts
    /// with the typed text and then `character`, the last character before the caret. A bad
    /// suggestion does not count.
    fn is_continued_by(&self, character: char) -> bool {
        let (Some(session), Offer::Growing { typed, added, .. }) = (&self.session, &self.offer) else {
            return false;
        };
        let typed = format!("{typed}{character}");
        let Some(context) = session.text_before.strip_suffix(typed.as_str()) else {
            return false;
        };
        suggestion_after(context, added, &session.text_after).is_some_and(|suggestion| {
            suggestion.starts_with(typed.as_str())
                && !is_bad_suggestion(&suggestion, context, &session.text_after, &session.preface)
        })
    }

    /// Makes the ghost text the text of `completion`, from the newest request, after the typed
    /// text, when it has text there, and keeps it in the shown suggestions of the session. A bad
    /// suggestion empties the ghost text, and the dictionary cuts it before a made-up word.
    /// Returns whether the ghost text changed.
    fn grow(&mut self, completion: &str) -> bool {
        let word_bias = self.session_word_bias();
        let (Some(session), Offer::Growing { typed, shown, .. }) = (&mut self.session, &mut self.offer)
        else {
            return false;
        };
        let Some(suggestion) = suggestion_after_typed(
            &session.text_before,
            &session.text_after,
            typed,
            completion,
            &session.preface,
        ) else {
            return false;
        };
        let written = WrittenWords {
            session,
            word_bias: word_bias.as_ref(),
        };
        let grown = known_start(
            self.dictionary.as_mut(),
            &session.text_before,
            &suggestion,
            &written,
        )
        .to_owned();
        if grown == *shown {
            return false;
        }
        session.shown_suggestions.record(&session.text_before, &grown);
        *shown = grown;
        true
    }

    pub(super) fn dismiss(&mut self) -> Reaction {
        self.offer = Offer::Idle;
        Reaction::Hide
    }

    /// Hides the ghost text, and forgets the shown suggestions that continue the text before the
    /// caret, so that the suggestion that the user rejected does not show again without the model.
    pub(super) fn reject(&mut self) -> Reaction {
        if let Some(session) = self.session.as_mut() {
            session.shown_suggestions.forget_at(&session.text_before);
        }
        self.dismiss()
    }

    /// Rejects the suggestion that shows, and pauses the suggestions in the field until
    /// [`end_field_pause`](Engine::end_field_pause).
    pub(super) fn pause_field(&mut self) -> Reaction {
        self.reject();
        self.is_field_paused = true;
        Reaction::PauseField
    }

    pub(super) fn show(&mut self, suggestion: String) -> Reaction {
        self.offer = Offer::Shown(suggestion.clone());
        Reaction::Show(suggestion)
    }
}
