//! Puts the llama.cpp DLLs of a GPU build next to the programs that load them.
//!
//! The `vulkan` and `cuda` features build llama.cpp as DLLs. On MSVC, llama-cpp-sys-2 0.1.159
//! does not copy them to the target folder, because it looks for `.lib` files in their folder.
//! Without the copies, Windows loads another `llama.dll` from `PATH`, or none, and the program
//! stops before `main`.
//!
//! Each backend DLL goes into the folder of its backend in `backends` next to the bin:
//! `backends\cpu`, `backends\vulkan` and `backends\cuda`. The model worker loads only the folders
//! that a load needs. Thus a target folder also runs on another computer, as the installers do.

use std::path::{Path, PathBuf};
use std::{env, fs};

/// The folder next to the bin with a folder for each backend. The model worker reads the same
/// folders.
const BACKENDS_FOLDER: &str = "backends";

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
    for dll in dlls_in(&libraries) {
        copy_dll(&dll, profile_dir);
        copy_dll(&dll, &profile_dir.join("deps"));
    }
    for dll in dlls_in(&backends) {
        copy_backend_dll(&dll, &profile_dir.join(BACKENDS_FOLDER));
    }
}

fn dlls_in(source: &Path) -> Vec<PathBuf> {
    let entries = match fs::read_dir(source) {
        Ok(entries) => entries,
        Err(error) => {
            println!(
                "cargo::warning=cannot read the llama.cpp DLLs in {}: {error}",
                source.display()
            );
            return Vec::new();
        }
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
        })
        .collect()
}

/// Copies the backend `dll` into the folder of its backend in `backends`.
fn copy_backend_dll(dll: &Path, backends: &Path) {
    let Some(backend) = backend_of(dll) else {
        println!("cargo::warning={} is not a llama.cpp backend DLL", dll.display());
        return;
    };
    let folder = backends.join(backend);
    if let Err(error) = fs::create_dir_all(&folder) {
        println!(
            "cargo::warning=cannot make the folder {}: {error}",
            folder.display()
        );
        return;
    }
    copy_dll(dll, &folder);
}

/// Returns the backend of a backend DLL, as llama.cpp finds it in the name: `cpu` for
/// `ggml-cpu-haswell.dll`, and `cuda` for `ggml-cuda.dll`.
fn backend_of(dll: &Path) -> Option<&str> {
    let name = dll.file_stem()?.to_str()?.strip_prefix("ggml-")?;
    name.split('-').next()
}

fn copy_dll(dll: &Path, destination: &Path) {
    let name = dll.file_name().expect("a directory entry has a name");
    if let Err(error) = fs::copy(dll, destination.join(name)) {
        println!(
            "cargo::warning=cannot copy {} to {}: {error}",
            dll.display(),
            destination.display()
        );
    }
}
