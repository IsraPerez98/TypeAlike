//! The About pane of the settings window: the version of TypeAlike, the check for a newer version
//! and its install, and the licenses of the third-party software of the build.

use crate::localization::{MessageArgument, Translations};
use crate::release::{
    DownloadedRelease, ReleaseDownloadFailed, ReleaseInstallFailed, ReleaseReadFailed, ReleaseTag,
    SelfUpdate, Version,
};
use crate::settings::Updates;
use crate::settings_window::view::{
    Accessory, Button, ControlId, Item, Output, Page, PageIcon, Row, RowIcon, Section, SettingsInput, Switch,
};

pub(crate) const CHECK_FOR_UPDATES: &str = "about-check-for-updates";
pub(crate) const DOWNLOAD_UPDATE: &str = "about-download-update";
pub(crate) const KEEP_UP_TO_DATE: &str = "about-keep-up-to-date";
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
    /// Turned the switch that keeps TypeAlike up to date on or off.
    SwitchUpdates(Updates),
}

/// What the controller does for an action of the About pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AboutRequest {
    /// Reads the newest release, and gives it to [`AboutPane::on_newest_release`].
    ReadNewestRelease,
    /// Downloads the release of this tag, and gives it to [`AboutPane::on_release_downloaded`].
    DownloadRelease(ReleaseTag),
    /// Opens this address in the web browser.
    OpenWebPage(String),
    /// Saves the settings with the [`Updates`] of the pane.
    SaveSettings,
}

/// How far the check for a newer version, and its install, got.
#[derive(Debug, Clone, PartialEq, Eq)]
enum UpdateCheck {
    NotStarted,
    Running,
    UpToDate,
    Available(Version),
    Downloading(Version),
    /// Waits until the install can start.
    Downloaded(Version, DownloadedRelease),
    Installing(Version),
    CheckFailed,
    InstallFailed(Version),
}

impl UpdateCheck {
    /// Whether a check, a download or an install runs, so that no other check can start.
    fn is_running(&self) -> bool {
        matches!(
            self,
            Self::Running | Self::Downloading(_) | Self::Downloaded(..) | Self::Installing(_)
        )
    }
}

/// State of the About pane: the check for a newer version and its install, and the license that
/// the pane shows.
///
/// ```
/// use typealike_core::release::SelfUpdate;
/// use typealike_core::settings::Updates;
/// use typealike_core::settings_window::about::{AboutAction, AboutPane, AboutRequest};
///
/// let mut pane = AboutPane::new(&[], Updates::Automatic, SelfUpdate::Supported);
///
/// let request = pane.on_action(AboutAction::CheckForUpdates);
///
/// assert_eq!(request, Some(AboutRequest::ReadNewestRelease));
/// ```
#[derive(Debug)]
pub struct AboutPane {
    libraries: &'static [Library],
    updates: Updates,
    self_update: SelfUpdate,
    update_check: UpdateCheck,
    shown_license: Option<usize>,
}

impl AboutPane {
    /// Creates a pane that lists `libraries` in their order, before a check for a newer version.
    /// With `updates` and a copy of TypeAlike that `self_update` supports, the pane installs each
    /// newer version that a check finds.
    #[must_use]
    pub fn new(libraries: &'static [Library], updates: Updates, self_update: SelfUpdate) -> Self {
        Self {
            libraries,
            updates,
            self_update,
            update_check: UpdateCheck::NotStarted,
            shown_license: None,
        }
    }

    /// Whether TypeAlike installs each new version by itself, as the switch of the pane shows.
    #[must_use]
    pub fn updates(&self) -> Updates {
        self.updates
    }

    fn installs_updates(&self) -> bool {
        self.updates == Updates::Automatic && self.self_update == SelfUpdate::Supported
    }

    /// Returns the action of `input`, when `input` is for a control of the pane.
    #[must_use]
    pub fn action_for(&self, input: &SettingsInput) -> Option<AboutAction> {
        let id = match input {
            SettingsInput::Toggled { id, is_on } if id.as_str() == KEEP_UP_TO_DATE => {
                let updates = if *is_on {
                    Updates::Automatic
                } else {
                    Updates::Manual
                };
                return Some(AboutAction::SwitchUpdates(updates));
            }
            SettingsInput::Pressed(id) => id,
            _ => return None,
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

    /// Handles a user action. Returns what the controller must do for it. A check, a download or
    /// an install that runs already blocks another check. When the user turns off the switch, a
    /// running download does not install.
    pub fn on_action(&mut self, action: AboutAction) -> Option<AboutRequest> {
        match action {
            AboutAction::CheckForUpdates if self.update_check.is_running() => None,
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
            AboutAction::SwitchUpdates(updates) => {
                self.updates = updates;
                if let (
                    Updates::Manual,
                    UpdateCheck::Downloading(newest) | UpdateCheck::Downloaded(newest, _),
                ) = (updates, &self.update_check)
                {
                    self.update_check = UpdateCheck::Available(*newest);
                }
                Some(AboutRequest::SaveSettings)
            }
        }
    }

    /// Starts the check of the day, while TypeAlike installs new versions by itself and no other
    /// check runs.
    pub fn on_scheduled_check(&mut self) -> Option<AboutRequest> {
        if !self.installs_updates() || self.update_check.is_running() {
            return None;
        }
        self.update_check = UpdateCheck::Running;
        Some(AboutRequest::ReadNewestRelease)
    }

    /// Ends the running check with `release`, the answer to [`AboutRequest::ReadNewestRelease`].
    /// A tag that is not a version counts as a failed read. While TypeAlike installs new versions
    /// by itself, it asks for the download of a newer version.
    pub fn on_newest_release(
        &mut self,
        release: Result<ReleaseTag, ReleaseReadFailed>,
    ) -> Option<AboutRequest> {
        if self.update_check != UpdateCheck::Running {
            return None;
        }
        let newest = release.ok().and_then(|tag| Version::parse(&tag.0));
        let (update_check, request) = match newest {
            Some(newest) if newest > Version::of_build() && self.installs_updates() => (
                UpdateCheck::Downloading(newest),
                Some(AboutRequest::DownloadRelease(newest.tag())),
            ),
            Some(newest) if newest > Version::of_build() => (UpdateCheck::Available(newest), None),
            Some(_) => (UpdateCheck::UpToDate, None),
            None => (UpdateCheck::CheckFailed, None),
        };
        self.update_check = update_check;
        request
    }

    /// Ends the running download with `release`, the answer to [`AboutRequest::DownloadRelease`].
    /// The release then waits for [`Self::release_to_install`].
    pub fn on_release_downloaded(&mut self, release: Result<DownloadedRelease, ReleaseDownloadFailed>) {
        let UpdateCheck::Downloading(newest) = self.update_check else {
            return;
        };
        self.update_check = match release {
            Ok(release) => UpdateCheck::Downloaded(newest, release),
            Err(ReleaseDownloadFailed) => UpdateCheck::InstallFailed(newest),
        };
    }

    /// The downloaded release that waits for its install.
    #[must_use]
    pub fn release_to_install(&self) -> Option<&DownloadedRelease> {
        match &self.update_check {
            UpdateCheck::Downloaded(_, release) => Some(release),
            _ => None,
        }
    }

    /// Ends the wait of the downloaded release with `install`, the start of its install.
    pub fn on_install(&mut self, install: Result<(), ReleaseInstallFailed>) {
        let UpdateCheck::Downloaded(newest, _) = self.update_check else {
            return;
        };
        self.update_check = match install {
            Ok(()) => UpdateCheck::Installing(newest),
            Err(ReleaseInstallFailed) => UpdateCheck::InstallFailed(newest),
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
        let switch = (self.self_update == SelfUpdate::Supported).then(|| {
            Row::Switch(Switch {
                id: ControlId::new(KEEP_UP_TO_DATE),
                icon: RowIcon::CircleArrows,
                label: translations.text("about-keep-up-to-date"),
                description: translations.text("about-keep-up-to-date-description"),
                is_on: self.updates == Updates::Automatic,
                note: None,
            })
        });
        let mut rows = vec![Row::Item(version_item)];
        rows.extend(switch);
        rows.push(Row::Link(button(SOURCE_CODE, "about-source-code", translations)));
        Section {
            footer: Some(translations.text("about-footer")),
            ..Section::new(rows)
        }
    }

    /// The line below the version and the button after it, for the state of the check.
    fn update_status(&self, translations: &Translations) -> (String, Accessory) {
        let check = button(CHECK_FOR_UPDATES, "about-check-for-updates", translations);
        let dimmed_check = Accessory::Action(Button {
            is_enabled: false,
            ..check.clone()
        });
        let retry = Accessory::Failed {
            retry: button(CHECK_FOR_UPDATES, "about-try-again", translations),
        };
        let with_version = |message: &str, version: Version| {
            translations.text_with(
                message,
                &[("version", MessageArgument::Text(&version.to_string()))],
            )
        };
        match self.update_check {
            UpdateCheck::NotStarted => (String::new(), Accessory::Action(check)),
            UpdateCheck::Running => (translations.text("about-checking"), dimmed_check),
            UpdateCheck::UpToDate => (translations.text("about-up-to-date"), Accessory::Action(check)),
            UpdateCheck::Available(newest) => (
                with_version("about-update-available", newest),
                Accessory::Download(button(DOWNLOAD_UPDATE, "about-download", translations)),
            ),
            UpdateCheck::Downloading(newest) => (with_version("about-downloading", newest), dimmed_check),
            UpdateCheck::Downloaded(newest, _) => {
                (with_version("about-install-waiting", newest), dimmed_check)
            }
            UpdateCheck::Installing(newest) => (with_version("about-installing", newest), dimmed_check),
            UpdateCheck::CheckFailed => (translations.text("about-check-failed"), retry),
            UpdateCheck::InstallFailed(newest) => (with_version("about-install-failed", newest), retry),
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
