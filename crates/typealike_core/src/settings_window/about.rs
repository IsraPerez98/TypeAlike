//! The About pane of the settings window: the version of TypeAlike, the check for a newer version,
//! and the licenses of the third-party software of the build.

use crate::localization::{MessageArgument, Translations};
use crate::release::{ReleaseReadFailed, ReleaseTag, Version};
use crate::settings_window::view::{
    Accessory, Button, ControlId, Item, Output, Page, PageIcon, Row, Section, SettingsInput,
};

pub(crate) const CHECK_FOR_UPDATES: &str = "about-check-for-updates";
pub(crate) const DOWNLOAD_UPDATE: &str = "about-download-update";
const SOURCE_CODE: &str = "about-source-code";
const BACK_TO_LIBRARIES: &str = "about-back-to-libraries";
const LICENSE_PREFIX: &str = "about-license-";
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const NEWEST_RELEASE_PATH: &str = "/releases/latest";

/// A third-party library of the build and its license.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Library {
    /// The name, as its authors write it.
    pub name: &'static str,
    /// The version, or an empty text when the library has no version.
    pub version: &'static str,
    /// The license, as an SPDX expression such as `MIT OR Apache-2.0`.
    pub license: &'static str,
    /// The text of each license file of the library.
    pub texts: &'static [&'static str],
}

/// Something the user did in the About pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AboutAction {
    /// Clicked the button that checks for a newer version.
    CheckForUpdates,
    /// Clicked the button that downloads the newer version.
    DownloadUpdate,
    /// Clicked the link to the source code of TypeAlike.
    ShowSourceCode,
    /// Clicked the button of the library at this index, which shows its license.
    ShowLicense(usize),
    /// Clicked the button that goes back from a license to the list of the libraries.
    BackToLibraries,
}

/// What the controller does for an action of the About pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AboutRequest {
    /// Reads the newest release, and gives it to [`AboutPane::on_newest_release`].
    ReadNewestRelease,
    /// Opens this address in the web browser.
    OpenWebPage(String),
}

/// How far the check for a newer version got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateCheck {
    NotStarted,
    Running,
    UpToDate,
    Available(Version),
    Failed,
}

/// State of the About pane: the check for a newer version, and the license that the pane shows.
///
/// ```
/// use typealike_core::settings_window::about::{AboutAction, AboutPane, AboutRequest};
///
/// let mut pane = AboutPane::new(&[]);
///
/// let request = pane.on_action(AboutAction::CheckForUpdates);
///
/// assert_eq!(request, Some(AboutRequest::ReadNewestRelease));
/// ```
#[derive(Debug)]
pub struct AboutPane {
    libraries: &'static [Library],
    update_check: UpdateCheck,
    shown_license: Option<usize>,
}

impl AboutPane {
    /// Creates a pane that lists `libraries` in their order, before a check for a newer version.
    #[must_use]
    pub fn new(libraries: &'static [Library]) -> Self {
        Self {
            libraries,
            update_check: UpdateCheck::NotStarted,
            shown_license: None,
        }
    }

    /// Returns the action of `input`, when `input` is for a control of the pane.
    #[must_use]
    pub fn action_for(&self, input: &SettingsInput) -> Option<AboutAction> {
        let SettingsInput::Pressed(id) = input else {
            return None;
        };
        match id.as_str() {
            CHECK_FOR_UPDATES => Some(AboutAction::CheckForUpdates),
            DOWNLOAD_UPDATE => Some(AboutAction::DownloadUpdate),
            SOURCE_CODE => Some(AboutAction::ShowSourceCode),
            BACK_TO_LIBRARIES => Some(AboutAction::BackToLibraries),
            other => other
                .strip_prefix(LICENSE_PREFIX)
                .and_then(|index| index.parse().ok())
                .filter(|&index| index < self.libraries.len())
                .map(AboutAction::ShowLicense),
        }
    }

    /// Handles a user action. Returns what the controller must do for it. A check that runs
    /// already does not start again.
    pub fn on_action(&mut self, action: AboutAction) -> Option<AboutRequest> {
        match action {
            AboutAction::CheckForUpdates if self.update_check == UpdateCheck::Running => None,
            AboutAction::CheckForUpdates => {
                self.update_check = UpdateCheck::Running;
                Some(AboutRequest::ReadNewestRelease)
            }
            AboutAction::DownloadUpdate => matches!(self.update_check, UpdateCheck::Available(_))
                .then(|| AboutRequest::OpenWebPage(format!("{REPOSITORY}{NEWEST_RELEASE_PATH}"))),
            AboutAction::ShowSourceCode => Some(AboutRequest::OpenWebPage(REPOSITORY.to_owned())),
            AboutAction::ShowLicense(index) => {
                self.shown_license = Some(index);
                None
            }
            AboutAction::BackToLibraries => {
                self.shown_license = None;
                None
            }
        }
    }

    /// Ends the running check with `release`, the answer to [`AboutRequest::ReadNewestRelease`].
    /// A tag that is not a version counts as a failed read.
    pub fn on_newest_release(&mut self, release: Result<ReleaseTag, ReleaseReadFailed>) {
        if self.update_check != UpdateCheck::Running {
            return;
        }
        let newest = release.ok().and_then(|tag| Version::parse(&tag.0));
        self.update_check = match newest {
            Some(newest) if newest > Version::of_build() => UpdateCheck::Available(newest),
            Some(_) => UpdateCheck::UpToDate,
            None => UpdateCheck::Failed,
        };
    }

    /// Returns the page that the pane shows now, with the texts of `translations`.
    #[must_use]
    pub fn page(&self, translations: &Translations) -> Page {
        let shown_library = self.shown_license.and_then(|index| self.libraries.get(index));
        let libraries = match shown_library {
            Some(library) => license_section(library, translations),
            None => self.libraries_section(translations),
        };
        Page {
            title: translations.text("about-title"),
            icon: PageIcon::Info,
            summary: translations.text("about-summary"),
            search: None,
            sections: vec![self.version_section(translations), libraries],
        }
    }

    fn version_section(&self, translations: &Translations) -> Section {
        let version = Version::of_build().to_string();
        let (subtitle, accessory) = self.update_status(translations);
        let version_item = Item {
            title: translations.text_with("about-version", &[("version", MessageArgument::Text(&version))]),
            subtitle,
            detail: String::new(),
            accessory,
        };
        Section {
            footer: Some(translations.text("about-footer")),
            ..Section::new(vec![
                Row::Item(version_item),
                Row::Link(button(SOURCE_CODE, "about-source-code", translations)),
            ])
        }
    }

    /// The line below the version and the button after it, for the state of the check.
    fn update_status(&self, translations: &Translations) -> (String, Accessory) {
        let check = button(CHECK_FOR_UPDATES, "about-check-for-updates", translations);
        match self.update_check {
            UpdateCheck::NotStarted => (String::new(), Accessory::Action(check)),
            UpdateCheck::Running => (
                translations.text("about-checking"),
                Accessory::Action(Button {
                    is_enabled: false,
                    ..check
                }),
            ),
            UpdateCheck::UpToDate => (translations.text("about-up-to-date"), Accessory::Action(check)),
            UpdateCheck::Available(newest) => (
                translations.text_with(
                    "about-update-available",
                    &[("version", MessageArgument::Text(&newest.to_string()))],
                ),
                Accessory::Download(button(DOWNLOAD_UPDATE, "about-download", translations)),
            ),
            UpdateCheck::Failed => (
                translations.text("about-check-failed"),
                Accessory::Failed {
                    retry: button(CHECK_FOR_UPDATES, "about-try-again", translations),
                },
            ),
        }
    }

    fn libraries_section(&self, translations: &Translations) -> Section {
        let rows = self
            .libraries
            .iter()
            .enumerate()
            .map(|(index, library)| {
                Row::Item(Item {
                    title: library.name.to_owned(),
                    subtitle: library.license.to_owned(),
                    detail: library.version.to_owned(),
                    accessory: Accessory::Open(Button {
                        id: ControlId::from(format!("{LICENSE_PREFIX}{index}")),
                        label: translations.text("about-show-license"),
                        is_enabled: true,
                    }),
                })
            })
            .collect();
        Section {
            heading: Some(translations.text("about-libraries-heading")),
            footer: Some(translations.text("about-libraries-footer")),
            ..Section::new(rows)
        }
    }
}

/// The section with the license texts of `library`, and a button back to the list.
fn license_section(library: &Library, translations: &Translations) -> Section {
    let rows = library
        .texts
        .iter()
        .map(|&text| {
            Row::Output(Output {
                text: text.to_owned(),
                detail: String::new(),
            })
        })
        .collect();
    Section {
        heading: Some(library.name.to_owned()),
        back: Some(button(BACK_TO_LIBRARIES, "about-back-to-libraries", translations)),
        ..Section::new(rows)
    }
}

fn button(id: &'static str, label: &str, translations: &Translations) -> Button {
    Button {
        id: ControlId::new(id),
        label: translations.text(label),
        is_enabled: true,
    }
}

#[cfg(test)]
mod tests;
