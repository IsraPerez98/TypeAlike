//! Embeds the Windows application manifest. It selects version 6 of the common controls, so the
//! tray menu and dialogs draw with the current visual styles. It also declares Windows 10 and 11
//! support, so Windows runs GhostType without the compatibility mode for older apps. It sets the
//! UTF-8 code page, so llama.cpp can write a model file in a folder with letters outside ASCII.

fn main() {
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
    }
}
