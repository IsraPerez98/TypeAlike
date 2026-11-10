//! The dictionaries that the hub can download, each a set of files at a pinned commit of
//! `wooorm/dictionaries` with the size and the SHA-256 digest of each file.

mod pinned;

use typealike_core::hub::ByteSize;
use typealike_core::spelling::DownloadableDictionary;

/// The URL of the folder of the dictionaries at the pinned commit.
const SOURCE: &str = "https://raw.githubusercontent.com/wooorm/dictionaries";

/// A file of a dictionary, as the pinned commit has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PinnedFile {
    /// The name of the file in the folder of the dictionary.
    pub(crate) name: &'static str,
    /// The size in bytes.
    pub(crate) size: u64,
    /// The SHA-256 digest in lowercase hex.
    pub(crate) sha256: &'static str,
}

/// A dictionary of the catalog: its Hunspell affix file, its word list and its license.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CatalogEntry {
    /// The tag of the language, from the writing languages of core.
    pub(crate) language: &'static str,
    /// The folder of the dictionary in the source.
    pub(crate) folder: &'static str,
    /// The affix file, the word list and the license, in this order.
    pub(crate) files: [PinnedFile; 3],
}

impl CatalogEntry {
    /// The entry of the dictionary of `language`, a tag in any case and with `-` or `_`.
    pub(crate) fn of(language: &str) -> Option<&'static Self> {
        let language = language.replace('_', "-");
        pinned::ENTRIES
            .iter()
            .find(|entry| entry.language.eq_ignore_ascii_case(&language))
    }

    /// The size of all the files of the dictionary.
    pub(crate) fn size(&self) -> ByteSize {
        ByteSize(self.files.iter().map(|file| file.size).sum())
    }

    /// The URL of `file`, a file of the dictionary.
    pub(crate) fn url(&self, file: &PinnedFile) -> String {
        format!(
            "{SOURCE}/{}/dictionaries/{}/{}",
            pinned::COMMIT,
            self.folder,
            file.name
        )
    }
}

/// Each dictionary that the hub can download.
pub(crate) fn downloadable() -> Vec<DownloadableDictionary> {
    pinned::ENTRIES
        .iter()
        .map(|entry| DownloadableDictionary {
            language: entry.language.to_owned(),
            size: entry.size(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use typealike_core::spelling::language_name;

    use super::*;

    #[test]
    fn each_dictionary_is_a_writing_language_of_core() {
        let missing = pinned::ENTRIES
            .iter()
            .filter(|entry| language_name(entry.language).is_none())
            .map(|entry| entry.language)
            .collect::<Vec<_>>();

        assert_eq!(missing, Vec::<&str>::new());
    }

    #[test]
    fn entry_of_a_language_matches_its_exact_tag_only() {
        let tags = ["es-CL", "es_cl", "en-US", "es-419", "en"];

        let folders = tags.map(|tag| CatalogEntry::of(tag).map(|entry| entry.folder));

        assert_eq!(folders, [Some("es-CL"), Some("es-CL"), Some("en"), None, None]);
    }

    #[test]
    fn url_of_a_file_points_at_the_pinned_commit() {
        let entry = CatalogEntry::of("pt-BR").expect("the catalog has Portuguese of Brazil");

        let url = entry.url(&entry.files[1]);

        assert_eq!(
            url,
            "https://raw.githubusercontent.com/wooorm/dictionaries/8cfea406b505e4d7df52d5a19bce525df98c54ab/dictionaries/pt/index.dic"
        );
    }
}
