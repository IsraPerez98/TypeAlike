//! The password managers and the terminals that GhostType knows by their app id.

/// Whether the app with `app_id` is a password manager, which starts off in the list of the
/// Screen text pane. Its id matches by its start in lowercase.
pub(super) fn is_password_manager(app_id: &str) -> bool {
    let app_id = app_id.to_lowercase();
    PASSWORD_MANAGERS.iter().any(|start| app_id.starts_with(start))
}

/// The password managers that GhostType knows, by the start of their app id in lowercase: the
/// bundle identifier on macOS and the program file name on Windows.
const PASSWORD_MANAGERS: [&str; 24] = [
    "com.1password.",
    "com.agilebits.onepassword",
    "com.bitwarden.desktop",
    "org.keepassxc.keepassxc",
    "com.lastpass.",
    "com.dashlane.",
    "com.apple.keychainaccess",
    "com.apple.passwords",
    "in.sinew.enpass",
    "com.nordpass.",
    "me.proton.pass",
    "com.callpod.keepermac",
    "com.siber.roboform",
    "1password.exe",
    "bitwarden.exe",
    "keepassxc.exe",
    "keepass.exe",
    "enpass.exe",
    "nordpass.exe",
    "protonpass.exe",
    "dashlane.exe",
    "lastpass.exe",
    "keeperpasswordmanager.exe",
    "roboform",
];

/// Whether the app with `app_id` is a terminal, whose text GhostType does not save unless the
/// user turns it on. Its id matches by its start in lowercase.
pub(super) fn is_terminal(app_id: &str) -> bool {
    let app_id = app_id.to_lowercase();
    TERMINALS.iter().any(|start| app_id.starts_with(start))
}

/// The terminals that GhostType knows, by the start of their app id in lowercase: the bundle
/// identifier on macOS and the program file name on Windows.
const TERMINALS: [&str; 25] = [
    "com.apple.terminal",
    "com.googlecode.iterm2",
    "com.mitchellh.ghostty",
    "dev.warp.",
    "net.kovidgoyal.kitty",
    "io.alacritty",
    "org.alacritty",
    "com.github.wez.wezterm",
    "co.zeit.hyper",
    "com.raphaelamorim.rio",
    "org.tabby",
    "windowsterminal.exe",
    "openconsole.exe",
    "conhost.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "wezterm-gui.exe",
    "alacritty.exe",
    "warp.exe",
    "mintty.exe",
    "tabby.exe",
    "hyper.exe",
    "putty.exe",
    "rio.exe",
];
