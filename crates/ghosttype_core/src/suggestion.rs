//! The seam between the engine and whatever produces completions.

/// Produces a completion for the text typed so far.
pub trait SuggestionProvider {
    /// Returns the text to append after `context`, or `None` when there is nothing to suggest.
    ///
    /// `context` is the text typed since the caret last moved unpredictably, oldest first.
    /// A returned completion is never empty.
    fn suggest(&self, context: &str) -> Option<String>;
}
