//! The text of each token of a vocabulary, sorted by text.

use llama_cpp_2::token::LlamaToken;

use super::words::starts_phrase_end;

/// The text of each token of a vocabulary, sorted by text, so that the tokens that agree with a
/// text come without a scan of the whole vocabulary.
#[derive(Debug)]
pub(super) struct TokenPieces {
    sorted: Vec<(Vec<u8>, LlamaToken)>,
    /// The tokens that end a phrase: a sign that ends a phrase, a line break or the end of the
    /// text.
    phrase_ends: Vec<LlamaToken>,
}

impl TokenPieces {
    /// Keeps each token with its text. It leaves out the tokens without text, which never agree.
    /// `ends_text` tells the tokens that end the text.
    pub(super) fn new(
        pieces: impl IntoIterator<Item = (LlamaToken, Vec<u8>)>,
        ends_text: impl Fn(LlamaToken) -> bool,
    ) -> Self {
        let mut sorted = Vec::new();
        let mut phrase_ends = Vec::new();
        for (token, piece) in pieces {
            if ends_text(token) || starts_phrase_end(&piece) {
                phrase_ends.push(token);
            }
            if !piece.is_empty() {
                sorted.push((piece, token));
            }
        }
        sorted.sort_unstable();
        Self { sorted, phrase_ends }
    }

    /// Returns the tokens that end a phrase: a sign that ends a phrase, a line break or the end of
    /// the text.
    pub(super) fn phrase_ends(&self) -> impl Iterator<Item = LlamaToken> + '_ {
        self.phrase_ends.iter().copied()
    }

    /// Returns the tokens that can write the start of `text`: their text is a start of `text`, or
    /// starts with all of `text`.
    pub(super) fn agreeing_with<'pieces>(
        &'pieces self,
        text: &'pieces [u8],
    ) -> impl Iterator<Item = LlamaToken> + 'pieces {
        self.agreeing_from(text, 1)
    }

    /// Returns the tokens that agree with `text`, without the tokens that write only the
    /// whitespace at its start. Such a token also comes before many other texts, for example
    /// before the digits of a number.
    pub(super) fn agreeing_past_whitespace<'pieces>(
        &'pieces self,
        text: &'pieces [u8],
    ) -> impl Iterator<Item = LlamaToken> + 'pieces {
        let whitespace = text.iter().take_while(|byte| byte.is_ascii_whitespace()).count();
        self.agreeing_from(text, whitespace + 1)
    }

    /// Returns the tokens whose text is a start of `text` with at least `shortest` bytes, or starts
    /// with all of `text`.
    fn agreeing_from<'pieces>(
        &'pieces self,
        text: &'pieces [u8],
        shortest: usize,
    ) -> impl Iterator<Item = LlamaToken> + 'pieces {
        let shorter = (shortest..text.len()).flat_map(|length| self.with_text(&text[..length]));
        shorter.chain(self.starting_with(text))
    }

    /// Returns the tokens that write as much of the start of `text` as one token can: the longest
    /// tokens whose text is a start of `text`, and the tokens whose text starts with all of `text`.
    pub(super) fn covering<'pieces>(
        &'pieces self,
        text: &'pieces [u8],
    ) -> impl Iterator<Item = LlamaToken> + 'pieces {
        let has_all = self.with_text(text).next().is_some();
        let longest_start = (1..text.len())
            .rev()
            .filter(move |_| !has_all)
            .find(|&length| self.with_text(&text[..length]).next().is_some());
        let starts = longest_start
            .into_iter()
            .flat_map(move |length| self.with_text(&text[..length]));
        starts.chain(self.starting_with(text))
    }

    fn with_text<'pieces>(&'pieces self, text: &'pieces [u8]) -> impl Iterator<Item = LlamaToken> + 'pieces {
        self.at_or_after(text)
            .take_while(move |(piece, _)| piece == text)
            .map(|&(_, token)| token)
    }

    fn starting_with<'pieces>(
        &'pieces self,
        text: &'pieces [u8],
    ) -> impl Iterator<Item = LlamaToken> + 'pieces {
        self.at_or_after(text)
            .take_while(move |(piece, _)| piece.starts_with(text))
            .map(|&(_, token)| token)
    }

    /// Returns the tokens in the order of their text, from the first one whose text is not before
    /// `text`.
    fn at_or_after(&self, text: &[u8]) -> impl Iterator<Item = &(Vec<u8>, LlamaToken)> {
        let first = self.sorted.partition_point(|(piece, _)| piece.as_slice() < text);
        self.sorted[first..].iter()
    }

    /// Keeps each of `texts` as the text of the token whose id is its index.
    #[cfg(test)]
    pub(super) fn from_texts(texts: &[&str]) -> Self {
        Self::new(
            texts
                .iter()
                .zip(0..)
                .map(|(text, id)| (LlamaToken(id), text.as_bytes().to_vec())),
            |_| false,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_that_write_a_start_of_the_text_or_all_of_it_agree() {
        let pieces = TokenPieces::from_texts(&[
            " beaut",
            " beautiful",
            " beast",
            "",
            " beauti",
            " b",
            "beauti",
            " beautify",
        ]);

        let mut agreeing = pieces.agreeing_with(b" beauti").collect::<Vec<_>>();
        agreeing.sort_unstable();

        assert_eq!(agreeing, [0, 1, 4, 5, 7].map(LlamaToken));
    }

    #[test]
    fn longest_token_inside_the_text_and_the_tokens_that_write_all_of_it_cover_it() {
        let pieces = TokenPieces::from_texts(&[" ", " n", " net", " netw", " network", " networks", " nest"]);

        let mut covering = pieces.covering(b" netwo").collect::<Vec<_>>();
        covering.sort_unstable();

        assert_eq!(covering, [3, 4, 5].map(LlamaToken));
    }

    #[test]
    fn token_with_all_of_the_text_leaves_the_shorter_tokens_out_of_the_cover() {
        let pieces = TokenPieces::from_texts(&[" ", " co", " cost", " costume", " cot"]);

        let mut covering = pieces.covering(b" cost").collect::<Vec<_>>();
        covering.sort_unstable();

        assert_eq!(covering, [2, 3].map(LlamaToken));
    }

    #[test]
    fn tokens_that_write_only_the_whitespace_at_the_start_do_not_agree_past_it() {
        let pieces = TokenPieces::from_texts(&[" ", "  ", " o", " on", " one", " on Monday."]);

        let mut agreeing = pieces.agreeing_past_whitespace(b" on Monday").collect::<Vec<_>>();
        agreeing.sort_unstable();

        assert_eq!(agreeing, [2, 3, 5].map(LlamaToken));
    }

    #[test]
    fn signs_line_breaks_and_the_end_of_the_text_end_a_phrase() {
        let texts = [" soon", ",", ".\n", "\n", "'s", " (", ""];
        let pieces = TokenPieces::new(
            texts
                .iter()
                .zip(0..)
                .map(|(text, id)| (LlamaToken(id), text.as_bytes().to_vec())),
            |token| token == LlamaToken(6),
        );

        let ends = pieces.phrase_ends().collect::<Vec<_>>();

        assert_eq!(ends, [1, 2, 3, 6].map(LlamaToken));
    }

    #[test]
    fn tokens_with_the_same_text_all_agree() {
        let pieces = TokenPieces::from_texts(&[" a", " a", " ab"]);

        let mut agreeing = pieces.agreeing_with(b" ab").collect::<Vec<_>>();
        agreeing.sort_unstable();

        assert_eq!(agreeing, [0, 1, 2].map(LlamaToken));
    }
}
