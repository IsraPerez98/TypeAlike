//! What a completion takes and gives. It has the decoder, the limits, the progress and the healed
//! text. It also has the state of each step and the added tokens with their text.

use std::str;

use ghosttype_core::inference::{Correction, LogProbability, TypoCheck};
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::token_type::{LlamaTokenAttr, LlamaTokenAttrs};
use llama_cpp_2::vocab::LlamaVocab;

use super::correction::TypedLetters;
use super::logits::{TokenBias, agreeing_log_probability, choose_token, typed_log_probability};
use super::pieces::TokenPieces;
use super::words::{WordLogProbabilities, ends_sentence, starts_next_sentence, starts_word};

/// The most text at the end of a prompt that generation takes back and writes again.
const MAX_HEALED_BYTES: usize = 24;
/// The most text at the start of the line after the caret that a join step compares with the text
/// of the tokens: more than a token holds.
const MAX_JOINED_BYTES: usize = 64;

/// Where a completion stops adding tokens.
#[derive(Debug, Clone, Copy)]
pub(super) struct Limits<'text> {
    pub(super) max_added: usize,
    /// The least likely that the added text can get after its first word.
    pub(super) min_log_probability: Option<LogProbability>,
    /// What the completion does when the healed text looks like a typo.
    pub(super) typo_check: TypoCheck,
    /// The text after the caret on its line, which the added text leads into, or empty.
    pub(super) line_after: &'text [u8],
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
    typed: TypedLetters,
    /// The end of the added text where the model finds the added text and then the line after the
    /// caret most likely, if any.
    best_end: Option<JoinEnd>,
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
            typed: TypedLetters::default(),
            best_end: None,
        }
    }

    /// The tokens that the completion added so far.
    pub(super) fn tokens(&self) -> &[LlamaToken] {
        &self.tokens
    }

    /// The tokens that show: the tokens up to the best end before the line after the caret, else
    /// all the tokens.
    pub(super) fn shown_tokens(&self) -> &[LlamaToken] {
        &self.tokens[..self.shown_counts().0]
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
    /// first step takes the first token of the healing, if it has one.
    pub(super) fn choose_next_token(&self, scores: &[f32], logits: &[f32]) -> (LlamaToken, f32) {
        match self.healing.first_token.filter(|_| self.tokens.is_empty()) {
            Some(token) => (
                token,
                agreeing_log_probability(logits, token, self.pieces.agreeing_with(self.unwritten)),
            ),
            None => choose_token(scores, logits, self.pieces, self.unwritten),
        }
    }

    /// Adds `token`, with `piece` as text and `log_probability`, unless it starts the line after
    /// the caret, a placeholder before that line, or the next sentence, or the minimum probability
    /// of the `limits` cuts it. A cut
    /// inside a word leaves out that word. Stops after a line break, and at the token limit, where
    /// it leaves out the last word unless that word ends a sentence. It never leaves out the only
    /// word.
    pub(super) fn add_token(
        &mut self,
        token: LlamaToken,
        piece: &[u8],
        log_probability: f32,
        limits: Limits<'_>,
    ) -> Step {
        let text = added_after(piece, self.unwritten);
        if self.starts_line_after(text, limits.line_after)
            || (!limits.line_after.is_empty() && opens_placeholder(text))
            || starts_next_sentence(self.healing.text, &self.written, text)
        {
            return Step::Stop;
        }
        let is_cut = limits
            .min_log_probability
            .is_some_and(|cut| !self.words.keeps(text, log_probability, cut));
        if is_cut {
            let dropped = self.words.end_before(text);
            self.tokens.truncate(self.tokens.len().saturating_sub(dropped));
            return Step::Stop;
        }
        let ends_line = writes_line_break(piece, self.unwritten);
        self.words.add(text, log_probability);
        self.written.extend_from_slice(text);
        self.unwritten = after_prefix(self.unwritten, piece);
        self.tokens.push(token);
        if ends_line {
            return Step::Stop;
        }
        if self.tokens.len() == limits.max_added {
            if !ends_sentence(self.healing.text, &self.written) {
                let dropped = self.words.drop_last_word();
                self.tokens.truncate(self.tokens.len().saturating_sub(dropped));
            }
            return Step::Stop;
        }
        Step::Continue
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

    /// Returns the tokens that show, as [`Generation::shown_tokens`] gives them.
    pub(super) fn into_added(self) -> AddedTokens {
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

/// Returns the text `added` puts after `prompt_tokens`, with the spacing it has after them, without
/// the `healed_text` that the prompt already ends with.
pub(super) fn added_text(
    vocab: &LlamaVocab<'_>,
    prompt_tokens: &[LlamaToken],
    added: &[LlamaToken],
    healed_text: &[u8],
) -> String {
    let prompt_bytes = vocab.detokenize(prompt_tokens, false, false);
    let all_bytes = vocab.detokenize(&[prompt_tokens, added].concat(), false, false);
    let added_bytes = all_bytes
        .strip_prefix(prompt_bytes.as_slice())
        .map_or_else(|| vocab.detokenize(added, false, false), <[u8]>::to_vec);
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
            line_after: b"",
        }
    }

    /// Adds a token for each of `tokens`, with the ids from 1, after an empty healed text, until a
    /// step stops. Returns the last step and the tokens that stay.
    fn add_tokens(tokens: &[(&str, f32)], limits: Limits<'_>) -> (Step, Vec<LlamaToken>) {
        let pieces = TokenPieces::from_texts(&[]);
        let mut generation = Generation::new(&pieces, healing(b""));
        let step = tokens
            .iter()
            .zip(1..)
            .map(|(&(text, log_probability), id)| {
                generation.add_token(LlamaToken(id), text.as_bytes(), log_probability, limits)
            })
            .find(|&step| step == Step::Stop)
            .unwrap_or(Step::Continue);
        (step, generation.tokens().to_vec())
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
        generation.add_token(LlamaToken(0), b" beaut", -1.0, limits(16, None));

        let added = generation.into_typo_stop();

        assert_eq!(added.tokens, []);
    }

    #[test]
    fn first_step_takes_the_first_token_of_the_healing() {
        let pieces = TokenPieces::from_texts(&[" de", " deli"]);
        let generation = Generation::new(
            &pieces,
            Healing {
                text: b" deli",
                first_token: Some(LlamaToken(1)),
            },
        );

        let (token, _) = generation.choose_next_token(&[5.0, 1.0], &[5.0, 1.0]);

        assert_eq!(token, LlamaToken(1));
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
    fn new_word_after_the_end_of_a_sentence_is_left_out() {
        let added = add_tokens(&[("Sure.", -1.0), (" Then", -1.0)], limits(16, None));

        assert_eq!(added, (Step::Stop, vec![LlamaToken(1)]));
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
                generation.add_token(LlamaToken(id), text.as_bytes(), log_probability, limits)
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
