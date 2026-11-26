//! Embeds the Windows application manifest. It selects version 6 of the common controls, so the
//! tray menu and dialogs draw with the current visual styles. It also declares Windows 10 and 11
//! support, so Windows runs TypeAlike without the compatibility mode for older apps.
//! It also embeds the app icon (`TypeAlike.rc`), which File Explorer, the Start menu and the
//! windows of TypeAlike show.
//! With the `self-contained` feature, it also copies the Windows App Runtime next to the program
//! and adds the manifest that tells windows-reactor to load that copy. It also delay-loads the
//! dynamic dependency API of Windows 11, so that the program starts on Windows 10.
//! On each OS, it gives the program the git commit of the source (`TYPEALIKE_COMMIT`), which the
//! log shows when TypeAlike starts.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The number of hex digits of the commit in the release workflow, the length of a short hash.
const SHORT_COMMIT_DIGITS: usize = 7;
const UNKNOWN_COMMIT: &str = "unknown";

fn main() {
    embed_commit();
    let is_windows_msvc = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|env| env == "msvc");
    if is_windows_msvc {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets the manifest directory");
        let compatibility = std::path::Path::new(&manifest_dir).join("compatibility.manifest");
        println!("cargo::rerun-if-changed=compatibility.manifest");
        println!("cargo::rustc-link-arg-bins=/MANIFEST:EMBED");
        println!(
            "cargo::rustc-link-arg-bins=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
        println!(
            "cargo::rustc-link-arg-bins=/MANIFESTINPUT:{}",
            compatibility.display()
        );
        println!("cargo::rerun-if-changed=TypeAlike.rc");
        println!("cargo::rerun-if-changed=TypeAlike.ico");
        embed_resource::compile("TypeAlike.rc", embed_resource::NONE)
            .manifest_required()
            .expect("the resource compiler of the Windows SDK embeds the app icon");
        #[cfg(feature = "self-contained")]
        {
            windows_reactor_setup::as_self_contained();
            delay_load_package_dependency_api();
        }
    }
}

/// Loads the dynamic dependency API (`AddPackageDependency`) only at its first call, so that
/// TypeAlike also starts on Windows 10. Only Windows 11 has this API. windows-reactor imports it,
/// but calls it only when no self-contained runtime is next to the program.
#[cfg(feature = "self-contained")]
fn delay_load_package_dependency_api() {
    println!("cargo::rustc-link-arg-bins=delayimp.lib");
    println!("cargo::rustc-link-arg-bins=/DELAYLOAD:api-ms-win-appmodel-runtime-l1-1-5.dll");
}

/// Gives the program the git commit of the source as `TYPEALIKE_COMMIT`: the start of
/// `GITHUB_SHA` in a workflow, else the short hash of `HEAD`, else `unknown`. The script runs
/// again when `HEAD` or the branch of `HEAD` moves.
fn embed_commit() {
    println!("cargo::rerun-if-env-changed=GITHUB_SHA");
    let commit = std::env::var("GITHUB_SHA")
        .ok()
        .filter(|sha| sha.len() >= SHORT_COMMIT_DIGITS && is_hex(sha))
        .map(|sha| sha[..SHORT_COMMIT_DIGITS].to_owned())
        .or_else(|| git(&["rev-parse", "--short", "HEAD"]).filter(|hash| is_hex(hash)))
        .unwrap_or_else(|| UNKNOWN_COMMIT.to_owned());
    for path in head_files() {
        println!("cargo::rerun-if-changed={}", path.display());
    }
    println!("cargo::rustc-env=TYPEALIKE_COMMIT={commit}");
}

fn is_hex(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|character| character.is_ascii_hexdigit())
}

/// The files that change when `HEAD` moves: `HEAD`, and the file of its branch when the branch has
/// one, else the packed refs and the folder of the branch. Only files that exist, because cargo
/// runs the script at each build for a path that does not exist.
fn head_files() -> Vec<PathBuf> {
    let Some(head) = git_path("HEAD") else {
        return Vec::new();
    };
    let branch = std::fs::read_to_string(&head)
        .ok()
        .and_then(|text| text.trim().strip_prefix("ref: ").map(str::to_owned))
        .and_then(|reference| git_path(&reference));
    let branch_files = match branch {
        Some(branch) if branch.is_file() => vec![branch],
        Some(branch) => [git_path("packed-refs"), branch.parent().map(Path::to_path_buf)]
            .into_iter()
            .flatten()
            .collect(),
        None => Vec::new(),
    };
    std::iter::once(head)
        .chain(branch_files)
        .filter(|path| path.exists())
        .collect()
}

/// The absolute path of `name` in the git folder of the source, such as the folder of a worktree.
fn git_path(name: &str) -> Option<PathBuf> {
    git(&["rev-parse", "--path-format=absolute", "--git-path", name]).map(PathBuf::from)
}

/// The trimmed output of git with `arguments` in the folder of this crate, or `None` when git
/// fails or is not installed.
fn git(arguments: &[&str]) -> Option<String> {
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")?;
    let output = Command::new("git")
        .args(arguments)
        .current_dir(manifest_dir)
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim().to_owned()).filter(|text| !text.is_empty())
}
