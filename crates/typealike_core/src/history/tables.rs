//! The word bias and the phrases of each pool of the earlier writing, and of each place of a pool.

use std::collections::HashMap;
use std::fmt;

use super::entry::HistoryEntry;
use super::phrases::{Phrases, continuation_in, next_word_bias_in, phrases};
use super::store::HistoryPool;
use super::word_bias::word_bias;
use crate::desktop::Timestamp;
use crate::inference::{NextWordBias, WordBias};
use crate::place::Place;
use crate::settings::{PersonalizationSettings, WordChoice};

/// The word bias and the phrases of each pool of the earlier writing, and the phrases of each place
/// of a pool. `Debug` shows only the number of pools, so logs never hold what the user wrote.
#[derive(Default)]
pub(crate) struct HistoryTables {
    pools: HashMap<HistoryPool, PoolTables>,
}

struct PoolTables {
    word_bias: Option<WordBias>,
    phrases: Phrases,
    place_phrases: HashMap<Place, Phrases>,
}

impl fmt::Debug for HistoryTables {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HistoryTables")
            .field("pools", &self.pools.len())
            .finish_non_exhaustive()
    }
}

impl HistoryTables {
    /// Builds the tables of `entries` for the pools and the word choice of `personalization`, at
    /// the time `now`. Each word bias gets a new version after `versions`, which then holds the
    /// newest version.
    pub(crate) fn of(
        entries: &[HistoryEntry],
        personalization: &PersonalizationSettings,
        versions: &mut u64,
        now: Option<Timestamp>,
    ) -> Self {
        let mut pools = HashMap::<HistoryPool, Vec<&HistoryEntry>>::new();
        for entry in entries {
            if let Some(pool) = HistoryPool::of(&entry.app_id, personalization) {
                pools.entry(pool).or_default().push(entry);
            }
        }
        let pools = pools
            .into_iter()
            .map(|(pool, texts)| {
                *versions = versions.wrapping_add(1);
                (
                    pool,
                    PoolTables::of(&texts, personalization.word_choice, *versions, now),
                )
            })
            .collect();
        Self { pools }
    }

    /// The word bias of `pool`, if the word choice is on.
    pub(crate) fn word_bias(&self, pool: &HistoryPool) -> Option<WordBias> {
        self.pools.get(pool)?.word_bias.clone()
    }

    /// Returns the rest of a phrase that the writer repeats after `text_before`, the text before
    /// the caret in `place`, from the texts of `pool`. The texts of the place decide when they have
    /// the last words, else all the texts of the pool.
    pub(crate) fn continuation_of(
        &self,
        pool: &HistoryPool,
        place: Place<&str>,
        text_before: &str,
    ) -> Option<&str> {
        continuation_in(&self.phrases_of(pool, place)?, text_before)
    }

    /// Returns the preference for the words that the writer wrote after the last words of
    /// `text_before`, the text before the caret in `place`, from the texts of `pool`: the words of
    /// the texts of the place, else the words of all the texts of the pool.
    pub(crate) fn next_word_bias(
        &self,
        pool: &HistoryPool,
        place: Place<&str>,
        text_before: &str,
    ) -> NextWordBias {
        self.phrases_of(pool, place)
            .map(|tables| next_word_bias_in(&tables, text_before))
            .unwrap_or_default()
    }

    /// The phrases of `place` in `pool`, if any, then the phrases of `pool`.
    fn phrases_of(&self, pool: &HistoryPool, place: Place<&str>) -> Option<Vec<&Phrases>> {
        let tables = self.pools.get(pool)?;
        let place_phrases = tables.place_phrases.get(&place.owned());
        Some(place_phrases.into_iter().chain([&tables.phrases]).collect())
    }
}

impl PoolTables {
    fn of(texts: &[&HistoryEntry], choice: WordChoice, version: u64, now: Option<Timestamp>) -> Self {
        let mut places = HashMap::<Place<&str>, Vec<&HistoryEntry>>::new();
        for &entry in texts {
            places.entry(entry.place()).or_default().push(entry);
        }
        Self {
            word_bias: word_bias(texts, choice, version, now),
            phrases: phrases(texts, choice, now),
            place_phrases: places
                .into_iter()
                .map(|(place, texts)| (place.owned(), phrases(&texts, choice, now)))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::test_support::{entry, mail, on_own_days, shared, word};

    fn place_and_pool_entries() -> Vec<HistoryEntry> {
        let mut entries = vec![mail("Let me know when you can."); 2];
        entries.extend(vec![
            entry(
                "Let me know if it works.",
                "net.whatsapp.WhatsApp",
                None
            );
            5
        ]);
        on_own_days(entries)
    }

    fn tables_of(entries: &[HistoryEntry]) -> HistoryTables {
        HistoryTables::of(entries, &shared(), &mut 0, None)
    }

    #[test]
    fn phrases_of_the_place_decide_when_they_have_the_last_words() {
        let tables = tables_of(&place_and_pool_entries());
        let apps = ["com.apple.mail", "com.apple.Notes"];

        let continuations = apps.map(|app_id| {
            tables.continuation_of(
                &HistoryPool::Shared,
                Place::of(app_id, None),
                "Please let me know ",
            )
        });

        assert_eq!(continuations, [None, Some("if")]);
    }

    #[test]
    fn next_words_of_the_place_come_before_the_next_words_of_the_pool() {
        let tables = tables_of(&place_and_pool_entries());
        let apps = ["com.apple.mail", "com.apple.Notes"];

        let next = apps.map(|app_id| {
            tables
                .next_word_bias(
                    &HistoryPool::Shared,
                    Place::of(app_id, None),
                    "Please let me know ",
                )
                .0
        });

        assert_eq!(
            next,
            [
                vec![word("when", 1.0)],
                vec![word("if", 5.0 / 7.0), word("when", 2.0 / 7.0)]
            ]
        );
    }

    #[test]
    fn debug_of_the_tables_hides_the_text() {
        let debug = format!("{:?}", tables_of(&place_and_pool_entries()));

        assert_eq!(debug, "HistoryTables { pools: 1, .. }");
    }
}
