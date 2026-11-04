//! The earlier writing of the user: the texts that TypeAlike saves, the secrets that it removes
//! from them, and the texts that a prompt shows as examples.

mod context;
mod entry;
mod examples;
mod phrases;
mod redaction;
mod reply;
mod store;
mod tables;
#[cfg(test)]
mod test_support;
mod word_bias;
mod words;

use std::fmt;
use std::io;

pub use context::typing_context;
pub use entry::HistoryEntry;
pub(crate) use examples::examples;
pub(crate) use reply::reply_words;
pub use reply::reply_words_of;
pub(crate) use store::{FinishedText, HistoryPool, add, own_text, session_texts};
pub(crate) use tables::HistoryTables;
pub use word_bias::word_bias_of_counts;
pub(crate) use words::{is_apostrophe, is_within_edits};

/// Keeps the earlier writing of the user between runs, on this computer only.
pub trait WritingHistory {
    /// Returns the kept entries, oldest first. Returns none when the store has none or cannot read
    /// them. The store reports its own failures.
    fn load(&mut self) -> Vec<HistoryEntry>;

    /// Keeps `entries`, oldest first, in place of all the kept entries. The store keeps its entries
    /// as they are when it could not read them. The store reports its own failures.
    fn save(&mut self, entries: &[HistoryEntry]);

    /// Deletes all the kept entries, also when the store could not read them. The store reports its
    /// own failures.
    fn delete(&mut self);
}

/// The secret key that locks the file of the earlier writing. `Debug` does not show it.
#[derive(Clone, PartialEq, Eq)]
pub struct HistoryKey(pub [u8; 32]);

impl fmt::Debug for HistoryKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HistoryKey(..)")
    }
}

/// Keeps the [`HistoryKey`] in the secure store of the OS, apart from the file that it locks.
pub trait KeyVault {
    /// Returns the kept key, or `None` when the vault has no key yet.
    ///
    /// # Errors
    ///
    /// Fails when the vault cannot be read, for example when the user does not allow it.
    fn load(&mut self) -> io::Result<Option<HistoryKey>>;

    /// Keeps `key` as the key of the vault.
    ///
    /// # Errors
    ///
    /// Fails when the vault cannot keep the key.
    fn store(&mut self, key: &HistoryKey) -> io::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_of_a_key_hides_the_key() {
        let debug = format!("{:?}", HistoryKey([7; 32]));

        assert_eq!(debug, "HistoryKey(..)");
    }
}
