//! The folder of the downloaded dictionaries: one folder for each language, with the files of its
//! dictionary.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::catalog::CatalogEntry;

/// The end of the name of the folder of a download that did not finish.
const PARTIAL_SUFFIX: &str = ".part";

/// Why a downloaded dictionary cannot be read.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ReadError {
    #[error("the catalog has no dictionary of that language")]
    Unknown,
    #[error("failed to read the dictionary files")]
    Storage(#[from] io::Error),
    #[error("the dictionary files are not a hunspell dictionary: {0}")]
    Parse(String),
}

/// The folder of the dictionary of `entry` in `dictionaries_dir`.
pub(crate) fn folder(dictionaries_dir: &Path, entry: &CatalogEntry) -> PathBuf {
    dictionaries_dir.join(entry.language)
}

/// The folder that a download of the dictionary of `entry` fills before it gets its name.
pub(crate) fn partial_folder(dictionaries_dir: &Path, entry: &CatalogEntry) -> PathBuf {
    dictionaries_dir.join(format!("{}{PARTIAL_SUFFIX}", entry.language))
}

/// The languages of the downloaded dictionaries in `dictionaries_dir`: each folder of a language of
/// the catalog that holds each file of its dictionary. A missing folder has none.
pub(crate) fn installed(dictionaries_dir: &Path) -> io::Result<Vec<String>> {
    let entries = match fs::read_dir(dictionaries_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut languages = Vec::new();
    for dir_entry in entries {
        let name = dir_entry?.file_name();
        let Some(entry) = name.to_str().and_then(CatalogEntry::of) else {
            continue;
        };
        let folder = folder(dictionaries_dir, entry);
        if entry.files.iter().all(|file| folder.join(file.name).is_file()) {
            languages.push(entry.language.to_owned());
        }
    }
    languages.sort();
    Ok(languages)
}

/// Reads the downloaded dictionary of `language` in `dictionaries_dir`.
pub(crate) fn read(dictionaries_dir: &Path, language: &str) -> Result<spellbook::Dictionary, ReadError> {
    let entry = CatalogEntry::of(language).ok_or(ReadError::Unknown)?;
    let folder = folder(dictionaries_dir, entry);
    let [aff, dic, _] = &entry.files;
    let aff = fs::read_to_string(folder.join(aff.name))?;
    let dic = fs::read_to_string(folder.join(dic.name))?;
    spellbook::Dictionary::new(&aff, &dic).map_err(|error| ReadError::Parse(error.to_string()))
}

/// Deletes the downloaded dictionary of `language` in `dictionaries_dir`. A dictionary that is not
/// there needs no delete.
pub(crate) fn delete(dictionaries_dir: &Path, language: &str) -> io::Result<()> {
    let Some(entry) = CatalogEntry::of(language) else {
        return Ok(());
    };
    match fs::remove_dir_all(folder(dictionaries_dir, entry)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A new empty folder in the temp folder for one test.
    pub(crate) fn temporary_folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("typealike_dictionaries_{name}_{}", std::process::id()));
        fs::remove_dir_all(&folder).ok();
        fs::create_dir_all(&folder).expect("the temp folder is writable");
        folder
    }

    /// Writes a dictionary of Esperanto that knows `words` into `dictionaries_dir`.
    pub(crate) fn write_dictionary(dictionaries_dir: &Path, words: &[&str]) {
        let entry = CatalogEntry::of("eo").expect("the catalog has Esperanto");
        let folder = folder(dictionaries_dir, entry);
        fs::create_dir_all(&folder).expect("the temp folder is writable");
        let dic = format!("{}\n{}\n", words.len(), words.join("\n"));
        for (file, text) in entry.files.iter().zip(["SET UTF-8\n", dic.as_str(), "license"]) {
            fs::write(folder.join(file.name), text).expect("the temp folder is writable");
        }
    }

    #[test]
    fn installed_dictionaries_are_the_complete_folders_of_catalog_languages() {
        let dir = temporary_folder("installed");
        write_dictionary(&dir, &["saluton"]);
        fs::create_dir_all(dir.join("es-CL")).expect("the temp folder is writable");
        fs::create_dir_all(dir.join("klingon")).expect("the temp folder is writable");

        let languages = installed(&dir).expect("the folder is readable");

        fs::remove_dir_all(&dir).ok();
        assert_eq!(languages, ["eo"]);
    }

    #[test]
    fn missing_folder_has_no_dictionaries() {
        let dir = std::env::temp_dir().join("typealike_dictionaries_missing_folder");

        let languages = installed(&dir).expect("a missing folder is empty");

        assert_eq!(languages, Vec::<String>::new());
    }

    #[test]
    fn read_dictionary_knows_its_words() {
        let dir = temporary_folder("read");
        write_dictionary(&dir, &["saluton", "mondo"]);

        let dictionary = read(&dir, "eo").expect("the dictionary is readable");

        fs::remove_dir_all(&dir).ok();
        let answers = ["saluton", "mondo", "zorbly"].map(|word| dictionary.check(word));
        assert_eq!(answers, [true, true, false]);
    }

    #[test]
    fn delete_removes_the_folder_of_the_dictionary() {
        let dir = temporary_folder("delete");
        write_dictionary(&dir, &["saluton"]);

        delete(&dir, "eo").expect("the folder is writable");

        let languages = installed(&dir).expect("the folder is readable");
        fs::remove_dir_all(&dir).ok();
        assert_eq!(languages, Vec::<String>::new());
    }
}
