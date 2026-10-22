//! The log probability of each added word, and the end of a sentence.

use std::mem;

use ghosttype_core::inference::LogProbability;

/// The signs other than a period that end a sentence when a new word comes after them.
const SENTENCE_ENDS: [char; 3] = ['!', '?', '\u{2026}'];
/// The fewest letters of the word before a period that ends a sentence. A shorter word is often
/// an abbreviation, such as "Dr." or "e.g.".
const MIN_LETTERS_BEFORE_PERIOD: usize = 3;

/// Sums the log probabilities of the added tokens for each word that they write.
#[derive(Debug, Default)]
pub(super) struct WordLogProbabilities {
    words: Vec<LogProbability>,
    /// The tokens of the last word.
    last_word_tokens: usize,
    /// Whether the added text ends with a character that is not whitespace.
    ends_in_word: bool,
}

impl WordLogProbabilities {
    /// Adds a token that writes `text` after the healed text. Whitespace after a word starts the
    /// next word. A token that only writes healed text belongs to the first word.
    pub(super) fn add(&mut self, text: &[u8], log_probability: f32) {
        let is_new_word = self.starts_new_word(text);
        match self.words.last_mut() {
            Some(word) if !is_new_word => {
                word.0 += log_probability;
                self.last_word_tokens += 1;
            }
            _ => {
                self.words.push(LogProbability(log_probability));
                self.last_word_tokens = 1;
            }
        }
        if let Some(last) = text.last() {
            self.ends_in_word = !last.is_ascii_whitespace();
        }
    }

    /// Whether a token that writes `text` with `log_probability` keeps the added text at least as
    /// likely as `cut`. Each token of the first word does.
    pub(super) fn keeps(&self, text: &[u8], log_probability: f32, cut: LogProbability) -> bool {
        let is_in_first_word = self.words.len() <= 1 && !self.starts_new_word(text);
        is_in_first_word || self.total() + log_probability >= cut.0
    }

    /// The log probability of all the added text.
    pub(super) fn total(&self) -> f32 {
        self.words.iter().map(|word| word.0).sum()
    }

    /// The number of words.
    pub(super) fn count(&self) -> usize {
        self.words.len()
    }

    /// Ends the words before a token that writes `text`: it drops the last word when the token
    /// would continue it. Returns how many tokens the dropped word had.
    pub(super) fn end_before(&mut self, text: &[u8]) -> usize {
        if self.starts_new_word(text) {
            return 0;
        }
        self.drop_last_word()
    }

    /// Drops the last word, unless it is the only word. Returns how many tokens the dropped word
    /// had.
    pub(super) fn drop_last_word(&mut self) -> usize {
        if self.words.len() <= 1 {
            return 0;
        }
        self.words.pop();
        mem::take(&mut self.last_word_tokens)
    }

    /// Whether a token that writes `text` starts a new word: it starts a word after added text that
    /// ends inside a word.
    fn starts_new_word(&self, text: &[u8]) -> bool {
        self.ends_in_word && starts_word(text)
    }

    pub(super) fn into_vec(self) -> Vec<LogProbability> {
        self.words
    }
}

/// Whether a token with `piece` as text starts a word: the piece starts with ASCII whitespace.
pub(super) fn starts_word(piece: &[u8]) -> bool {
    piece.first().is_some_and(u8::is_ascii_whitespace)
}

/// Whether a token that adds `text` after the `written` text, which follows the `healed` text,
/// starts a new sentence: a new word after the end of a sentence.
pub(super) fn starts_next_sentence(healed: &[u8], written: &[u8], text: &[u8]) -> bool {
    starts_word(text) && ends_sentence(healed, written)
}

/// Whether the `written` text after the `healed` text ends a sentence. It must have a letter, so
/// that a sign after a word of the prompt or a number such as "1." does not end it. A period
/// counts only after a word of at least [`MIN_LETTERS_BEFORE_PERIOD`] letters, with the letters
/// of the healed text.
pub(super) fn ends_sentence(healed: &[u8], written: &[u8]) -> bool {
    let written = String::from_utf8_lossy(written);
    if !written.contains(char::is_alphabetic) {
        return false;
    }
    let Some(before_period) = written.strip_suffix('.') else {
        return written.ends_with(SENTENCE_ENDS);
    };
    let healed = String::from_utf8_lossy(healed);
    let word_letters = before_period
        .chars()
        .rev()
        .chain(healed.chars().rev())
        .take_while(|character| character.is_alphabetic())
        .count();
    word_letters >= MIN_LETTERS_BEFORE_PERIOD
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words_of(tokens: &[(&str, f32)]) -> WordLogProbabilities {
        let mut words = WordLogProbabilities::default();
        for (text, log_probability) in tokens {
            words.add(text.as_bytes(), *log_probability);
        }
        words
    }

    #[test]
    fn token_after_the_end_of_a_word_starts_the_next_word() {
        let words = words_of(&[("very", -1.0), (" soon", -2.0), (",", -0.5), (" thanks", -3.0)]);

        assert_eq!(
            words.into_vec(),
            [LogProbability(-1.0), LogProbability(-2.5), LogProbability(-3.0)]
        );
    }

    #[test]
    fn token_that_only_writes_the_healed_text_belongs_to_the_first_word() {
        let words = words_of(&[("", -0.5), (" meeting", -1.0), (" at", -2.0)]);

        assert_eq!(words.into_vec(), [LogProbability(-1.5), LogProbability(-2.0)]);
    }

    #[test]
    fn whitespace_after_whitespace_stays_in_its_word() {
        let words = words_of(&[("Hi", -1.0), (" ", -1.0), (" there", -1.0)]);

        assert_eq!(words.into_vec(), [LogProbability(-1.0), LogProbability(-2.0)]);
    }

    #[test]
    fn cut_never_stops_the_first_word() {
        let words = words_of(&[("", -0.5), (" meet", -2.0)]);

        let is_kept = words.keeps(b"ing", -4.0, LogProbability(-3.0));

        assert!(is_kept);
    }

    #[test]
    fn cut_keeps_a_later_token_while_the_text_stays_as_likely_as_the_cut() {
        let words = words_of(&[("very", -1.0)]);

        let kept = [(&b" soon"[..], -2.0), (b" soon", -2.5)]
            .map(|(text, log_probability)| words.keeps(text, log_probability, LogProbability(-3.0)));

        assert_eq!(kept, [true, false]);
    }

    #[test]
    fn stop_before_a_new_word_keeps_all_words() {
        let mut words = words_of(&[("very", -1.0), (" soon", -0.5)]);

        let dropped = words.end_before(b" thanks");

        assert_eq!(
            (dropped, words.into_vec()),
            (0, vec![LogProbability(-1.0), LogProbability(-0.5)])
        );
    }

    #[test]
    fn stop_inside_a_word_drops_that_word() {
        let mut words = words_of(&[("very", -1.0), (" th", -0.5), ("an", -0.5)]);

        let dropped = words.end_before(b"ks");

        assert_eq!((dropped, words.into_vec()), (2, vec![LogProbability(-1.0)]));
    }

    #[test]
    fn dropping_the_last_word_drops_its_tokens() {
        let mut words = words_of(&[("very", -1.0), (" th", -0.5), ("an", -0.5)]);

        let dropped = words.drop_last_word();

        assert_eq!((dropped, words.into_vec()), (2, vec![LogProbability(-1.0)]));
    }

    #[test]
    fn only_word_is_never_dropped() {
        let mut words = words_of(&[("1Z", -1.0), ("999", -0.5)]);

        let dropped = words.drop_last_word();

        assert_eq!((dropped, words.into_vec()), (0, vec![LogProbability(-1.5)]));
    }

    #[test]
    fn written_text_with_a_letter_and_a_closing_sign_ends_a_sentence() {
        let cases = [
            (&b""[..], &b"see you soon."[..], true),
            (b" Aish", b"a.", true),
            (b"", b"Really?!", true),
            (b"", "wait\u{2026}".as_bytes(), true),
            (b" Thanks", b".", false),
            (b"", b" 1.", false),
            (b"", b"soon", false),
            (b"", b"3.5", false),
        ];

        let ends = cases.map(|(healed, written, _)| ends_sentence(healed, written));

        assert_eq!(ends, cases.map(|(_, _, ends)| ends));
    }

    #[test]
    fn period_after_a_word_of_fewer_than_three_letters_does_not_end_a_sentence() {
        let cases = [
            (&b""[..], &b" Dr."[..]),
            (b" D", b"r."),
            (b"", b" e.g."),
            (b"", b"Ann J."),
        ];

        let ends = cases.map(|(healed, written)| ends_sentence(healed, written));

        assert_eq!(ends, [false; 4]);
    }

    #[test]
    fn only_a_new_word_after_the_end_of_a_sentence_starts_the_next_sentence() {
        let cases = [
            (&b"see you."[..], &b" Then"[..], true),
            (b"see you.", b"\nThen", true),
            (b"in May.", b"5", false),
            (b"see you", b" Then", false),
            (b"", b" Then", false),
        ];

        let starts = cases.map(|(written, text, _)| starts_next_sentence(b"", written, text));

        assert_eq!(starts, cases.map(|(_, _, starts)| starts));
    }

    #[test]
    fn only_a_piece_with_leading_whitespace_starts_a_word() {
        let cases = [
            (&b" beaut"[..], true),
            (b"\nHi", true),
            (b"i", false),
            (b"", false),
        ];

        let starts = cases.map(|(piece, _)| starts_word(piece));

        assert_eq!(starts, cases.map(|(_, starts)| starts));
    }
}
