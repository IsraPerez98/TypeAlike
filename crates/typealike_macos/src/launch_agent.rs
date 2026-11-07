//! Starts TypeAlike at login with a launchd agent in the user's `LaunchAgents` folder.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use typealike_core::settings::Autostart;

const LABEL: &str = "io.github.israperez98.typealike";

/// The launchd agent file that starts a program when the user logs in.
#[derive(Debug)]
pub struct LaunchAgent {
    file: PathBuf,
    program: PathBuf,
}

impl LaunchAgent {
    /// Creates the agent in `agents_folder` that starts `program`.
    #[must_use]
    pub fn new(agents_folder: &Path, program: &Path) -> Self {
        Self {
            file: agents_folder.join(format!("{LABEL}.plist")),
            program: program.to_owned(),
        }
    }

    fn write(&self) -> io::Result<()> {
        if let Some(folder) = self.file.parent() {
            fs::create_dir_all(folder)?;
        }
        fs::write(&self.file, property_list(&self.program))
    }

    fn remove(&self) -> io::Result<()> {
        match fs::remove_file(&self.file) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }
}

impl Autostart for LaunchAgent {
    /// The agent counts only when it starts this program, and the program is still at its path.
    fn is_enabled(&self) -> bool {
        self.program.is_file()
            && fs::read_to_string(&self.file).is_ok_and(|written| written == property_list(&self.program))
    }

    fn enable(&mut self) {
        if let Err(error) = self.write() {
            tracing::warn!(%error, "failed to add the launch agent");
        }
    }

    fn disable(&mut self) {
        if let Err(error) = self.remove() {
            tracing::warn!(%error, "failed to remove the launch agent");
        }
    }
}

fn property_list(program: &Path) -> String {
    let program = escape_xml(&program.to_string_lossy());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{program}</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
</dict>
</plist>
"#
    )
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agents_folder(test: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("typealike-launch-agent-{}", std::process::id()))
            .join(test)
    }

    fn agent_file(folder: &Path) -> PathBuf {
        folder.join("io.github.israperez98.typealike.plist")
    }

    fn existing_program(folder: &Path, name: &str) -> PathBuf {
        let program = folder.join(name);
        fs::create_dir_all(folder).expect("the temp folder is writable");
        fs::write(&program, "").expect("the temp folder is writable");
        program
    }

    #[test]
    fn enabling_writes_an_agent_that_starts_the_program_at_login() {
        let folder = agents_folder("enable");
        let mut agent = LaunchAgent::new(&folder, Path::new("/Applications/TypeAlike"));

        agent.enable();

        let written = fs::read_to_string(agent_file(&folder)).ok();
        fs::remove_dir_all(&folder).ok();
        assert_eq!(
            written.as_deref(),
            Some(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>io.github.israperez98.typealike</string>
	<key>ProgramArguments</key>
	<array>
		<string>/Applications/TypeAlike</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
</dict>
</plist>
"#
            )
        );
    }

    #[test]
    fn agent_of_the_running_program_is_enabled() {
        let folder = agents_folder("running-program");
        let mut agent = LaunchAgent::new(&folder, &existing_program(&folder, "TypeAlike"));
        agent.enable();

        let is_enabled = agent.is_enabled();

        fs::remove_dir_all(&folder).ok();
        assert!(is_enabled);
    }

    #[test]
    fn agent_that_starts_another_program_is_not_enabled() {
        let folder = agents_folder("other-program");
        LaunchAgent::new(&folder, &existing_program(&folder, "Other")).enable();
        let agent = LaunchAgent::new(&folder, &existing_program(&folder, "TypeAlike"));

        let is_enabled = agent.is_enabled();

        fs::remove_dir_all(&folder).ok();
        assert!(!is_enabled);
    }

    #[test]
    fn disabling_removes_the_agent() {
        let folder = agents_folder("disable");
        let mut agent = LaunchAgent::new(&folder, Path::new("/Applications/TypeAlike"));
        agent.enable();

        agent.disable();

        let is_file_left = agent_file(&folder).exists();
        fs::remove_dir_all(&folder).ok();
        assert_eq!((agent.is_enabled(), is_file_left), (false, false));
    }

    #[test]
    fn disabling_a_missing_agent_keeps_it_disabled() {
        let mut agent = LaunchAgent::new(&agents_folder("missing"), Path::new("/Applications/TypeAlike"));

        agent.disable();

        assert!(!agent.is_enabled());
    }

    #[test]
    fn agent_of_a_missing_program_is_not_enabled() {
        let folder = agents_folder("missing-program");
        let mut agent = LaunchAgent::new(&folder, &folder.join("TypeAlike"));
        agent.enable();

        let is_enabled = agent.is_enabled();

        fs::remove_dir_all(&folder).ok();
        assert!(
            !is_enabled,
            "the agent counts as enabled while its program is missing"
        );
    }

    #[test]
    fn program_path_is_escaped_for_xml() {
        let folder = agents_folder("escape");
        let mut agent = LaunchAgent::new(&folder, Path::new("/Users/a&b/<TypeAlike>"));

        agent.enable();

        let written = fs::read_to_string(agent_file(&folder)).unwrap_or_default();
        fs::remove_dir_all(&folder).ok();
        assert!(written.contains("<string>/Users/a&amp;b/&lt;TypeAlike&gt;</string>"));
    }
}
