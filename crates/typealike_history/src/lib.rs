//! The earlier writing of the TypeAlike user, in one encrypted file on this computer.
//!
//! [`HistoryFile`] implements the core [`WritingHistory`] seam. The file holds the entries as JSON
//! lines, sealed with ChaCha20-Poly1305 and a new random nonce at each write. The key stays in the
//! [`KeyVault`] of the OS, and the file reads it only when it first reads or writes the entries.
//! Nothing here uses the network.

use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use ring::aead::{Aad, CHACHA20_POLY1305, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use typealike_core::desktop::Timestamp;
use typealike_core::history::{HistoryEntry, HistoryKey, KeyVault, WritingHistory};

/// The start of the file, which names its format.
const FORMAT: &[u8] = b"GTH1";

/// The extension of the copy of a file that a read could not use in full.
const BACKUP_EXTENSION: &str = "bin.backup";

/// Why the entries could not be read or written. No error holds the text of an entry.
#[derive(Debug, thiserror::Error)]
enum HistoryError {
    #[error("the file cannot be read or written")]
    File(#[from] io::Error),
    #[error("the key vault cannot give the key")]
    Vault(#[source] io::Error),
    #[error("the system cannot give random bytes")]
    Random,
    #[error("the file is damaged or was locked with another key")]
    Locked,
    #[error("the file was not read, so it is kept as it is")]
    Unread,
}

/// One entry as the file keeps it.
#[derive(Serialize, Deserialize)]
struct StoredEntry {
    text: String,
    app_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    domain: Option<String>,
    /// The seconds since the Unix epoch when the user started to write the text. An entry of an
    /// older file has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    written_at: Option<u64>,
    /// The words of the message that the text answers. An entry of an older file has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    reply_to: Vec<String>,
    /// The typing context of the text. An entry of an older file has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    typing_context: Option<String>,
}

/// What a save can do with the file after the last read.
#[derive(Clone, Copy)]
enum Replace {
    /// No read got the file, so a save keeps it as it is.
    Refused,
    Allowed,
    /// The read could not use all of the file, so a save first keeps a copy of it.
    AfterBackup,
}

/// The entries of a file, and the count of its lines that are not an entry.
struct Decoded {
    entries: Vec<HistoryEntry>,
    skipped_lines: usize,
}

/// The file that keeps the earlier writing of the user, locked with a key of the `vault`. A save
/// changes the file only after a read that got the entries of the file or found the file unusable,
/// so that a vault that fails one time never makes it lose the entries. Before the first save
/// replaces a file that the read could not use in full, it keeps a copy of the file next to it,
/// with the extension `bin.backup`. Only a delete removes the file and its copy without such a read.
pub struct HistoryFile {
    path: PathBuf,
    vault: Box<dyn KeyVault>,
    key: Option<LessSafeKey>,
    random: SystemRandom,
    replace: Replace,
}

impl std::fmt::Debug for HistoryFile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HistoryFile")
            .field("path", &self.path)
            .field("has_key", &self.key.is_some())
            .finish_non_exhaustive()
    }
}

impl HistoryFile {
    /// Creates the history in the file at `path`, locked with the key of `vault`. The vault makes
    /// a new key the first time.
    #[must_use]
    pub fn new(path: PathBuf, vault: Box<dyn KeyVault>) -> Self {
        Self {
            path,
            vault,
            key: None,
            random: SystemRandom::new(),
            replace: Replace::Refused,
        }
    }

    fn read(&mut self) -> Result<Decoded, HistoryError> {
        let sealed = match fs::read(&self.path) {
            Ok(sealed) => sealed,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Decoded {
                    entries: Vec::new(),
                    skipped_lines: 0,
                });
            }
            Err(error) => return Err(error.into()),
        };
        open(self.key()?, &sealed)
            .and_then(decode)
            .ok_or(HistoryError::Locked)
    }

    fn backup_path(&self) -> PathBuf {
        self.path.with_extension(BACKUP_EXTENSION)
    }

    /// Replaces the file with `entries`, or removes it when there are none. It changes the file only
    /// after a read that got the entries of the file or found the file unusable.
    fn replace(&mut self, entries: &[HistoryEntry]) -> Result<(), HistoryError> {
        match self.replace {
            Replace::Refused => return Err(HistoryError::Unread),
            Replace::AfterBackup => {
                write_synced(&self.backup_path(), &fs::read(&self.path)?)?;
                self.replace = Replace::Allowed;
            }
            Replace::Allowed => {}
        }
        if entries.is_empty() {
            remove(&self.path)
        } else {
            self.write(entries)
        }
    }

    /// Writes a new file next to the old one, then replaces the old one with it, so that a failed
    /// write never leaves a damaged file.
    fn write(&mut self, entries: &[HistoryEntry]) -> Result<(), HistoryError> {
        let plain = entries
            .iter()
            .map(|entry| serde_json::to_string(&StoredEntry::from(entry)).map(|line| line + "\n"))
            .collect::<Result<String, _>>()
            .expect("entries of strings always serialize");
        let mut nonce = [0; NONCE_LEN];
        self.random
            .fill(&mut nonce)
            .map_err(|_unspecified| HistoryError::Random)?;
        let sealed = seal(self.key()?, nonce, plain.into_bytes())?;
        if let Some(folder) = self.path.parent() {
            fs::create_dir_all(folder)?;
        }
        let staged = self.path.with_extension("bin.new");
        write_synced(&staged, &sealed)?;
        fs::rename(&staged, &self.path)?;
        Ok(())
    }

    /// Returns the key of the vault, and makes and keeps a new key when the vault has none.
    fn key(&mut self) -> Result<&LessSafeKey, HistoryError> {
        if self.key.is_none() {
            let key = match self.vault.load().map_err(HistoryError::Vault)? {
                Some(key) => key,
                None => self.new_key()?,
            };
            let unbound =
                UnboundKey::new(&CHACHA20_POLY1305, &key.0).map_err(|_unspecified| HistoryError::Locked)?;
            self.key = Some(LessSafeKey::new(unbound));
        }
        Ok(self.key.as_ref().expect("the key was set above"))
    }

    fn new_key(&mut self) -> Result<HistoryKey, HistoryError> {
        let mut bytes = [0; 32];
        self.random
            .fill(&mut bytes)
            .map_err(|_unspecified| HistoryError::Random)?;
        let key = HistoryKey(bytes);
        self.vault.store(&key).map_err(HistoryError::Vault)?;
        Ok(key)
    }
}

impl WritingHistory for HistoryFile {
    fn load(&mut self) -> Vec<HistoryEntry> {
        let (entries, replace) = match self.read() {
            Ok(Decoded {
                entries,
                skipped_lines: 0,
            }) => (entries, Replace::Allowed),
            Ok(Decoded {
                entries,
                skipped_lines,
            }) => {
                tracing::warn!(
                    skipped_lines,
                    "skipped the lines of the earlier writing that are not entries"
                );
                (entries, Replace::AfterBackup)
            }
            Err(error) => {
                tracing::warn!(%error, "failed to read the earlier writing, starting with none");
                let replace = match error {
                    HistoryError::Locked => Replace::AfterBackup,
                    _ => Replace::Refused,
                };
                (Vec::new(), replace)
            }
        };
        self.replace = replace;
        entries
    }

    fn save(&mut self, entries: &[HistoryEntry]) {
        if let Err(error) = self.replace(entries) {
            tracing::warn!(%error, "failed to save the earlier writing");
        }
    }

    fn delete(&mut self) {
        for path in [self.path.clone(), self.backup_path()] {
            if let Err(error) = remove(&path) {
                tracing::warn!(%error, "failed to delete the earlier writing");
            }
        }
    }
}

/// Removes the file at `path`. A missing file is not an error.
fn remove(path: &Path) -> Result<(), HistoryError> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

/// Writes `bytes` to the file at `path`, and returns only after they are on the disk.
fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Returns the file that holds `plain`, sealed with `key` and `nonce`.
fn seal(key: &LessSafeKey, nonce: [u8; NONCE_LEN], mut plain: Vec<u8>) -> Result<Vec<u8>, HistoryError> {
    key.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(FORMAT), &mut plain)
        .map_err(|_unspecified| HistoryError::Locked)?;
    Ok([FORMAT, &nonce, &plain].concat())
}

/// Returns the text of the `sealed` file, opened with `key`, or `None` when the file is damaged or
/// was locked with another key.
fn open(key: &LessSafeKey, sealed: &[u8]) -> Option<Vec<u8>> {
    let body = sealed.strip_prefix(FORMAT)?;
    let (nonce, cipher) = body.split_at_checked(NONCE_LEN)?;
    let nonce = Nonce::try_assume_unique_for_key(nonce).ok()?;
    let mut cipher = cipher.to_vec();
    let plain_len = key
        .open_in_place(nonce, Aad::from(FORMAT), &mut cipher)
        .ok()?
        .len();
    cipher.truncate(plain_len);
    Some(cipher)
}

/// Returns the entries in the JSON lines of `plain`, without the lines that are not an entry, or
/// `None` when `plain` is not UTF-8.
fn decode(plain: Vec<u8>) -> Option<Decoded> {
    let text = String::from_utf8(plain).ok()?;
    let stored: Vec<Option<StoredEntry>> = text.lines().map(|line| serde_json::from_str(line).ok()).collect();
    let skipped_lines = stored.iter().filter(|entry| entry.is_none()).count();
    let entries = stored.into_iter().flatten().map(HistoryEntry::from).collect();
    Some(Decoded {
        entries,
        skipped_lines,
    })
}

impl From<StoredEntry> for HistoryEntry {
    fn from(stored: StoredEntry) -> Self {
        Self {
            text: stored.text,
            app_id: stored.app_id,
            domain: stored.domain,
            written_at: stored.written_at.map(Timestamp),
            reply_to: stored.reply_to,
            typing_context: stored.typing_context,
        }
    }
}

impl From<&HistoryEntry> for StoredEntry {
    fn from(entry: &HistoryEntry) -> Self {
        Self {
            text: entry.text.clone(),
            app_id: entry.app_id.clone(),
            domain: entry.domain.clone(),
            written_at: entry.written_at.map(|written_at| written_at.0),
            reply_to: entry.reply_to.clone(),
            typing_context: entry.typing_context.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    /// A vault in memory, which also counts its reads.
    #[derive(Clone, Default)]
    struct FakeVault {
        key: Rc<RefCell<Option<HistoryKey>>>,
        loads: Rc<RefCell<usize>>,
    }

    impl KeyVault for FakeVault {
        fn load(&mut self) -> io::Result<Option<HistoryKey>> {
            *self.loads.borrow_mut() += 1;
            Ok(self.key.borrow().clone())
        }

        fn store(&mut self, key: &HistoryKey) -> io::Result<()> {
            *self.key.borrow_mut() = Some(key.clone());
            Ok(())
        }
    }

    /// A vault that fails its first read, as when the user does not allow it one time.
    #[derive(Clone, Default)]
    struct OnceLockedVault {
        inner: FakeVault,
        has_failed: Rc<RefCell<bool>>,
    }

    impl KeyVault for OnceLockedVault {
        fn load(&mut self) -> io::Result<Option<HistoryKey>> {
            if !self.has_failed.replace(true) {
                return Err(io::Error::other("the user did not allow the vault"));
            }
            self.inner.load()
        }

        fn store(&mut self, key: &HistoryKey) -> io::Result<()> {
            self.inner.store(key)
        }
    }

    struct LockedVault;

    impl KeyVault for LockedVault {
        fn load(&mut self) -> io::Result<Option<HistoryKey>> {
            Err(io::Error::other("the user did not allow the vault"))
        }

        fn store(&mut self, _key: &HistoryKey) -> io::Result<()> {
            Err(io::Error::other("the user did not allow the vault"))
        }
    }

    fn temporary_file(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("typealike-history-{}", std::process::id()));
        fs::create_dir_all(&folder).expect("the temporary folder can be made");
        let path = folder.join(name);
        fs::remove_file(&path).ok();
        fs::remove_file(path.with_extension(BACKUP_EXTENSION)).ok();
        path
    }

    /// Writes the JSON `lines`, sealed with the key of `vault`, to the file at `path`, and returns
    /// the bytes of the file.
    fn write_lines(path: &Path, vault: &FakeVault, lines: &str) -> Vec<u8> {
        let mut writer = HistoryFile::new(path.to_owned(), Box::new(vault.clone()));
        let key = writer.key().expect("the vault gives a key");
        let sealed = seal(key, [0; NONCE_LEN], lines.as_bytes().to_vec()).expect("the lines can be sealed");
        fs::write(path, &sealed).expect("the file can be written");
        sealed
    }

    const LINES_WITH_AN_UNKNOWN_SHAPE: &str = concat!(
        r#"{"text":"Cheers,\nTom","app_id":"com.apple.mail"}"#,
        "\n",
        r#"{"text":"LGTM","app_id":"com.google.Chrome","written_at":"2026-11-07"}"#,
        "\n",
    );

    fn entries() -> Vec<HistoryEntry> {
        vec![
            HistoryEntry {
                text: "Cheers,\nTom".to_owned(),
                app_id: "com.apple.mail".to_owned(),
                domain: None,
                written_at: None,
                reply_to: Vec::new(),
                typing_context: None,
            },
            HistoryEntry {
                text: "LGTM, ship it".to_owned(),
                app_id: "com.google.Chrome".to_owned(),
                domain: Some("github.com".to_owned()),
                written_at: Some(Timestamp(1_793_199_600)),
                reply_to: vec!["merge".to_owned(), "review".to_owned()],
                typing_context: Some("#team-web | acme".to_owned()),
            },
        ]
    }

    /// Returns a history in the file at `path` that read the file, so that it can replace it.
    fn read_history(path: &Path, vault: Box<dyn KeyVault>) -> HistoryFile {
        let mut history = HistoryFile::new(path.to_owned(), vault);
        history.load();
        history
    }

    #[test]
    fn entries_survive_a_save_and_a_load() {
        let path = temporary_file("round-trip.bin");
        let vault = FakeVault::default();
        read_history(&path, Box::new(vault.clone())).save(&entries());

        let loaded = HistoryFile::new(path, Box::new(vault)).load();

        assert_eq!(loaded, entries());
    }

    #[test]
    fn entries_are_not_saved_before_a_read() {
        let path = temporary_file("unread.bin");

        HistoryFile::new(path.clone(), Box::new(FakeVault::default())).save(&entries());

        assert!(!path.exists());
    }

    #[test]
    fn vault_that_failed_at_the_read_keeps_the_file_as_it_is() {
        let path = temporary_file("vault-failed.bin");
        let vault = OnceLockedVault::default();
        read_history(&path, Box::new(vault.inner.clone())).save(&entries());
        let saved = fs::read(&path).expect("the file exists");
        let mut history = read_history(&path, Box::new(vault));

        history.save(&entries()[..1]);

        assert_eq!(fs::read(&path).expect("the file exists"), saved);
    }

    #[test]
    fn vault_that_failed_at_the_read_keeps_the_file_when_no_entries_are_saved() {
        let path = temporary_file("vault-failed-emptied.bin");
        let vault = OnceLockedVault::default();
        read_history(&path, Box::new(vault.inner.clone())).save(&entries());
        let mut history = read_history(&path, Box::new(vault.clone()));

        history.save(&[]);

        assert_eq!(HistoryFile::new(path, Box::new(vault)).load(), entries());
    }

    #[test]
    fn missing_key_keeps_a_copy_of_the_history_file() {
        let path = temporary_file("missing-key.bin");
        read_history(&path, Box::new(FakeVault::default())).save(&entries());
        let saved = fs::read(&path).expect("the file exists");
        let mut history = read_history(&path, Box::new(FakeVault::default()));

        history.save(&entries()[..1]);

        let backup = fs::read(path.with_extension(BACKUP_EXTENSION)).expect("the copy exists");
        assert_eq!(backup, saved);
    }

    #[test]
    fn entry_of_an_unknown_shape_keeps_a_copy_of_the_history_file() {
        let path = temporary_file("unknown-shape.bin");
        let vault = FakeVault::default();
        let sealed = write_lines(&path, &vault, LINES_WITH_AN_UNKNOWN_SHAPE);
        let mut history = read_history(&path, Box::new(vault));

        history.save(&entries()[..1]);

        let backup = fs::read(path.with_extension(BACKUP_EXTENSION)).expect("the copy exists");
        assert_eq!(backup, sealed);
    }

    #[test]
    fn entry_of_an_unknown_shape_is_skipped() {
        let path = temporary_file("skipped-shape.bin");
        let vault = FakeVault::default();
        write_lines(&path, &vault, LINES_WITH_AN_UNKNOWN_SHAPE);

        let loaded = HistoryFile::new(path, Box::new(vault)).load();

        assert_eq!(loaded, entries()[..1]);
    }

    #[test]
    fn readable_file_gets_no_backup_copy() {
        let path = temporary_file("readable.bin");
        let vault = FakeVault::default();
        read_history(&path, Box::new(vault.clone())).save(&entries());
        let mut history = read_history(&path, Box::new(vault));

        history.save(&entries()[..1]);

        assert!(!path.with_extension(BACKUP_EXTENSION).exists());
    }

    #[test]
    fn backup_copy_survives_later_saves() {
        let path = temporary_file("later-saves.bin");
        fs::write(&path, b"damaged").expect("the file can be written");
        let mut history = read_history(&path, Box::new(FakeVault::default()));
        history.save(&entries());

        history.save(&entries()[..1]);

        let backup = fs::read(path.with_extension(BACKUP_EXTENSION)).expect("the copy exists");
        assert_eq!(backup, b"damaged");
    }

    #[test]
    fn delete_removes_the_backup_copy() {
        let path = temporary_file("deleted-backup.bin");
        fs::write(&path, b"damaged").expect("the file can be written");
        let mut history = read_history(&path, Box::new(FakeVault::default()));
        history.save(&entries());

        history.delete();

        assert!(!path.with_extension(BACKUP_EXTENSION).exists());
    }

    #[test]
    fn delete_removes_the_file_before_a_read() {
        let path = temporary_file("deleted.bin");
        let vault = FakeVault::default();
        read_history(&path, Box::new(vault.clone())).save(&entries());

        HistoryFile::new(path.clone(), Box::new(vault)).delete();

        assert!(!path.exists());
    }

    #[test]
    fn damaged_file_is_replaced_after_a_read() {
        let path = temporary_file("replaced.bin");
        fs::write(&path, b"damaged").expect("the file can be written");
        let vault = FakeVault::default();

        read_history(&path, Box::new(vault.clone())).save(&entries());

        assert_eq!(HistoryFile::new(path, Box::new(vault)).load(), entries());
    }

    #[test]
    fn file_starts_with_the_format_name() {
        let path = temporary_file("format.bin");

        read_history(&path, Box::new(FakeVault::default())).save(&entries());

        assert!(fs::read(&path).expect("the file exists").starts_with(FORMAT));
    }

    #[test]
    fn file_has_no_readable_text() {
        let path = temporary_file("sealed.bin");

        read_history(&path, Box::new(FakeVault::default())).save(&entries());

        let bytes = fs::read(&path).expect("the file exists");
        let holds = |text: &[u8]| bytes.windows(text.len()).any(|window| window == text);
        assert!(
            ![&b"Tom"[..], b"LGTM", b"com.apple", b"github"]
                .into_iter()
                .any(holds)
        );
    }

    #[test]
    fn missing_file_has_no_entries_and_reads_no_key() {
        let vault = FakeVault::default();

        let loaded = HistoryFile::new(temporary_file("missing.bin"), Box::new(vault.clone())).load();

        assert_eq!((loaded, *vault.loads.borrow()), (Vec::new(), 0));
    }

    #[test]
    fn file_of_another_key_has_no_entries() {
        let path = temporary_file("other-key.bin");
        read_history(&path, Box::new(FakeVault::default())).save(&entries());

        let loaded = HistoryFile::new(path, Box::new(FakeVault::default())).load();

        assert_eq!(loaded, Vec::new());
    }

    #[test]
    fn damaged_file_has_no_entries() {
        let path = temporary_file("damaged.bin");
        let vault = FakeVault::default();
        read_history(&path, Box::new(vault.clone())).save(&entries());
        let mut bytes = fs::read(&path).expect("the file exists");
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(&path, bytes).expect("the file can be written");

        let loaded = HistoryFile::new(path, Box::new(vault)).load();

        assert_eq!(loaded, Vec::new());
    }

    #[test]
    fn locked_vault_saves_nothing() {
        let path = temporary_file("locked-vault.bin");

        read_history(&path, Box::new(LockedVault)).save(&entries());

        assert!(!path.exists());
    }

    #[test]
    fn saving_no_entries_removes_the_file() {
        let path = temporary_file("emptied.bin");
        let mut history = read_history(&path, Box::new(FakeVault::default()));
        history.save(&entries());

        history.save(&[]);

        assert!(!path.exists());
    }

    #[test]
    fn each_save_uses_a_new_nonce() {
        let path = temporary_file("nonce.bin");
        let mut history = read_history(&path, Box::new(FakeVault::default()));
        history.save(&entries());
        let first = fs::read(&path).expect("the file exists");

        history.save(&entries());

        assert_ne!(fs::read(&path).expect("the file exists"), first);
    }

    #[test]
    fn key_comes_from_the_vault_one_time() {
        let path = temporary_file("one-key.bin");
        let vault = FakeVault::default();
        let mut history = read_history(&path, Box::new(vault.clone()));

        history.save(&entries());
        history.save(&entries());
        history.load();

        assert_eq!(*vault.loads.borrow(), 1);
    }
}
