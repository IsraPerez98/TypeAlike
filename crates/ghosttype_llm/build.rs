//! Puts the llama.cpp DLLs of a GPU build next to the programs that load them.
//!
//! The `vulkan` and `cuda` features build llama.cpp as DLLs. On MSVC, llama-cpp-sys-2 0.1.159
//! does not copy them to the target folder, because it looks for `.lib` files in their folder.
//! Without the copies, Windows loads another `llama.dll` from `PATH`, or none, and the program
//! stops before `main`. The GPU backends go next to the programs too, so that a target folder
//! runs on another computer.

use std::path::{Path, PathBuf};
use std::{env, fs};

fn main() {
    println!("cargo::rerun-if-env-changed=DEP_LLAMA_BACKENDS_DIR");
    let is_windows_msvc = env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows")
        && env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|target_env| target_env == "msvc");
    let Some(backends) = env::var_os("DEP_LLAMA_BACKENDS_DIR").map(PathBuf::from) else {
        return;
    };
    if !is_windows_msvc {
        return;
    }
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets the output folder"));
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("the output folder is target/<profile>/build/<package>/out");
    let libraries = backends
        .parent()
        .expect("the backends folder is in the llama.cpp output folder")
        .join("bin");
    copy_dlls(&libraries, &[profile_dir.to_path_buf(), profile_dir.join("deps")]);
    copy_dlls(&backends, &[profile_dir.to_path_buf()]);
}

fn copy_dlls(source: &Path, destinations: &[PathBuf]) {
    let entries = match fs::read_dir(source) {
        Ok(entries) => entries,
        Err(error) => {
            println!(
                "cargo::warning=cannot read the llama.cpp DLLs in {}: {error}",
                source.display()
            );
            return;
        }
    };
    let dlls = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
        });
    for dll in dlls {
        let name = dll.file_name().expect("a directory entry has a name");
        for destination in destinations {
            if let Err(error) = fs::copy(&dll, destination.join(name)) {
                println!(
                    "cargo::warning=cannot copy {} to {}: {error}",
                    dll.display(),
                    destination.display()
                );
            }
        }
    }
}
