//! The install on macOS: the app from the disk image replaces the app bundle that runs, and a shell
//! opens the new app after TypeAlike quits.

use std::fs;
use std::io;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const APP_NAME: &str = "TypeAlike.app";
const HDIUTIL: &str = "/usr/bin/hdiutil";
const DITTO: &str = "/usr/bin/ditto";
/// Waits until the process of the first argument ends, then opens the app of the second argument.
const OPEN_AFTER_EXIT: &str =
    r#"while /bin/kill -0 "$0" 2>/dev/null; do /bin/sleep 0.2; done; /usr/bin/open "$1""#;

/// The app bundle that runs this program, when the program runs from a bundle.
pub(crate) fn installed_copy() -> Option<PathBuf> {
    let program = std::env::current_exe().ok()?.canonicalize().ok()?;
    bundle_of(&program)
}

fn bundle_of(program: &Path) -> Option<PathBuf> {
    let macos_folder = program
        .parent()
        .filter(|folder| folder.ends_with("Contents/MacOS"))?;
    let bundle = macos_folder.parent()?.parent()?;
    let is_app = bundle.extension().is_some_and(|extension| extension == "app");
    is_app.then(|| bundle.to_path_buf())
}

/// Copies the app out of the disk image `download` into `folder`, and returns the copy.
pub(crate) fn prepare(download: &Path, folder: &Path) -> io::Result<PathBuf> {
    let mount = folder.join("mount");
    fs::create_dir_all(&mount)?;
    run(Command::new(HDIUTIL)
        .args(["attach", "-nobrowse", "-readonly", "-noautoopen", "-mountpoint"])
        .arg(&mount)
        .arg(download))?;
    let prepared = folder.join("new").join(APP_NAME);
    let copy = run(Command::new(DITTO).arg(mount.join(APP_NAME)).arg(&prepared));
    if let Err(error) = run(Command::new(HDIUTIL).args(["detach", "-force"]).arg(&mount)) {
        tracing::warn!(%error, "failed to detach the disk image of the update");
    }
    copy?;
    fs::remove_file(download)?;
    Ok(prepared)
}

/// Puts the `prepared` app in the place of the `installed` bundle, and starts a shell that opens
/// it after TypeAlike quits. The old bundle goes to `folder`, which the next start removes.
pub(crate) fn start_install(prepared: &Path, installed: &Path, folder: &Path) -> io::Result<()> {
    let old = folder.join("old").join(APP_NAME);
    if let Some(old_folder) = old.parent() {
        fs::create_dir_all(old_folder)?;
    }
    fs::rename(installed, &old)?;
    if let Err(error) = fs::rename(prepared, installed) {
        fs::rename(&old, installed)?;
        return Err(error);
    }
    // launchd ends the process group of a launch agent when it quits, so the shell needs its own.
    Command::new("/bin/sh")
        .arg("-c")
        .arg(OPEN_AFTER_EXIT)
        .arg(std::process::id().to_string())
        .arg(installed)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()?;
    Ok(())
}

/// Runs `command` to its end, and fails with its error output when it fails.
fn run(command: &mut Command) -> io::Result<()> {
    let output = command.stdin(Stdio::null()).output()?;
    if output.status.success() {
        return Ok(());
    }
    let program = command.get_program().to_string_lossy().into_owned();
    let error_output = String::from_utf8_lossy(&output.stderr);
    Err(io::Error::other(format!(
        "{program} failed with {}: {}",
        output.status,
        error_output.trim()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_in_an_app_bundle_has_its_bundle() {
        let program = Path::new("/Applications/TypeAlike.app/Contents/MacOS/TypeAlike");

        assert_eq!(
            bundle_of(program),
            Some(PathBuf::from("/Applications/TypeAlike.app"))
        );
    }

    #[test]
    fn program_outside_an_app_bundle_has_no_bundle() {
        let programs = [
            "/Users/dev/TypeAlike/target/debug/TypeAlike",
            "/Applications/TypeAlike/Contents/MacOS/TypeAlike",
            "/Applications/TypeAlike.app/Contents/Resources/TypeAlike",
        ];

        let bundles = programs.map(|program| bundle_of(Path::new(program)));

        assert_eq!(bundles, [None, None, None]);
    }
}
