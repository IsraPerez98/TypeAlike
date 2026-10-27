//! Where downloaded model files live: `<models dir>/<owner>/<repo>/<file>.gguf`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use typealike_core::hub::{ByteSize, ModelFile, SizedFile};

const MODEL_EXTENSION: &str = "gguf";
const PARTIAL_SUFFIX: &str = ".part";
const NAME_PUNCTUATION: [char; 3] = ['.', '_', '-'];

/// Returns where `file` is stored, or `None` when its repository or name could escape the models
/// directory.
pub(crate) fn file_path(models_dir: &Path, file: &ModelFile) -> Option<PathBuf> {
    let (owner, repo) = split_repo_id(&file.repo)?;
    is_safe_name(&file.name).then(|| models_dir.join(owner).join(repo).join(&file.name))
}

/// Returns where a download writes `destination` until it completes.
pub(crate) fn partial_path(destination: &Path) -> PathBuf {
    let mut partial = destination.as_os_str().to_owned();
    partial.push(PARTIAL_SUFFIX);
    PathBuf::from(partial)
}

/// Whether `repo` is an `owner/name` id that is safe to put in a path or URL.
pub(crate) fn is_safe_repo_id(repo: &str) -> bool {
    split_repo_id(repo).is_some()
}

/// Whether `name` is one plain path component made of letters, digits, `.`, `_` and `-`.
pub(crate) fn is_safe_name(name: &str) -> bool {
    let is_allowed =
        |character: char| character.is_ascii_alphanumeric() || NAME_PUNCTUATION.contains(&character);
    !matches!(name, "" | "." | "..") && name.chars().all(is_allowed)
}

fn split_repo_id(repo: &str) -> Option<(&str, &str)> {
    repo.split_once('/')
        .filter(|(owner, name)| is_safe_name(owner) && is_safe_name(name))
}

/// Lists every complete model file under `models_dir`.
pub(crate) fn installed_files(models_dir: &Path) -> io::Result<Vec<SizedFile>> {
    let mut installed = Vec::new();
    for owner_dir in subdirectories(models_dir)? {
        for repo_dir in subdirectories(&owner_dir)? {
            installed.extend(installed_in_repo(&repo_dir)?);
        }
    }
    Ok(installed)
}

fn installed_in_repo(repo_dir: &Path) -> io::Result<Vec<SizedFile>> {
    fs::read_dir(repo_dir)?
        .map(|entry| installed_file(&entry?.path()))
        .filter_map(Result::transpose)
        .collect()
}

fn subdirectories(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(dir) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        entries => entries?,
    };
    let paths = entries
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    Ok(paths.into_iter().filter(|path| path.is_dir()).collect())
}

fn installed_file(path: &Path) -> io::Result<Option<SizedFile>> {
    let is_model = path
        .extension()
        .is_some_and(|extension| extension == MODEL_EXTENSION);
    let repo_dir = path.parent();
    let owner_dir = repo_dir.and_then(Path::parent);
    let (true, Some(name), Some(repo), Some(owner)) = (
        is_model,
        utf8_name(Some(path)),
        utf8_name(repo_dir),
        utf8_name(owner_dir),
    ) else {
        return Ok(None);
    };
    let metadata = fs::metadata(path)?;
    let file = ModelFile {
        repo: format!("{owner}/{repo}"),
        name: name.to_owned(),
    };
    Ok(metadata.is_file().then_some(SizedFile {
        file,
        size: ByteSize(metadata.len()),
    }))
}

fn utf8_name(path: Option<&Path>) -> Option<&str> {
    path?.file_name()?.to_str()
}

/// Deletes the stored `file`. Deleting a file that is already gone succeeds.
pub(crate) fn delete(models_dir: &Path, file: &ModelFile) -> io::Result<()> {
    let path = file_path(models_dir, file).ok_or(io::ErrorKind::InvalidInput)?;
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_file(repo: &str, name: &str) -> ModelFile {
        ModelFile {
            repo: repo.to_owned(),
            name: name.to_owned(),
        }
    }

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(test_name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("typealike-store-{}-{test_name}", std::process::id()));
            fs::remove_dir_all(&path).ok();
            fs::create_dir_all(&path).expect("temp dir is writable");
            Self(path)
        }

        fn write(&self, relative_path: &str, contents: &[u8]) {
            let path = self.0.join(relative_path);
            fs::create_dir_all(path.parent().expect("file has a parent")).expect("temp dir is writable");
            fs::write(path, contents).expect("temp dir is writable");
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).ok();
        }
    }

    #[test]
    fn file_is_stored_under_owner_and_repo() {
        let path = file_path(Path::new("/models"), &model_file("Qwen/Qwen3-GGUF", "q4.gguf"));

        assert_eq!(path, Some(PathBuf::from("/models/Qwen/Qwen3-GGUF/q4.gguf")));
    }

    #[test]
    fn names_that_could_escape_the_models_dir_have_no_path() {
        let unsafe_files = [
            model_file("../etc", "q4.gguf"),
            model_file("owner/..", "q4.gguf"),
            model_file("owner/repo/extra", "q4.gguf"),
            model_file("owner", "q4.gguf"),
            model_file("owner/repo", "../q4.gguf"),
            model_file("owner/repo", "sub/q4.gguf"),
            model_file("owner/repo", ""),
            model_file("owner/repo", "q4 copy.gguf"),
        ];

        let paths = unsafe_files.map(|file| file_path(Path::new("/models"), &file));

        assert_eq!(paths, [None, None, None, None, None, None, None, None]);
    }

    #[test]
    fn partial_download_sits_beside_destination() {
        let partial = partial_path(Path::new("/models/o/r/q4.gguf"));

        assert_eq!(partial, PathBuf::from("/models/o/r/q4.gguf.part"));
    }

    #[test]
    fn missing_models_dir_has_no_files() {
        let dir = TestDir::new("missing");

        let installed = installed_files(&dir.0.join("absent")).expect("missing dir is not an error");

        assert_eq!(installed, Vec::new());
    }

    #[test]
    fn only_complete_model_files_are_installed() {
        let dir = TestDir::new("installed");
        dir.write("owner/repo/q4.gguf", b"GGUF");
        dir.write("owner/repo/q8.gguf.part", b"GG");
        dir.write("owner/repo/README.md", b"#");
        dir.write("owner/stray.gguf", b"GGUF");

        let installed = installed_files(&dir.0).expect("dir is readable");

        assert_eq!(
            installed,
            [SizedFile {
                file: model_file("owner/repo", "q4.gguf"),
                size: ByteSize(4),
            }]
        );
    }

    #[test]
    fn delete_removes_the_file() {
        let dir = TestDir::new("delete");
        dir.write("owner/repo/q4.gguf", b"GGUF");

        delete(&dir.0, &model_file("owner/repo", "q4.gguf")).expect("file is deletable");

        assert!(!dir.0.join("owner/repo/q4.gguf").exists());
    }

    #[test]
    fn deleting_a_missing_file_succeeds() {
        let dir = TestDir::new("delete-missing");

        let result = delete(&dir.0, &model_file("owner/repo", "q4.gguf"));

        assert!(result.is_ok());
    }
}
