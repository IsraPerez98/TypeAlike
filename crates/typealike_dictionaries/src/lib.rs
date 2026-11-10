//! The Hunspell dictionaries that TypeAlike downloads for the languages that the OS has no
//! dictionary for.
//!
//! [`HunspellDictionaries`] implements the core [`DictionaryHub`] seam. It downloads each
//! dictionary from a pinned commit of `wooorm/dictionaries` on GitHub, keeps it only when each file
//! has its pinned size and SHA-256 digest, and reads it with `spellbook`. The downloads, the reads
//! and the list of the downloaded dictionaries run on their own threads and report through the
//! caller's sink, so the network and the disk never block the caller's thread.

mod catalog;
mod download;
mod store;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

use typealike_core::hub::HubError;
use typealike_core::spelling::{
    DictionaryEvent, DictionaryEventSink, DictionaryHub, DictionaryRequest, DownloadableDictionary,
    SpellChecker,
};

use crate::catalog::CatalogEntry;
use crate::download::FetchError;

/// The dictionaries that the hub read, by language, shared with its threads.
#[derive(Debug, Clone, Default)]
struct LoadedDictionaries(Arc<Mutex<HashMap<String, Arc<spellbook::Dictionary>>>>);

impl LoadedDictionaries {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Arc<spellbook::Dictionary>>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Downloads Hunspell dictionaries into one local folder, and checks words with the ones that it
/// read.
#[derive(Debug)]
pub struct HunspellDictionaries {
    agent: ureq::Agent,
    dictionaries_dir: PathBuf,
    cancel_flags: HashMap<String, Arc<AtomicBool>>,
    loaded: LoadedDictionaries,
}

impl HunspellDictionaries {
    /// Creates a hub that keeps the downloaded dictionaries in `dictionaries_dir`, created on the
    /// first download.
    #[must_use]
    pub fn new(dictionaries_dir: PathBuf) -> Self {
        Self {
            agent: download::new_agent(),
            dictionaries_dir,
            cancel_flags: HashMap::new(),
            loaded: LoadedDictionaries::default(),
        }
    }

    fn start_download(&mut self, language: String, sink: DictionaryEventSink) {
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel_flags.insert(language.clone(), Arc::clone(&cancel));
        let agent = self.agent.clone();
        let dictionaries_dir = self.dictionaries_dir.clone();
        thread::spawn(move || {
            let result = CatalogEntry::of(&language)
                .ok_or(FetchError::Unknown)
                .and_then(|entry| {
                    download::download(&agent, &dictionaries_dir, entry, &cancel, |received| {
                        sink(DictionaryEvent::DownloadProgress {
                            language: language.clone(),
                            received,
                        });
                    })
                });
            let result = result.map_err(|error| {
                if !matches!(error, FetchError::Canceled) {
                    tracing::warn!(?error, "failed to download a dictionary");
                }
                error.to_hub_error()
            });
            sink(DictionaryEvent::DownloadFinished { language, result });
        });
    }

    fn start_load(&self, language: String, sink: DictionaryEventSink) {
        let dictionaries_dir = self.dictionaries_dir.clone();
        let loaded = self.loaded.clone();
        thread::spawn(move || {
            let result = match store::read(&dictionaries_dir, &language) {
                Ok(dictionary) => {
                    loaded.lock().insert(language.clone(), Arc::new(dictionary));
                    Ok(())
                }
                Err(error) => {
                    tracing::warn!(%error, "failed to read a downloaded dictionary");
                    Err(HubError::Storage)
                }
            };
            sink(DictionaryEvent::Loaded { language, result });
        });
    }
}

impl DictionaryHub for HunspellDictionaries {
    fn catalog(&self) -> Vec<DownloadableDictionary> {
        catalog::downloadable()
    }

    fn submit(&mut self, request: DictionaryRequest, sink: &DictionaryEventSink) {
        let sink = Arc::clone(sink);
        match request {
            DictionaryRequest::ListInstalled => {
                let dictionaries_dir = self.dictionaries_dir.clone();
                thread::spawn(move || match store::installed(&dictionaries_dir) {
                    Ok(languages) => sink(DictionaryEvent::Installed(languages)),
                    Err(error) => tracing::warn!(%error, "failed to list the downloaded dictionaries"),
                });
            }
            DictionaryRequest::Load { language } => self.start_load(language, sink),
            DictionaryRequest::Download { language } => self.start_download(language, sink),
            DictionaryRequest::CancelDownload { language } => {
                if let Some(cancel) = self.cancel_flags.remove(&language) {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            DictionaryRequest::Delete { language } => {
                self.loaded.lock().remove(&language);
                if let Err(error) = store::delete(&self.dictionaries_dir, &language) {
                    tracing::warn!(%error, "failed to delete a downloaded dictionary");
                }
            }
        }
    }

    fn open(&self, languages: &[String]) -> Box<dyn SpellChecker> {
        let loaded = self.loaded.lock();
        let dictionaries = languages
            .iter()
            .filter_map(|language| loaded.get(language).cloned())
            .collect();
        Box::new(HunspellChecker(dictionaries))
    }
}

/// Hunspell dictionaries that know a word when any of them knows it.
struct HunspellChecker(Vec<Arc<spellbook::Dictionary>>);

impl SpellChecker for HunspellChecker {
    fn is_known_word(&self, word: &str) -> bool {
        self.0.iter().any(|dictionary| dictionary.check(word))
    }

    fn is_word_start(&self, letters: &str) -> bool {
        // Hunspell gives no completions, so only letters that are a whole word count.
        self.is_known_word(letters)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;
    use crate::store::tests::{temporary_folder, write_dictionary};

    /// A sink that sends each event to the returned receiver.
    fn channel_sink() -> (DictionaryEventSink, mpsc::Receiver<DictionaryEvent>) {
        let (sender, receiver) = mpsc::channel();
        let sender = Mutex::new(sender);
        let sink: DictionaryEventSink = Arc::new(move |event| {
            sender
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .send(event)
                .ok();
        });
        (sink, receiver)
    }

    fn next_event(receiver: &mpsc::Receiver<DictionaryEvent>) -> DictionaryEvent {
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("the hub answers")
    }

    #[test]
    fn catalog_offers_spanish_of_chile_with_the_size_of_its_files() {
        let hub = HunspellDictionaries::new(temporary_folder("catalog"));

        let chile = hub
            .catalog()
            .into_iter()
            .find(|dictionary| dictionary.language == "es-CL");

        assert_eq!(chile.map(|dictionary| dictionary.size.0 > 800_000), Some(true));
    }

    #[test]
    fn list_gives_the_downloaded_dictionaries() {
        let dir = temporary_folder("list");
        write_dictionary(&dir, &["saluton"]);
        let mut hub = HunspellDictionaries::new(dir.clone());
        let (sink, receiver) = channel_sink();

        hub.submit(DictionaryRequest::ListInstalled, &sink);

        let event = next_event(&receiver);
        fs::remove_dir_all(&dir).ok();
        assert_eq!(event, DictionaryEvent::Installed(vec!["eo".to_owned()]));
    }

    #[test]
    fn read_dictionary_checks_the_words_of_its_language_only() {
        let dir = temporary_folder("open");
        write_dictionary(&dir, &["saluton"]);
        let mut hub = HunspellDictionaries::new(dir.clone());
        let (sink, receiver) = channel_sink();
        hub.submit(
            DictionaryRequest::Load {
                language: "eo".to_owned(),
            },
            &sink,
        );
        let loaded = next_event(&receiver);

        let checkers = [vec!["eo".to_owned()], Vec::new()].map(|languages| hub.open(&languages));

        fs::remove_dir_all(&dir).ok();
        let answers = checkers.map(|checker| checker.is_known_word("saluton"));
        assert_eq!(
            (loaded, answers),
            (
                DictionaryEvent::Loaded {
                    language: "eo".to_owned(),
                    result: Ok(()),
                },
                [true, false]
            )
        );
    }

    #[test]
    fn deleted_dictionary_checks_no_word() {
        let dir = temporary_folder("delete_hub");
        write_dictionary(&dir, &["saluton"]);
        let mut hub = HunspellDictionaries::new(dir.clone());
        let (sink, receiver) = channel_sink();
        let language = "eo".to_owned();
        hub.submit(
            DictionaryRequest::Load {
                language: language.clone(),
            },
            &sink,
        );
        next_event(&receiver);

        hub.submit(
            DictionaryRequest::Delete {
                language: language.clone(),
            },
            &sink,
        );

        let is_known = hub.open(&[language]).is_known_word("saluton");
        fs::remove_dir_all(&dir).ok();
        assert!(!is_known);
    }

    #[test]
    fn read_of_a_missing_dictionary_fails() {
        let dir = temporary_folder("missing_read");
        let mut hub = HunspellDictionaries::new(dir.clone());
        let (sink, receiver) = channel_sink();

        hub.submit(
            DictionaryRequest::Load {
                language: "eo".to_owned(),
            },
            &sink,
        );

        let event = next_event(&receiver);
        fs::remove_dir_all(&dir).ok();
        assert_eq!(
            event,
            DictionaryEvent::Loaded {
                language: "eo".to_owned(),
                result: Err(HubError::Storage),
            }
        );
    }
}
