//! The apps of the settings: the password managers, the terminals and the database tools that
//! TypeAlike knows by their app id, and the order of the app lists.

/// Adds `app` to `apps` in the alphabetical order of the names that `app_name` gives, unless
/// `apps` has an app with the id that `app_id` gives. Returns whether `apps` changed.
pub(super) fn insert_in_name_order<App: Clone>(
    apps: &mut Vec<App>,
    app: &App,
    app_id: fn(&App) -> &str,
    app_name: fn(&App) -> &str,
) -> bool {
    if apps.iter().any(|known| app_id(known) == app_id(app)) {
        return false;
    }
    let name = app_name(app).to_lowercase();
    let place = apps.partition_point(|known| app_name(known).to_lowercase() <= name);
    apps.insert(place, app.clone());
    true
}

/// Whether the app with `app_id` is a password manager, which starts off in the list of the
/// Copied and screen text pane. Its id matches by its start in lowercase.
pub(super) fn is_password_manager(app_id: &str) -> bool {
    starts_with_any(app_id, &PASSWORD_MANAGERS)
}

/// The password managers that TypeAlike knows, by the start of their app id in lowercase: the
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

/// Whether the app with `app_id` is a terminal, whose text TypeAlike does not save unless the
/// user turns it on. Its id matches by its start in lowercase.
pub(super) fn is_terminal(app_id: &str) -> bool {
    starts_with_any(app_id, &TERMINALS)
}

fn starts_with_any(app_id: &str, starts: &[&str]) -> bool {
    let app_id = app_id.to_lowercase();
    starts.iter().any(|start| app_id.starts_with(start))
}

/// The terminals that TypeAlike knows, by the start of their app id in lowercase: the bundle
/// identifier on macOS and the program file name on Windows.
const TERMINALS: [&str; 27] = [
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
    "com.termius",
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
    "termius.exe",
];

/// Whether the app with `app_id` is a database tool, whose queries can hold data and secrets.
/// TypeAlike does not save its text unless the user turns it on. Its id matches by its start in
/// lowercase.
pub(super) fn is_database_tool(app_id: &str) -> bool {
    starts_with_any(app_id, &DATABASE_TOOLS)
}

/// The database tools that TypeAlike knows, by the start of their app id in lowercase: the
/// bundle identifier on macOS and the program file name on Windows.
const DATABASE_TOOLS: [&str; 26] = [
    "org.jkiss.dbeaver",
    "com.dbeaver.",
    "com.tinyapp.tableplus",
    "at.eggerapps.postico",
    "com.sequel-ace.sequel-ace",
    "com.jetbrains.datagrip",
    "org.pgadmin.pgadmin4",
    "com.oracle.workbench.mysqlworkbench",
    "io.beekeeperstudio.desktop",
    "com.mongodb.compass",
    "org.redislabs.redisinsight",
    "com.azuredatastudio.oss",
    "com.navicat.",
    "com.prect.navicat",
    "dbeaver.exe",
    "tableplus.exe",
    "datagrip64.exe",
    "pgadmin4.exe",
    "mysqlworkbench.exe",
    "beekeeper studio.exe",
    "mongodbcompass.exe",
    "redis insight.exe",
    "azuredatastudio.exe",
    "ssms.exe",
    "heidisql.exe",
    "navicat.exe",
];
