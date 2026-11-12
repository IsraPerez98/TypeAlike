//! What a completion takes and gives. It has the decoder, the limits, the progress and the healed
//! text. It also has the state of each step and the added tokens with their text.

use std::str;

use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::token_type::{LlamaTokenAttr, LlamaTokenAttrs};
use llama_cpp_2::vocab::LlamaVocab;
use typealike_core::inference::{Correction, LogProbability, SentenceEnd, TypoCheck};

use super::correction::TypedLetters;
use super::logits::{
    TokenBias, agreeing_log_probability, choose_token, log_probability_of, most_likely_tokens,
    typed_log_probability,
};
use super::pieces::TokenPieces;
use super::words::{WordLogProbabilities, ends_phrase, ends_sentence, starts_next_sentence, starts_word};

/// The most text at the end of a prompt that generation takes back and writes again.
const MAX_HEALED_BYTES: usize = 24;
/// The most text at the start of the line after the caret that a join step compares with the text
/// of the tokens: more than a token holds.
const MAX_JOINED_BYTES: usize = 64;
/// The least likely that the model can find the end of a phrase next, after a word that ends a
/// phrase: about 10 percent. After a word that needs a next word, such as "the" or "to", the
/// model almost never finds it this likely.
const MIN_PHRASE_END: LogProbability = LogProbability(-2.3);
/// The tokens that a completion can add after the cut of the tail or the token limit, to reach the
/// end of a phrase, in percent of the token limit.
const PHRASE_END_TOKENS_PERCENT: usize = 40;
/// The most likely tokens where a completion looks for a token that keeps its text on the line,
/// when the most likely token must not come next.
const SAME_LINE_CANDIDATES: usize = 64;

/// Where a completion stops adding tokens.
#[derive(Debug, Clone, Copy)]
pub(super) struct Limits<'text> {
    pub(super) max_added: usize,
    /// The least likely that the added text can get after its first word.
    pub(super) min_log_probability: Option<LogProbability>,
    /// What the completion does when the healed text looks like a typo.
    pub(super) typo_check: TypoCheck,
    /// Whether the completion stops at the end of a sentence that it adds.
    pub(super) sentence_end: SentenceEnd,
    /// The text after the caret on its line, which the added text leads into, or empty.
    pub(super) line_after: &'text [u8],
}

impl Limits<'_> {
    /// The most tokens that a completion adds, with the tokens that it can add to reach the end of
    /// a phrase.
    pub(super) fn most_added(&self) -> usize {
        self.max_added + self.phrase_end_tokens()
    }

    /// The tokens that a completion can add after the cut of the tail or the token limit to reach
    /// the end of a phrase. Inside a line, the end that fits the line after the caret ends the
    /// added text instead.
    fn phrase_end_tokens(&self) -> usize {
        if self.line_after.is_empty() {
            self.max_added * PHRASE_END_TOKENS_PERCENT / 100
        } else {
            0
        }
    }
}

/// A place where the added text can end: after its first `tokens` tokens and `words` words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TextEnd {
    tokens: usize,
    words: usize,
}

/// A stop of the cut of the tail or the token limit after a word that ends no phrase. The
/// completion goes on until the end of a phrase, for at most `max_tokens`, and goes back to
/// `fallback` when it stops without one.
#[derive(Debug, Clone, Copy)]
struct PendingStop {
    fallback: TextEnd,
    max_tokens: usize,
}

/// What a running completion asks and tells its job.
pub(super) struct Progress<'job> {
    /// Tells that a newer completion waits, so that this completion must stop.
    pub(super) is_outdated: &'job mut dyn FnMut() -> bool,
    /// Receives the text that the completion added so far.
    pub(super) send_partial: &'job mut dyn FnMut(String),
}

/// What each step of a completion reads: the vocabulary, the text of each token, which finds the
/// tokens that agree with the healed text, the bias of the logits and the limits.
#[derive(Clone, Copy)]
pub(super) struct Decoder<'run> {
    pub(super) vocab: &'run LlamaVocab<'run>,
    pub(super) pieces: &'run TokenPieces,
    pub(super) token_bias: &'run TokenBias,
    pub(super) limits: Limits<'run>,
}

/// The text that the first added tokens write again.
#[derive(Clone, Copy)]
pub(super) struct Healing<'text> {
    pub(super) text: &'text [u8],
    /// The token that the first step takes, from the fix of a typo, if any.
    pub(super) first_token: Option<LlamaToken>,
}

/// The tokens that a completion added, how likely the model found each word of them, and how likely
/// it found the healed text.
pub(super) struct AddedTokens {
    pub(super) tokens: Vec<LlamaToken>,
    pub(super) word_log_probabilities: Vec<LogProbability>,
    /// How likely the model found the healed text, up to the stop for a typo.
    pub(super) typed_log_probability: LogProbability,
    /// The fix of a typo in the healed text, which the tokens continue, if any.
    pub(super) correction: Option<Correction>,
}

/// What a completion does with the next token that it chose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Choice {
    /// Adds the token, with the natural log of its probability.
    Add(LlamaToken, f32),
    /// Starts the completion again with the token as its first token.
    StartWith(LlamaToken),
}

/// Whether a completion goes on after a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    Continue,
    Stop,
}

/// The state of a running completion: the tokens that it added, the text that they wrote, and how
/// likely the model found each word and the healed text. It decides where the completion stops,
/// without the model.
pub(super) struct Generation<'text> {
    pieces: &'text TokenPieces,
    healing: Healing<'text>,
    /// The end of the healed text that the added tokens did not write yet.
    unwritten: &'text [u8],
    tokens: Vec<LlamaToken>,
    /// The text that the added tokens wrote after the healed text.
    written: Vec<u8>,
    words: WordLogProbabilities,
    /// The log probability of the added text before the sentence that it writes now. The cut of
    /// the tail applies to each sentence on its own.
    log_probability_before_sentence: f32,
    typed: TypedLetters,
    /// The end of the added text where the model finds the added text and then the line after the
    /// caret most likely, if any.
    best_end: Option<JoinEnd>,
    /// The last end after a word where the added text ends a phrase, if any.
    phrase_end: Option<TextEnd>,
    pending_stop: Option<PendingStop>,
    /// The most likely first token that writes a word after a typed space, and the natural log of
    /// its probability, when the first token wrote only whitespace.
    first_word: Option<(LlamaToken, f32)>,
}

/// A place where the added text can end before the line after the caret.
#[derive(Debug, Clone, Copy)]
struct JoinEnd {
    tokens: usize,
    words: usize,
    /// The log probability of the added text up to here, and then of the start of the line after.
    log_probability: f32,
}

impl<'text> Generation<'text> {
    /// Starts a completion whose first tokens write the `healing` text again. The `pieces` find the
    /// tokens that agree with that text.
    pub(super) fn new(pieces: &'text TokenPieces, healing: Healing<'text>) -> Self {
        Self {
            pieces,
            healing,
            unwritten: healing.text,
            tokens: Vec::new(),
            written: Vec::new(),
            words: WordLogProbabilities::default(),
            log_probability_before_sentence: 0.0,
            typed: TypedLetters::default(),
            best_end: None,
            phrase_end: None,
            pending_stop: None,
            first_word: None,
        }
    }

    /// The tokens that the completion added so far.
    pub(super) fn tokens(&self) -> &[LlamaToken] {
        &self.tokens
    }

    /// Whether the added tokens wrote all of the healed text.
    pub(super) fn is_healed(&self) -> bool {
        self.unwritten.is_empty()
    }

    /// Whether the completion can add another token within the `limits`.
    pub(super) fn has_room(&self, limits: Limits<'_>) -> bool {
        self.tokens.len() < limits.max_added || self.pending_stop.is_some()
    }

    /// The tokens that show while the completion runs: the tokens up to the best end before the
    /// line after the caret, else all the tokens. When a stop waits for the end of a phrase, they
    /// end at the place where that stop goes back to, else at the last end of a phrase, so that
    /// the shown text never gets shorter. They keep the token after that place, which starts the
    /// next word, because only the words before whitespace show.
    pub(super) fn shown_tokens(&self) -> &[LlamaToken] {
        let shown = self.shown_counts().0;
        let held = self.pending_stop.map(|stop| stop.fallback).or(self.phrase_end);
        let end = held.map_or(shown, |held| shown.min(held.tokens + 1));
        &self.tokens[..end]
    }

    fn end_here(&self) -> TextEnd {
        TextEnd {
            tokens: self.tokens.len(),
            words: self.words.count(),
        }
    }

    /// Notes the end of the added text before a token that adds `text`, when that token starts a
    /// word, as [`Generation::note_phrase_end`] does.
    fn note_word_end(&mut self, text: &[u8], logits: &[f32]) -> bool {
        starts_word(text) && self.note_phrase_end(logits)
    }

    /// Notes the end of the added text here, after written text. Returns whether it ends a phrase:
    /// the added text ends with a sign that ends a phrase, or `logits` find the end of a phrase
    /// next at least as likely as [`MIN_PHRASE_END`].
    fn note_phrase_end(&mut self, logits: &[f32]) -> bool {
        if !self.has_written_text() {
            return false;
        }
        let is_phrase_end = ends_phrase(&self.written)
            || typed_log_probability(logits, self.pieces.phrase_ends()) >= MIN_PHRASE_END.0;
        if is_phrase_end {
            self.phrase_end = Some(self.end_here());
        }
        is_phrase_end
    }

    fn truncate_to(&mut self, end: TextEnd) {
        self.tokens.truncate(end.tokens);
        self.words.truncate(end.words);
    }

    /// Stops at `stop`, where the cut of the tail or the token limit ends the added text, when it
    /// ends a phrase or the `limits` allow no tokens to reach the end of a phrase. Else the
    /// completion goes on, and goes back to the last end of a phrase, or to `stop`, when no end of
    /// a phrase comes.
    fn stop_at(&mut self, stop: TextEnd, limits: Limits<'_>) -> Step {
        let extra_tokens = limits.phrase_end_tokens();
        if self.phrase_end == Some(stop) || extra_tokens == 0 {
            self.truncate_to(stop);
            return Step::Stop;
        }
        self.pending_stop = Some(PendingStop {
            fallback: self.phrase_end.unwrap_or(stop),
            max_tokens: self.tokens.len() + extra_tokens,
        });
        Step::Continue
    }

    /// Stops at the token limit when the added text ends a sentence, else at the end without the
    /// last word, as [`Generation::stop_at`] does.
    fn stop_at_token_limit(&mut self, limits: Limits<'_>) -> Step {
        if ends_sentence(self.healing.text, &self.written) {
            return Step::Stop;
        }
        let dropped = self.words.last_word_tokens_to_drop();
        let stop = TextEnd {
            tokens: self.tokens.len() - dropped,
            words: self.words.count() - usize::from(dropped > 0),
        };
        self.stop_at(stop, limits)
    }

    /// Adds a join step with `logits`, which follow the added text: it keeps the end of the added
    /// text where the model finds the added text and then the start of `line_after` most likely.
    /// Only an end after written text counts, and only when a token can start `line_after`. A token
    /// of only whitespace does not count, because it also comes before other text, such as digits.
    pub(super) fn add_join_step(&mut self, logits: &[f32], line_after: &[u8]) {
        if line_after.is_empty() || !self.has_written_text() {
            return;
        }
        let start = &line_after[..line_after.len().min(MAX_JOINED_BYTES)];
        let join = typed_log_probability(logits, self.pieces.agreeing_past_whitespace(start));
        let end = JoinEnd {
            tokens: self.tokens.len(),
            words: self.words.count(),
            log_probability: self.words.total() + join,
        };
        let is_better = self
            .best_end
            .is_none_or(|best| end.log_probability > best.log_probability);
        if join.is_finite() && is_better {
            self.best_end = Some(end);
        }
    }

    /// Adds a healing step with `logits`, while healed text stays unwritten. Stops when the healed
    /// text gets less likely than `min`, the minimum for typed letters.
    pub(super) fn add_healing_step(&mut self, logits: &[f32], min: Option<LogProbability>) -> Step {
        if self.unwritten.is_empty() {
            return Step::Continue;
        }
        self.typed.add(logits, self.pieces, self.unwritten);
        if min.is_some_and(|min| self.typed.log_probability() < min) {
            Step::Stop
        } else {
            Step::Continue
        }
    }

    /// Returns the next token for `scores`, and the natural log of its probability in `logits`. The
    /// first step takes the first token of the healing, if it has one. Where the text must stay on
    /// its line ([`Generation::must_stay_on_line`]), a token that leaves it gives way to the most
    /// likely token among the [`SAME_LINE_CANDIDATES`] that stays. When the first token after a
    /// typed space wrote only whitespace and the next token leaves the line, it starts again with
    /// the most likely first token that writes a word, if the model finds that token more likely
    /// than the whitespace and the next token on the line together. `text_of` gives the text of a
    /// token, or `None` for the end of the text or markup.
    pub(super) fn choose_next_token(
        &mut self,
        scores: &[f32],
        logits: &[f32],
        limits: Limits<'_>,
        text_of: impl Fn(LlamaToken) -> Option<Vec<u8>>,
    ) -> Choice {
        if let Some(token) = self.healing.first_token.filter(|_| self.tokens.is_empty()) {
            return Choice::Add(token, self.log_probability_in(logits, token));
        }
        let chosen = choose_token(scores, logits, self.pieces, self.unwritten);
        if !self.must_stay_on_line(limits) {
            return Choice::Add(chosen.0, chosen.1);
        }
        let stays_on_line = |token| text_of(token).is_some_and(|piece| self.stays_on_line(&piece));
        let is_refused = !stays_on_line(chosen.0);
        let next = if is_refused {
            self.most_likely_where(scores, logits, stays_on_line)
        } else {
            Some(chosen)
        };
        if self.tokens.is_empty() {
            self.first_word = self.alternative_first_word(next, scores, logits, &text_of);
        } else if let Some((first_word, first_log_probability)) = self.first_word
            && is_refused
            && self.is_after_typed_space()
            && next.is_none_or(|(_, log_probability)| {
                first_log_probability > self.words.total() + log_probability
            })
        {
            return Choice::StartWith(first_word);
        }
        let (token, log_probability) = next.unwrap_or(chosen);
        Choice::Add(token, log_probability)
    }

    /// Returns the most likely token of `scores` among the [`SAME_LINE_CANDIDATES`] that `is_kept`
    /// keeps, and the natural log of its probability in `logits`.
    fn most_likely_where(
        &self,
        scores: &[f32],
        logits: &[f32],
        is_kept: impl Fn(LlamaToken) -> bool,
    ) -> Option<(LlamaToken, f32)> {
        most_likely_tokens(scores, SAME_LINE_CANDIDATES)
            .into_iter()
            .find(|&token| is_kept(token))
            .map(|token| (token, self.log_probability_in(logits, token)))
    }

    /// Returns the most likely first token that writes a word on the line of a typed space, and the
    /// natural log of its probability, when the `first` token writes only whitespace.
    fn alternative_first_word(
        &self,
        first: Option<(LlamaToken, f32)>,
        scores: &[f32],
        logits: &[f32],
        text_of: &impl Fn(LlamaToken) -> Option<Vec<u8>>,
    ) -> Option<(LlamaToken, f32)> {
        let writes_word = |piece: &[u8]| self.stays_on_line(piece) && self.writes_word(piece);
        let first_piece = first.and_then(|(token, _)| text_of(token))?;
        if writes_word(&first_piece) {
            return None;
        }
        self.most_likely_where(scores, logits, |token| {
            text_of(token).is_some_and(|piece| writes_word(&piece))
        })
    }

    /// Whether a token with `piece` as text writes a character that is not whitespace after the
    /// unwritten text.
    fn writes_word(&self, piece: &[u8]) -> bool {
        added_after(piece, self.unwritten)
            .iter()
            .any(|byte| !byte.is_ascii_whitespace())
    }

    /// Returns the natural log of the probability of `token` in `logits`. While healed text is
    /// unwritten, it is the share of `token` among the tokens that agree with that text.
    fn log_probability_in(&self, logits: &[f32], token: LlamaToken) -> f32 {
        if self.unwritten.is_empty() {
            log_probability_of(logits, token)
        } else {
            agreeing_log_probability(logits, token, self.pieces.agreeing_with(self.unwritten))
        }
    }

    /// Whether the next token must keep the text on its line: past the end of a sentence that the
    /// completion added, because the model often ends the message there, and after a typed space
    /// before the first added text, because the ghost text then shows nothing.
    fn must_stay_on_line(&self, limits: Limits<'_>) -> bool {
        self.continues_past_sentence_end(limits) || self.is_after_typed_space()
    }

    /// Whether the healed text ends with a typed space, and the added tokens wrote only whitespace
    /// after it.
    fn is_after_typed_space(&self) -> bool {
        self.healing.text.ends_with(b" ") && self.written.iter().all(u8::is_ascii_whitespace)
    }

    /// Whether a token with `piece` as text writes the start of the unwritten text, and then no
    /// line break. After a typed space, it must also open no placeholder, which the ghost text
    /// leaves out.
    fn stays_on_line(&self, piece: &[u8]) -> bool {
        let agrees =
            !piece.is_empty() && (piece.starts_with(self.unwritten) || self.unwritten.starts_with(piece));
        let added = added_after(piece, self.unwritten);
        let opens_hidden_placeholder = self.is_after_typed_space() && opens_placeholder(added);
        agrees && !added.contains(&b'\n') && !opens_hidden_placeholder
    }

    /// Whether the completion goes on past the end of a sentence that it added, so that the next
    /// token must not end the text or its line.
    fn continues_past_sentence_end(&self, limits: Limits<'_>) -> bool {
        limits.sentence_end == SentenceEnd::Continue
            && self.unwritten.is_empty()
            && ends_sentence(self.healing.text, &self.written)
    }

    /// Adds `token`, with `piece` as text and `log_probability`, unless it starts the line after
    /// the caret, a placeholder before that line, or the next sentence when the `limits` stop at
    /// the end of a sentence, or the minimum probability of the `limits` cuts it. The minimum
    /// applies to each sentence on its own. A cut inside a word leaves out that word. Stops after
    /// a line break, and at the token limit, where it leaves out the last word unless that word
    /// ends a sentence. It never leaves out the only word.
    ///
    /// Before a line, a stop of the cut or the token limit after a word that ends no phrase waits:
    /// the completion goes on for a share of the token limit until a new word after the end of a
    /// phrase, where it stops. `logits`, which come before `token`, tell whether the model finds
    /// the end of a phrase next. When it stops without the end of a phrase, also at a line break,
    /// [`Generation::into_added`] goes back to the last end of a phrase, else to the place of the
    /// stop.
    pub(super) fn add_token(
        &mut self,
        token: LlamaToken,
        piece: &[u8],
        log_probability: f32,
        logits: &[f32],
        limits: Limits<'_>,
    ) -> Step {
        let text = added_after(piece, self.unwritten);
        let starts_sentence = starts_next_sentence(self.healing.text, &self.written, text);
        if self.starts_line_after(text, limits.line_after)
            || (!limits.line_after.is_empty() && opens_placeholder(text))
            || (limits.sentence_end == SentenceEnd::Stop && starts_sentence)
        {
            return Step::Stop;
        }
        let ends_phrase_here = limits.line_after.is_empty() && self.note_word_end(text, logits);
        if self.pending_stop.is_some() && ends_phrase_here {
            return Step::Stop;
        }
        if starts_sentence {
            self.log_probability_before_sentence = self.words.total();
        }
        if self.is_cut(text, log_probability, limits) && self.stop_at_cut(text, limits) == Step::Stop {
            return Step::Stop;
        }
        let ends_line = writes_line_break(piece, self.unwritten);
        self.words.add(text, log_probability);
        self.written.extend_from_slice(text);
        self.unwritten = after_prefix(self.unwritten, piece);
        self.tokens.push(token);
        match self.pending_stop {
            _ if ends_line => Step::Stop,
            Some(stop) if self.tokens.len() < stop.max_tokens => Step::Continue,
            Some(_) => Step::Stop,
            None if self.tokens.len() == limits.max_added => self.stop_at_token_limit(limits),
            None => Step::Continue,
        }
    }

    /// Adds the last step with `logits`, where the model chose the end of the text or markup. When
    /// a stop waits for the end of a phrase, `logits` tell whether the added text ends a phrase.
    pub(super) fn add_end_step(&mut self, logits: &[f32]) {
        if self.pending_stop.is_some() {
            self.note_phrase_end(logits);
        }
    }

    /// Whether the minimum probability of the `limits` cuts a token that adds `text` with
    /// `log_probability`. No cut applies while a stop waits for the end of a phrase.
    fn is_cut(&self, text: &[u8], log_probability: f32, limits: Limits<'_>) -> bool {
        self.pending_stop.is_none()
            && limits.min_log_probability.is_some_and(|cut| {
                let sentence_cut = LogProbability(cut.0 + self.log_probability_before_sentence);
                !self.words.keeps(text, log_probability, sentence_cut)
            })
    }

    /// Stops before a token that adds `text`, which the cut of the tail cuts, as
    /// [`Generation::stop_at`] does. A cut inside a word stops before that word.
    fn stop_at_cut(&mut self, text: &[u8], limits: Limits<'_>) -> Step {
        let dropped = self.words.tokens_cut_before(text);
        let stop = TextEnd {
            tokens: self.tokens.len() - dropped,
            words: self.words.count() - usize::from(dropped > 0),
        };
        self.stop_at(stop, limits)
    }

    /// Returns no tokens, after a healing step stopped for a typo.
    pub(super) fn into_typo_stop(self) -> AddedTokens {
        AddedTokens {
            tokens: Vec::new(),
            word_log_probabilities: Vec::new(),
            typed_log_probability: self.typed.log_probability(),
            correction: None,
        }
    }

    /// Returns the tokens that show: the tokens up to the best end before the line after the
    /// caret, else all the tokens. When a stop waits for the end of a phrase and the added text
    /// ends no phrase, they end at the place where that stop goes back to.
    pub(super) fn into_added(mut self) -> AddedTokens {
        if let Some(stop) = self.pending_stop
            && !self.ends_at_phrase_end()
        {
            self.truncate_to(stop.fallback);
        }
        let (tokens, words) = self.shown_counts();
        let typed_log_probability = self.typed.log_probability();
        let mut added = self.tokens;
        added.truncate(tokens);
        let mut word_log_probabilities = self.words.into_vec();
        word_log_probabilities.truncate(words);
        AddedTokens {
            tokens: added,
            word_log_probabilities,
            typed_log_probability,
            correction: None,
        }
    }

    /// The tokens and the words that show.
    fn shown_counts(&self) -> (usize, usize) {
        let all = (self.tokens.len(), self.words.count());
        self.best_end
            .map_or(all, |end| (end.tokens.min(all.0), end.words.min(all.1)))
    }

    /// Whether the added text ends a phrase: before any whitespace at its end, it ends with a sign
    /// that ends a phrase or a sentence, or the model found the end of a phrase after it.
    fn ends_at_phrase_end(&self) -> bool {
        let written = self.written.trim_ascii_end();
        self.phrase_end == Some(self.end_here())
            || ends_phrase(written)
            || ends_sentence(self.healing.text, written)
    }

    /// Whether the healed text is written and the added tokens wrote text after it.
    fn has_written_text(&self) -> bool {
        self.unwritten.is_empty() && !self.written.is_empty()
    }

    /// Whether `text`, the text of a token after the healed text, starts to write `line_after`
    /// after the written text. Whitespace alone does not.
    fn starts_line_after(&self, text: &[u8], line_after: &[u8]) -> bool {
        let has_word = text.iter().any(|byte| !byte.is_ascii_whitespace());
        let agrees = !line_after.is_empty() && (line_after.starts_with(text) || text.starts_with(line_after));
        self.has_written_text() && has_word && agrees
    }
}

/// Removes the tokens of the last word from the end of `prompt_tokens`, and returns their text.
/// It stops at the token that starts the word, at markup, at a line break, and before the first
/// token, so the prompt never becomes empty. A line break ends a word, and the model writes too
/// many line breaks when it writes the last ones again.
pub(super) fn take_back_last_word(vocab: &LlamaVocab<'_>, prompt_tokens: &mut Vec<LlamaToken>) -> Vec<u8> {
    let mut text = Vec::new();
    while text.len() < MAX_HEALED_BYTES && prompt_tokens.len() > 1 {
        let Some(&token) = prompt_tokens
            .last()
            .filter(|&&token| !is_markup(vocab.attr(token)))
        else {
            break;
        };
        let piece = vocab.token_to_piece(token, false, None);
        if piece.contains(&b'\n') {
            break;
        }
        prompt_tokens.pop();
        text.splice(0..0, piece.iter().copied());
        if starts_word(&piece) {
            break;
        }
    }
    text
}

/// Returns `prompt_tokens` without their last word, and the text of that word, when the
/// `healed_text` that healing took back after them is only a typed space. A line break before that
/// space gives no word.
pub(super) fn word_before_typed_space(
    vocab: &LlamaVocab<'_>,
    prompt_tokens: &[LlamaToken],
    healed_text: &[u8],
) -> Option<(Vec<LlamaToken>, Vec<u8>)> {
    let is_typed_space = !healed_text.is_empty() && healed_text.iter().all(u8::is_ascii_whitespace);
    if !is_typed_space {
        return None;
    }
    let mut before_word = prompt_tokens.to_vec();
    let word = take_back_last_word(vocab, &mut before_word);
    (!word.is_empty()).then_some((before_word, word))
}

/// Returns the index of the token that starts the last word of `prompt_tokens`, as healing finds
/// that word.
pub(super) fn last_word_start(vocab: &LlamaVocab<'_>, prompt_tokens: &[LlamaToken]) -> usize {
    let mut before_last_word = prompt_tokens.to_vec();
    take_back_last_word(vocab, &mut before_last_word);
    before_last_word.len()
}

/// Returns `bytes` after `prefix`, or nothing when they do not start with it: what stays unwritten
/// after a token writes `prefix`, or the added text without the text that is already there.
fn after_prefix<'bytes>(bytes: &'bytes [u8], prefix: &[u8]) -> &'bytes [u8] {
    bytes.strip_prefix(prefix).unwrap_or_default()
}

/// Whether a token with `piece` as text adds a line break after it writes the `unwritten` text.
fn writes_line_break(piece: &[u8], unwritten: &[u8]) -> bool {
    added_after(piece, unwritten).contains(&b'\n')
}

/// Whether `text` opens a placeholder, such as "[Job Title]", which a model writes in place of a
/// fact that it does not know: an opening square bracket at its end or before a letter or a digit.
/// The ghost text ends before a placeholder, so an end after it never fits the line after the caret.
fn opens_placeholder(text: &[u8]) -> bool {
    text.iter().position(|&byte| byte == b'[').is_some_and(|start| {
        text.get(start + 1)
            .is_none_or(|&byte| byte.is_ascii_alphanumeric() || !byte.is_ascii())
    })
}

/// Returns the text that a token with `piece` as text adds after it writes the `unwritten` text.
fn added_after<'piece>(piece: &'piece [u8], unwritten: &[u8]) -> &'piece [u8] {
    piece.get(unwritten.len()..).unwrap_or_default()
}

/// Tells if a token with `attrs` is markup, such as `<think>`, that never belongs in typed text.
pub(super) fn is_markup(attrs: LlamaTokenAttrs) -> bool {
    attrs.intersects(LlamaTokenAttr::Control | LlamaTokenAttr::UserDefined)
}

/// Returns the text that the tokens of `added` write, without the `healed_text` that the prompt
/// already ends with. `piece` gives the text of each token. The text of each token stays as it is,
/// so a space before a sign, such as in "Merci !", stays.
pub(super) fn added_text(
    added: &[LlamaToken],
    healed_text: &[u8],
    piece: impl Fn(LlamaToken) -> Vec<u8>,
) -> String {
    let added_bytes = added.iter().flat_map(|&token| piece(token)).collect::<Vec<_>>();
    text_without_cut_character(after_prefix(&added_bytes, healed_text))
}

/// Returns `bytes` as text, without a character that the token limit cut at the end.
fn text_without_cut_character(bytes: &[u8]) -> String {
    let end = match str::from_utf8(bytes) {
        Err(error) if error.error_len().is_none() => error.valid_up_to(),
        _ => bytes.len(),
    };
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::test_support::assert_near;

    #[test]
    fn only_bytes_that_start_with_the_prefix_keep_the_rest_after_it() {
        let cases = [
            (&b" beauti"[..], &b" beaut"[..], &b"i"[..]),
            (b" beauti", b" beautiful", b""),
            (b" world, hi", b" wor", b"ld, hi"),
            (b" planet", b" wor", b""),
        ];

        let rests = cases.map(|(bytes, prefix, _)| after_prefix(bytes, prefix));

        assert_eq!(rests, cases.map(|(_, _, rest)| rest));
    }

    #[test]
    fn text_after_the_healed_text_is_what_a_token_adds() {
        let cases = [
            (&b" world"[..], &b" wor"[..], &b"ld"[..]),
            (b" wo", b" wor", b""),
            (b" world", b"", b" world"),
        ];

        let added = cases.map(|(piece, unwritten, _)| added_after(piece, unwritten));

        assert_eq!(added, cases.map(|(_, _, added)| added));
    }

    #[test]
    fn only_a_line_break_after_the_healed_text_ends_the_line() {
        let cases = [
            (&b"\n"[..], &b""[..], true),
            (b"ld\nHi", b"ld", true),
            (b" world", b"", false),
            (b"\nHi", b"\nHi", false),
            (b"\nHi", b"\nHi there", false),
        ];

        let ends = cases.map(|(piece, unwritten, _)| writes_line_break(piece, unwritten));

        assert_eq!(ends, cases.map(|(_, _, ends)| ends));
    }

    #[test]
    fn text_without_a_cut_character_is_kept() {
        let text = text_without_cut_character("café".as_bytes());

        assert_eq!(text, "café");
    }

    #[test]
    fn character_cut_at_the_end_is_dropped() {
        let bytes = "café".as_bytes();

        let text = text_without_cut_character(&bytes[..bytes.len() - 1]);

        assert_eq!(text, "caf");
    }

    #[test]
    fn invalid_bytes_inside_become_replacement_characters() {
        let text = text_without_cut_character(b"a\xffb");

        assert_eq!(text, "a\u{FFFD}b");
    }

    /// Returns the text of each token of `added`, which `texts` give in the order of their ids,
    /// after the `healed_text`.
    fn text_of_tokens(texts: &[&[u8]], healed_text: &[u8]) -> String {
        let added = (0..).take(texts.len()).map(LlamaToken).collect::<Vec<_>>();
        let piece = |token: LlamaToken| texts[usize::try_from(token.0).unwrap()].to_vec();

        added_text(&added, healed_text, piece)
    }

    #[test]
    fn added_text_keeps_the_space_before_punctuation() {
        let cases = [
            (&[&b" Merci"[..], b" !"][..], " Merci !"),
            (&[b" Pues", b" ..."], " Pues ..."),
            (&[b" one", b" ,", b" two", b" ?"], " one , two ?"),
            (&[b" it", b" ' ", b"s", b" 're"], " it ' s 're"),
        ];

        let texts = cases.map(|(texts, _)| text_of_tokens(texts, b""));

        assert_eq!(texts, cases.map(|(_, text)| text));
    }

    #[test]
    fn added_text_leaves_out_the_healed_text() {
        let text = text_of_tokens(&[b" Pu", b"es", b" !"], b" Pu");

        assert_eq!(text, "es !");
    }

    #[test]
    fn added_text_leaves_out_a_character_that_the_last_token_cut() {
        let text = text_of_tokens(&[b" caf", b"\xc3"], b"");

        assert_eq!(text, " caf");
    }

    #[test]
    fn added_text_joins_a_character_that_byte_tokens_write() {
        let text = text_of_tokens(&[b" caf", b"\xc3", b"\xa9", b" !"], b"");

        assert_eq!(text, " café !");
    }

    #[test]
    fn only_control_and_user_defined_tokens_are_markup() {
        let cases = [
            (LlamaTokenAttr::Normal, false),
            (LlamaTokenAttr::Byte, false),
            (LlamaTokenAttr::Control, true),
            (LlamaTokenAttr::UserDefined, true),
        ];

        let markup = cases.map(|(attr, _)| is_markup(LlamaTokenAttrs(attr.into())));

        assert_eq!(markup, cases.map(|(_, expected)| expected));
    }

    fn healing(text: &[u8]) -> Healing<'_> {
        Healing {
            text,
            first_token: None,
        }
    }

    fn limits(max_added: usize, min_log_probability: Option<f32>) -> Limits<'static> {
        Limits {
            max_added,
            min_log_probability: min_log_probability.map(LogProbability),
            typo_check: TypoCheck::Off,
            sentence_end: SentenceEnd::Continue,
            line_after: b"",
        }
    }

    /// The text of the tokens that the logits of [`PHRASE_END_NEXT`] and [`WORD_NEXT`] give: a word
    /// and a sign that ends a phrase.
    const WORD_AND_PERIOD: [&str; 2] = [" x", "."];
    /// Logits where the model finds the end of a phrase next half of the time.
    const PHRASE_END_NEXT: [f32; 2] = [0.0, 0.0];
    /// Logits where the model almost never finds the end of a phrase next.
    const WORD_NEXT: [f32; 2] = [0.0, -20.0];

    /// Adds a token for each of `steps`, with the ids from 1, after an empty healed text, until a
    /// step stops. Each step has the text and the log probability of its token, and the logits
    /// before it. Returns the generation and its last step.
    fn generation_of<'pieces>(
        pieces: &'pieces TokenPieces,
        steps: &[(&str, f32, [f32; 2])],
        limits: Limits<'_>,
    ) -> (Generation<'pieces>, Step) {
        let mut generation = Generation::new(pieces, healing(b""));
        let step = steps
            .iter()
            .zip(1..)
            .map(|(&(text, log_probability, logits), id)| {
                generation.add_token(LlamaToken(id), text.as_bytes(), log_probability, &logits, limits)
            })
            .find(|&step| step == Step::Stop)
            .unwrap_or(Step::Continue);
        (generation, step)
    }

    /// As [`generation_of`], and returns the last step and the tokens that stay.
    fn add_steps(steps: &[(&str, f32, [f32; 2])], limits: Limits<'_>) -> (Step, Vec<LlamaToken>) {
        let pieces = TokenPieces::from_texts(&WORD_AND_PERIOD);
        let (generation, step) = generation_of(&pieces, steps, limits);
        (step, generation.into_added().tokens)
    }

    /// As [`add_steps`], where the model finds the end of a phrase after each word.
    fn add_tokens(tokens: &[(&str, f32)], limits: Limits<'_>) -> (Step, Vec<LlamaToken>) {
        let steps = tokens
            .iter()
            .map(|&(text, log_probability)| (text, log_probability, PHRASE_END_NEXT))
            .collect::<Vec<_>>();
        add_steps(&steps, limits)
    }

    #[test]
    fn only_unwritten_healed_text_below_the_typo_minimum_stops() {
        let pieces = TokenPieces::from_texts(&[" cost", " cat"]);
        let logits = [1.0_f32.ln(), 99.0_f32.ln()];
        let cases = [
            (&b" cost"[..], Some(LogProbability(-3.0)), Step::Stop),
            (b" cost", Some(LogProbability(-5.0)), Step::Continue),
            (b" cost", None, Step::Continue),
            (b"", Some(LogProbability(0.0)), Step::Continue),
        ];

        let steps = cases
            .map(|(text, min, _)| Generation::new(&pieces, healing(text)).add_healing_step(&logits, min));

        assert_eq!(steps, cases.map(|(_, _, step)| step));
    }

    #[test]
    fn typo_stop_drops_the_tokens_that_wrote_the_healed_text() {
        let pieces = TokenPieces::from_texts(&[" beaut", "i"]);
        let mut generation = Generation::new(&pieces, healing(b" beauti"));
        generation.add_token(LlamaToken(0), b" beaut", -1.0, &[], limits(16, None));

        let added = generation.into_typo_stop();

        assert_eq!(added.tokens, []);
    }

    #[test]
    fn first_step_takes_the_first_token_of_the_healing() {
        let pieces = TokenPieces::from_texts(&[" de", " deli"]);
        let mut generation = Generation::new(
            &pieces,
            Healing {
                text: b" deli",
                first_token: Some(LlamaToken(1)),
            },
        );

        let choice = generation.choose_next_token(&[5.0, 1.0], &[5.0, 1.0], limits(16, None), |_| None);

        assert!(matches!(choice, Choice::Add(LlamaToken(1), _)));
    }

    /// The texts of the tokens of the tests of the token after a typed space, in the order of their
    /// ids. The last token ends the text.
    const AFTER_SPACE: [&str; 7] = [" [", " ", "\n\n", " dinner", "Si", " soon.", "<eos>"];
    const PLACEHOLDER: LlamaToken = LlamaToken(0);
    const SPACE: LlamaToken = LlamaToken(1);
    const LINE_BREAK: LlamaToken = LlamaToken(2);
    const DINNER: LlamaToken = LlamaToken(3);
    const SI: LlamaToken = LlamaToken(4);
    const SOON: LlamaToken = LlamaToken(5);
    const END_OF_TEXT: LlamaToken = LlamaToken(6);

    /// Returns the token that `choice` adds, if any.
    fn token_of(choice: Choice) -> Option<LlamaToken> {
        match choice {
            Choice::Add(token, _) => Some(token),
            Choice::StartWith(_) => None,
        }
    }

    /// Returns the text of each token of [`AFTER_SPACE`], or `None` for [`END_OF_TEXT`].
    fn text_after_space(token: LlamaToken) -> Option<Vec<u8>> {
        let text = AFTER_SPACE[usize::try_from(token.0).unwrap()];
        (token != END_OF_TEXT).then(|| text.as_bytes().to_vec())
    }

    /// Chooses the token after the `healed` text and the `added` tokens of [`AFTER_SPACE`], for
    /// `scores` that are also the logits.
    fn choose_after(healed: &[u8], added: &[LlamaToken], scores: &[f32; 7], limits: Limits<'_>) -> Choice {
        let pieces = TokenPieces::from_texts(&AFTER_SPACE);
        let mut generation = Generation::new(&pieces, healing(healed));
        for &token in added {
            let text = text_after_space(token).unwrap();
            generation.add_token(token, &text, -0.5, &[], limits);
        }
        generation.choose_next_token(scores, scores, limits, text_after_space)
    }

    #[test]
    fn placeholder_after_a_typed_space_gives_way_to_the_next_token_that_writes_the_space() {
        let scores = [5.0, 3.0, 0.0, 4.0, 6.0, 0.0, 0.0];

        let choice = choose_after(b" ", &[], &scores, limits(16, None));

        let Choice::Add(token, log_probability) = choice else {
            panic!("{choice:?} adds no token");
        };
        assert_eq!(token, DINNER);
        assert_near(
            log_probability,
            agreeing_log_probability(&scores, DINNER, [PLACEHOLDER, SPACE, DINNER, SOON].into_iter()),
        );
    }

    #[test]
    fn line_end_after_a_typed_space_gives_way_to_the_next_token_on_the_line() {
        let cases = [
            [0.0, 0.0, 6.0, 1.0, 5.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0, 5.0, 0.0, 6.0],
        ];

        let chosen = cases.map(|scores| choose_after(b" ", &[SPACE], &scores, limits(16, None)));

        assert_eq!(
            chosen,
            cases.map(|scores| Choice::Add(SI, log_probability_of(&scores, SI)))
        );
    }

    /// Chooses the first token after a typed space for `first_scores`, adds it, and chooses the
    /// next token for `next_scores`. The scores are also the logits.
    fn choose_after_first(first_scores: &[f32; 7], next_scores: &[f32; 7]) -> Choice {
        let pieces = TokenPieces::from_texts(&AFTER_SPACE);
        let mut generation = Generation::new(&pieces, healing(b" "));
        let first_choice =
            generation.choose_next_token(first_scores, first_scores, limits(16, None), text_after_space);
        let Choice::Add(first, log_probability) = first_choice else {
            panic!("{first_choice:?} adds no first token");
        };
        let text = text_after_space(first).unwrap();
        generation.add_token(first, &text, log_probability, first_scores, limits(16, None));
        generation.choose_next_token(next_scores, next_scores, limits(16, None), text_after_space)
    }

    /// Scores where a lone space is the most likely token that writes a typed space, and " dinner"
    /// the most likely one that also writes a word.
    const SPACE_BEFORE_DINNER: [f32; 7] = [0.0, 3.0, 5.0, 2.0, 0.0, 0.0, 0.0];

    #[test]
    fn line_end_after_whitespace_for_a_typed_space_starts_again_with_a_more_likely_first_word() {
        let next_scores = [0.0, 0.0, 6.0, 0.0, 1.0, 0.0, 0.0];

        let choice = choose_after_first(&SPACE_BEFORE_DINNER, &next_scores);

        assert_eq!(choice, Choice::StartWith(DINNER));
    }

    #[test]
    fn line_end_after_whitespace_for_a_typed_space_gives_way_to_a_more_likely_next_token() {
        let next_scores = [0.0, 0.0, 3.0, 0.0, 2.9, 0.0, 0.0];

        let choice = choose_after_first(&SPACE_BEFORE_DINNER, &next_scores);

        assert_eq!(choice, Choice::Add(SI, log_probability_of(&next_scores, SI)));
    }

    #[test]
    fn word_after_whitespace_for_a_typed_space_stays() {
        let next_scores = [0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0];

        let choice = choose_after_first(&SPACE_BEFORE_DINNER, &next_scores);

        assert_eq!(choice, Choice::Add(SI, log_probability_of(&next_scores, SI)));
    }

    #[test]
    fn line_break_after_text_on_the_line_stays() {
        let scores = [0.0, 0.0, 6.0, 1.0, 5.0, 0.0, 0.0];
        let cases: [(&[u8], &[LlamaToken]); 2] = [(b" dinner", &[DINNER]), (b" ", &[DINNER])];

        let choices = cases.map(|(healed, added)| choose_after(healed, added, &scores, limits(16, None)));

        assert_eq!(choices.map(token_of), [Some(LINE_BREAK), Some(LINE_BREAK)]);
    }

    #[test]
    fn line_break_after_an_added_sentence_end_gives_way_only_without_the_sentence_stop() {
        let scores = [0.0, 0.0, 6.0, 4.0, 0.0, 0.0, 0.0];
        let sentence_ends = [SentenceEnd::Continue, SentenceEnd::Stop];

        let tokens = sentence_ends.map(|sentence_end| {
            let limits = Limits {
                sentence_end,
                ..limits(16, None)
            };
            token_of(choose_after(b"", &[SOON], &scores, limits))
        });

        assert_eq!(tokens, [Some(DINNER), Some(LINE_BREAK)]);
    }

    #[test]
    fn tokens_inside_the_limits_continue() {
        let added = add_tokens(&[("very", -1.0), (" soon", -1.0)], limits(16, None));

        assert_eq!(added, (Step::Continue, vec![LlamaToken(1), LlamaToken(2)]));
    }

    #[test]
    fn token_with_a_line_break_is_the_last_token() {
        let added = add_tokens(
            &[("very", -1.0), (" soon\n", -1.0), (" thanks", -1.0)],
            limits(16, None),
        );

        assert_eq!(added, (Step::Stop, vec![LlamaToken(1), LlamaToken(2)]));
    }

    #[test]
    fn sentence_stop_leaves_out_a_new_word_after_the_end_of_a_sentence() {
        let limits = Limits {
            sentence_end: SentenceEnd::Stop,
            ..limits(16, None)
        };

        let added = add_tokens(&[("Sure.", -1.0), (" Then", -1.0)], limits);

        assert_eq!(added, (Step::Stop, vec![LlamaToken(1)]));
    }

    #[test]
    fn new_word_after_the_end_of_a_sentence_continues_without_the_sentence_stop() {
        let added = add_tokens(&[("Sure.", -1.0), (" Then", -1.0)], limits(16, None));

        assert_eq!(added, (Step::Continue, vec![LlamaToken(1), LlamaToken(2)]));
    }

    #[test]
    fn next_sentence_gets_its_own_tail_cut_without_the_sentence_stop() {
        let added = add_tokens(
            &[("Sure.", -1.0), (" Then", -2.5), (" we", -0.5)],
            limits(16, Some(-3.0)),
        );

        assert_eq!(
            added,
            (Step::Continue, vec![LlamaToken(1), LlamaToken(2), LlamaToken(3)])
        );
    }

    #[test]
    fn tail_cut_still_ends_the_next_sentence_without_the_sentence_stop() {
        let added = add_tokens(
            &[("Sure.", -1.0), (" Then", -2.5), (" we", -1.0)],
            limits(16, Some(-3.0)),
        );

        assert_eq!(added, (Step::Stop, vec![LlamaToken(1), LlamaToken(2)]));
    }

    #[test]
    fn only_an_added_sentence_end_without_the_sentence_stop_continues_past_it() {
        let cases = [
            ("Sure.", SentenceEnd::Continue, true),
            ("Sure.", SentenceEnd::Stop, false),
            ("Sure", SentenceEnd::Continue, false),
            (".", SentenceEnd::Continue, false),
        ];

        let continues = cases.map(|(text, sentence_end, _)| {
            let pieces = TokenPieces::from_texts(&[]);
            let limits = Limits {
                sentence_end,
                ..limits(16, None)
            };
            let mut generation = Generation::new(&pieces, healing(b""));
            generation.add_token(LlamaToken(1), text.as_bytes(), -1.0, &[], limits);
            generation.continues_past_sentence_end(limits)
        });

        assert_eq!(continues, cases.map(|(_, _, expected)| expected));
    }

    #[test]
    fn cut_inside_a_word_leaves_out_that_word() {
        let added = add_tokens(
            &[("very", -1.0), (" th", -0.5), ("anks", -5.0)],
            limits(16, Some(-3.0)),
        );

        assert_eq!(added, (Step::Stop, vec![LlamaToken(1)]));
    }

    #[test]
    fn token_limit_leaves_out_a_last_word_that_does_not_end_a_sentence() {
        let last_words = [" so", " soon."];

        let added =
            last_words.map(|last_word| add_tokens(&[("very", -1.0), (last_word, -1.0)], limits(2, None)));

        assert_eq!(
            added,
            [
                (Step::Stop, vec![LlamaToken(1)]),
                (Step::Stop, vec![LlamaToken(1), LlamaToken(2)]),
            ]
        );
    }

    #[test]
    fn tail_cut_after_a_word_that_ends_no_phrase_goes_on_to_the_end_of_a_phrase() {
        let steps = [
            ("Thanks", -0.5, WORD_NEXT),
            (" for", -0.5, WORD_NEXT),
            (" the", -3.0, WORD_NEXT),
            (" help", -2.0, WORD_NEXT),
            ("!", -0.5, WORD_NEXT),
            (" See", -1.0, WORD_NEXT),
        ];

        let added = add_steps(&steps, limits(16, Some(-3.0)));

        assert_eq!(added, (Step::Stop, [1, 2, 3, 4, 5].map(LlamaToken).to_vec()));
    }

    #[test]
    fn token_limit_after_a_word_that_ends_no_phrase_goes_on_to_the_end_of_a_phrase() {
        let steps = [
            ("We", -0.1, WORD_NEXT),
            (" will", -0.1, WORD_NEXT),
            (" send", -0.1, WORD_NEXT),
            (" the", -0.1, WORD_NEXT),
            (" file", -0.1, WORD_NEXT),
            (".", -0.1, WORD_NEXT),
            (" Thanks", -0.1, WORD_NEXT),
        ];

        let added = add_steps(&steps, limits(5, None));

        assert_eq!(added, (Step::Stop, [1, 2, 3, 4, 5, 6].map(LlamaToken).to_vec()));
    }

    /// Steps where the tail cut stops after "Sure thing, I", which ends no phrase, and the end of a
    /// phrase after "Sure thing," comes before it.
    const CUT_AFTER_A_PHRASE_END: [(&str, f32, [f32; 2]); 5] = [
        ("Sure", -0.5, WORD_NEXT),
        (" thing", -0.5, WORD_NEXT),
        (",", -0.1, WORD_NEXT),
        (" I", -0.5, WORD_NEXT),
        (" will", -3.0, WORD_NEXT),
    ];

    #[test]
    fn stop_without_the_end_of_a_phrase_in_reach_goes_back_to_the_last_end_of_a_phrase() {
        let words_after = [
            (" do", -0.1, WORD_NEXT),
            (" it", -0.1, WORD_NEXT),
            (" for", -0.1, WORD_NEXT),
        ];

        let added = add_steps(
            &[&CUT_AFTER_A_PHRASE_END[..], &words_after].concat(),
            limits(10, Some(-3.0)),
        );

        assert_eq!(added, (Step::Stop, [1, 2, 3].map(LlamaToken).to_vec()));
    }

    #[test]
    fn stop_that_waits_for_the_end_of_a_phrase_shows_the_text_up_to_the_word_after_where_it_goes_back() {
        let pieces = TokenPieces::from_texts(&WORD_AND_PERIOD);
        let steps = [&CUT_AFTER_A_PHRASE_END[..], &[(" do", -0.1, WORD_NEXT)]].concat();

        let (generation, step) = generation_of(&pieces, &steps, limits(10, Some(-3.0)));

        assert_eq!(
            (step, generation.shown_tokens()),
            (Step::Continue, &[1, 2, 3, 4].map(LlamaToken)[..])
        );
    }

    #[test]
    fn shown_text_ends_at_the_word_after_the_last_end_of_a_phrase() {
        let pieces = TokenPieces::from_texts(&WORD_AND_PERIOD);
        let steps = [
            ("Sure", -0.1, WORD_NEXT),
            (",", -0.1, WORD_NEXT),
            (" I", -0.1, WORD_NEXT),
            (" will", -0.1, WORD_NEXT),
        ];

        let (generation, _) = generation_of(&pieces, &steps, limits(10, None));

        assert_eq!(generation.shown_tokens(), [1, 2, 3].map(LlamaToken));
    }

    #[test]
    fn stop_that_waits_for_the_end_of_a_phrase_has_room_past_the_token_limit() {
        let pieces = TokenPieces::from_texts(&WORD_AND_PERIOD);
        let limits = limits(5, None);
        let steps = [
            ("We", -0.1, WORD_NEXT),
            (" will", -0.1, WORD_NEXT),
            (" send", -0.1, WORD_NEXT),
            (" the", -0.1, WORD_NEXT),
            (" file", -0.1, WORD_NEXT),
        ];

        let (generation, step) = generation_of(&pieces, &steps, limits);

        assert_eq!((step, generation.has_room(limits)), (Step::Continue, true));
    }

    #[test]
    fn last_token_that_a_stop_waits_for_keeps_the_sign_that_it_writes() {
        let steps = [
            ("We", -0.1, WORD_NEXT),
            (" will", -0.1, WORD_NEXT),
            (" send", -0.1, WORD_NEXT),
            (" the", -0.1, WORD_NEXT),
            (" file", -0.1, WORD_NEXT),
            (" now", -0.1, WORD_NEXT),
            (".", -0.1, WORD_NEXT),
        ];

        let added = add_steps(&steps, limits(5, None));

        assert_eq!(
            added,
            (Step::Stop, [1, 2, 3, 4, 5, 6, 7].map(LlamaToken).to_vec())
        );
    }

    /// Steps where the tail cut stops after "We will send", which ends no phrase, and the
    /// completion goes on with " it" to reach the end of a phrase.
    const CUT_AFTER_SEND: [(&str, f32, [f32; 2]); 4] = [
        ("We", -0.1, WORD_NEXT),
        (" will", -0.1, WORD_NEXT),
        (" send", -0.1, WORD_NEXT),
        (" it", -4.0, WORD_NEXT),
    ];

    /// Logits where a line break (token 2) is the most likely token, but the model finds the end of
    /// a phrase next only about 9 percent of the time.
    const UNSURE_LINE_BREAK: [f32; 13] = [0.0, -20.0, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];

    #[test]
    fn markup_while_a_stop_waits_for_the_end_of_a_phrase_goes_back_to_the_place_of_the_stop() {
        let pieces = TokenPieces::from_texts(&WORD_AND_PERIOD);
        let (mut generation, _) = generation_of(&pieces, &CUT_AFTER_SEND, limits(16, Some(-3.0)));

        generation.add_end_step(&WORD_NEXT);

        assert_eq!(generation.into_added().tokens, [1, 2, 3].map(LlamaToken));
    }

    #[test]
    fn unsure_line_break_while_a_stop_waits_for_the_end_of_a_phrase_goes_back_to_the_place_of_the_stop() {
        let pieces = TokenPieces::from_texts(&[" x", ".", "\n"]);
        let limits = limits(16, Some(-3.0));
        let (mut generation, _) = generation_of(&pieces, &CUT_AFTER_SEND, limits);

        let step = generation.add_token(LlamaToken(5), b"\n", -2.4, &UNSURE_LINE_BREAK, limits);

        assert_eq!(
            (step, generation.into_added().tokens),
            (Step::Stop, [1, 2, 3].map(LlamaToken).to_vec())
        );
    }

    #[test]
    fn likely_line_break_while_a_stop_waits_for_the_end_of_a_phrase_keeps_the_last_word() {
        let pieces = TokenPieces::from_texts(&[" x", ".", "\n"]);
        let limits = limits(16, Some(-3.0));
        let (mut generation, _) = generation_of(&pieces, &CUT_AFTER_SEND, limits);

        let step = generation.add_token(LlamaToken(5), b"\n", -0.1, &[0.0, -20.0, 2.0], limits);

        assert_eq!(
            (step, generation.into_added().tokens),
            (Step::Stop, [1, 2, 3, 4].map(LlamaToken).to_vec())
        );
    }

    #[test]
    fn likely_end_of_text_while_a_stop_waits_for_the_end_of_a_phrase_keeps_the_last_word() {
        let pieces = TokenPieces::from_texts(&WORD_AND_PERIOD);
        let (mut generation, _) = generation_of(&pieces, &CUT_AFTER_SEND, limits(16, Some(-3.0)));

        generation.add_end_step(&PHRASE_END_NEXT);

        assert_eq!(generation.into_added().tokens, [1, 2, 3, 4].map(LlamaToken));
    }

    #[test]
    fn only_a_completion_before_a_line_can_add_tokens_to_reach_the_end_of_a_phrase() {
        let inside_a_line = Limits {
            line_after: b" on Monday.",
            ..limits(10, None)
        };

        let most_added = [limits(10, None).most_added(), inside_a_line.most_added()];

        assert_eq!(most_added, [14, 10]);
    }

    #[test]
    fn tail_cut_inside_a_line_stops_without_the_end_of_a_phrase() {
        let limits = Limits {
            line_after: b" on Monday.",
            ..limits(10, Some(-3.0))
        };

        let added = add_steps(&CUT_AFTER_A_PHRASE_END, limits);

        assert_eq!(added, (Step::Stop, [1, 2, 3, 4].map(LlamaToken).to_vec()));
    }

    /// Log probabilities of the tokens " on" and " for", in that order.
    const LIKELY_ON: [f32; 2] = [-0.105, -2.303];
    const UNLIKELY_ON: [f32; 2] = [-4.605, -0.01];
    const LINE_AFTER: &[u8] = b" on Monday.";

    /// Adds a join step with the logits of each of `steps` and then its token, with the ids from 1,
    /// after an empty healed text and before `line_after`, until a step stops. Returns the last
    /// step and the tokens that show.
    fn add_tokens_before(line_after: &[u8], steps: &[([f32; 2], &str, f32)]) -> (Step, Vec<LlamaToken>) {
        let pieces = TokenPieces::from_texts(&[" on", " for"]);
        let limits = Limits {
            line_after,
            ..limits(16, None)
        };
        let mut generation = Generation::new(&pieces, healing(b""));
        let step = steps
            .iter()
            .zip(1..)
            .map(|(&(logits, text, log_probability), id)| {
                generation.add_join_step(&logits, line_after);
                generation.add_token(LlamaToken(id), text.as_bytes(), log_probability, &[], limits)
            })
            .find(|&step| step == Step::Stop)
            .unwrap_or(Step::Continue);
        (step, generation.into_added().tokens)
    }

    #[test]
    fn added_text_ends_where_it_and_the_line_after_it_are_most_likely() {
        let steps = [
            (UNLIKELY_ON, "opportunity", -0.7),
            (UNLIKELY_ON, " to", -0.1),
            (UNLIKELY_ON, " interview", -0.1),
            (LIKELY_ON, " for", -0.5),
            (UNLIKELY_ON, " the", -0.2),
        ];

        let added = add_tokens_before(LINE_AFTER, &steps);

        assert_eq!(added, (Step::Continue, [1, 2, 3].map(LlamaToken).to_vec()));
    }

    #[test]
    fn token_that_starts_the_line_after_stops_the_added_text() {
        let steps = [(UNLIKELY_ON, "invoice", -0.1), (LIKELY_ON, " on", -0.1)];

        let added = add_tokens_before(LINE_AFTER, &steps);

        assert_eq!(added, (Step::Stop, vec![LlamaToken(1)]));
    }

    #[test]
    fn whitespace_alone_does_not_start_the_line_after() {
        let steps = [
            (UNLIKELY_ON, "May", -0.1),
            (UNLIKELY_ON, " ", -0.1),
            (UNLIKELY_ON, "14", -0.1),
        ];

        let (step, _) = add_tokens_before(LINE_AFTER, &steps);

        assert_eq!(step, Step::Continue);
    }

    #[test]
    fn placeholder_stops_the_added_text_before_the_line_after() {
        let tokens = [" [", " [Job", " [é"];

        let steps = tokens.map(|token| {
            add_tokens_before(
                LINE_AFTER,
                &[(UNLIKELY_ON, "opportunity", -0.1), (UNLIKELY_ON, token, -0.1)],
            )
        });

        assert_eq!(steps, tokens.map(|_| (Step::Stop, vec![LlamaToken(1)])));
    }

    #[test]
    fn bracket_that_opens_no_placeholder_or_ends_a_line_keeps_the_added_text() {
        let cases = [(LINE_AFTER, " [-"), (&b""[..], " [Job")];

        let steps = cases.map(|(line_after, token)| {
            add_tokens_before(
                line_after,
                &[(UNLIKELY_ON, "opportunity", -0.1), (UNLIKELY_ON, token, -0.1)],
            )
            .0
        });

        assert_eq!(steps, cases.map(|_| Step::Continue));
    }

    #[test]
    fn line_after_that_no_token_starts_keeps_all_added_tokens() {
        let steps = [(LIKELY_ON, "very", -0.1), (LIKELY_ON, " soon", -3.0)];

        let added = add_tokens_before(b" 14 at noon", &steps);

        assert_eq!(added, (Step::Continue, [1, 2].map(LlamaToken).to_vec()));
    }

    #[test]
    fn added_text_without_a_line_after_keeps_all_its_tokens() {
        let steps = [(LIKELY_ON, "very", -0.1), (UNLIKELY_ON, " soon", -3.0)];

        let added = add_tokens_before(b"", &steps);

        assert_eq!(added, (Step::Continue, [1, 2].map(LlamaToken).to_vec()));
    }
}
