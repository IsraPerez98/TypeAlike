use std::path::PathBuf;

use super::*;
use crate::localization::UiLanguage;
use crate::test_support::{english, writing_rule_breaks};

const LIBRARIES: &[Library] = &[
    Library {
        name: "anyhow",
        version: "1.0.104",
        license: "MIT",
        texts: &["MIT License text", "Apache License text"],
    },
    Library {
        name: "Fluent UI System Icons",
        version: "",
        license: "MIT",
        texts: &["Icons license text"],
    },
];
const NEWER_TAG: &str = "v99.0.0";
const OLDER_TAG: &str = "v0.0.1";

/// The pane of a build from the source code, which cannot install a new version.
fn pane_from_source() -> AboutPane {
    AboutPane::new(LIBRARIES, Updates::Automatic, SelfUpdate::Unsupported)
}

/// The pane of an installed copy, with `updates`.
fn installed_pane(updates: Updates) -> AboutPane {
    AboutPane::new(LIBRARIES, updates, SelfUpdate::Supported)
}

/// An installed pane with automatic updates, whose check found the newer release.
fn downloading_pane() -> AboutPane {
    let mut pane = installed_pane(Updates::Automatic);
    pane.on_action(AboutAction::CheckForUpdates);
    pane.on_newest_release(Ok(tag(NEWER_TAG)));
    pane
}

fn downloaded_release() -> DownloadedRelease {
    DownloadedRelease(PathBuf::from("TypeAlike-99.0.0.dmg"))
}

fn downloaded_pane() -> AboutPane {
    let mut pane = downloading_pane();
    pane.on_release_downloaded(Ok(downloaded_release()));
    pane
}

fn press(id: &str) -> SettingsInput {
    SettingsInput::Pressed(ControlId::from(id.to_owned()))
}

/// A pane whose check for updates ended with `release`.
fn checked_pane(release: Result<ReleaseTag, ReleaseReadFailed>) -> AboutPane {
    let mut pane = pane_from_source();
    pane.on_action(AboutAction::CheckForUpdates);
    pane.on_newest_release(release);
    pane
}

fn tag(text: &str) -> ReleaseTag {
    ReleaseTag(text.to_owned())
}

/// The row of the version of TypeAlike, at the top of the page in English.
fn version_item(pane: &AboutPane) -> Item {
    version_item_in(pane, &english())
}

/// The row of the version of TypeAlike, at the top of the page with the texts of `translations`.
fn version_item_in(pane: &AboutPane, translations: &Translations) -> Item {
    match pane.page(translations).sections[0].rows.first() {
        Some(Row::Item(item)) => item.clone(),
        other => panic!("the page starts with no version: {other:?}"),
    }
}

fn check_button(label: &str, is_enabled: bool) -> Button {
    Button {
        id: ControlId::new(CHECK_FOR_UPDATES),
        label: label.to_owned(),
        is_enabled,
    }
}

#[test]
fn page_shows_the_version_the_source_code_and_each_library() {
    let pane = pane_from_source();

    let shown = pane.page(&english());

    let version = env!("CARGO_PKG_VERSION");
    assert_eq!(shown, Page {
        title: "About".to_owned(),
        icon: PageIcon::Info,
        summary: "The version of TypeAlike and the licenses of the software that it contains.".to_owned(),
        search: None,
        sections: vec![
            Section {
                footer: Some("TypeAlike is free software under the GNU General Public License, version 3 or later.".to_owned()),
                ..Section::new(vec![
                    Row::Item(Item {
                        title: format!("TypeAlike {version}"),
                        subtitle: String::new(),
                        detail: String::new(),
                        accessory: Accessory::Action(check_button("Check for updates", true)),
                    }),
                    Row::Link(Button {
                        id: ControlId::new(SOURCE_CODE),
                        label: "Show the source code".to_owned(),
                        is_enabled: true,
                    }),
                ])
            },
            Section {
                heading: Some("Third-party software".to_owned()),
                footer: Some("TypeAlike contains this open-source software. Each library has its own license.".to_owned()),
                ..Section::new(vec![
                    Row::Item(Item {
                        title: "anyhow".to_owned(),
                        subtitle: "MIT".to_owned(),
                        detail: "1.0.104".to_owned(),
                        accessory: Accessory::Open(Button {
                            id: ControlId::new("about-license-0"),
                            label: "Show license".to_owned(),
                            is_enabled: true,
                        }),
                    }),
                    Row::Item(Item {
                        title: "Fluent UI System Icons".to_owned(),
                        subtitle: "MIT".to_owned(),
                        detail: String::new(),
                        accessory: Accessory::Open(Button {
                            id: ControlId::new("about-license-1"),
                            label: "Show license".to_owned(),
                            is_enabled: true,
                        }),
                    }),
                ])
            },
        ],
    });
}

#[test]
fn each_button_of_the_pane_has_its_action() {
    let pane = pane_from_source();
    let cases = [
        (CHECK_FOR_UPDATES, Some(AboutAction::CheckForUpdates)),
        (DOWNLOAD_UPDATE, Some(AboutAction::DownloadUpdate)),
        (SOURCE_CODE, Some(AboutAction::ShowSourceCode)),
        (BACK_TO_LIBRARIES, Some(AboutAction::BackToLibraries)),
        ("about-license-1", Some(AboutAction::ShowLicense(1))),
        ("about-license-2", None),
        ("about-license-first", None),
        ("download", None),
    ];

    let actions = cases.map(|(id, _)| pane.action_for(&press(id)));

    assert_eq!(actions, cases.map(|(_, action)| action));
}

#[test]
fn switch_with_the_id_of_a_button_does_nothing() {
    let pane = pane_from_source();
    let input = SettingsInput::Toggled {
        id: ControlId::new(CHECK_FOR_UPDATES),
        is_on: true,
    };

    assert_eq!(pane.action_for(&input), None);
}

#[test]
fn check_for_updates_reads_the_newest_release_and_shows_the_wait() {
    let mut pane = pane_from_source();

    let request = pane.on_action(AboutAction::CheckForUpdates);

    assert_eq!(request, Some(AboutRequest::ReadNewestRelease));
    let item = version_item(&pane);
    assert_eq!(
        (item.subtitle.as_str(), item.accessory),
        (
            "Checking for updates…",
            Accessory::Action(check_button("Check for updates", false))
        )
    );
}

#[test]
fn check_that_runs_does_not_start_again() {
    let mut pane = pane_from_source();
    pane.on_action(AboutAction::CheckForUpdates);

    let request = pane.on_action(AboutAction::CheckForUpdates);

    assert_eq!(request, None);
}

#[test]
fn newer_release_offers_its_download() {
    let pane = checked_pane(Ok(tag(NEWER_TAG)));

    let item = version_item(&pane);

    assert_eq!(
        (item.subtitle.as_str(), item.accessory),
        (
            "Version 99.0.0 is available.",
            Accessory::Download(Button {
                id: ControlId::new(DOWNLOAD_UPDATE),
                label: "Download".to_owned(),
                is_enabled: true,
            })
        )
    );
}

#[test]
fn release_of_this_version_or_older_is_up_to_date() {
    let this_version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let panes = [
        checked_pane(Ok(tag(&this_version))),
        checked_pane(Ok(tag(OLDER_TAG))),
    ];

    let statuses = panes.map(|pane| version_item(&pane).subtitle);

    assert_eq!(
        statuses,
        ["This is the newest version.", "This is the newest version."]
    );
}

#[test]
fn failed_read_or_unknown_tag_offers_another_try() {
    let panes = [
        checked_pane(Err(ReleaseReadFailed)),
        checked_pane(Ok(tag("nightly"))),
    ];

    let items = panes.map(|pane| version_item(&pane));

    let failed = (
        "TypeAlike cannot check for updates. Check your internet connection.".to_owned(),
        Accessory::Failed {
            retry: check_button("Try again", true),
        },
    );
    assert_eq!(
        items.map(|item| (item.subtitle, item.accessory)),
        [failed.clone(), failed]
    );
}

#[test]
fn release_without_a_running_check_changes_nothing() {
    let mut pane = pane_from_source();

    pane.on_newest_release(Ok(tag(NEWER_TAG)));

    assert_eq!(version_item(&pane).subtitle, "");
}

#[test]
fn download_opens_the_page_of_the_newest_release() {
    let mut pane = checked_pane(Ok(tag(NEWER_TAG)));

    let request = pane.on_action(AboutAction::DownloadUpdate);

    assert_eq!(
        request,
        Some(AboutRequest::OpenWebPage(format!("{REPOSITORY}/releases/latest")))
    );
}

#[test]
fn download_without_a_newer_release_does_nothing() {
    let mut pane = checked_pane(Ok(tag(OLDER_TAG)));

    let request = pane.on_action(AboutAction::DownloadUpdate);

    assert_eq!(request, None);
}

#[test]
fn source_code_link_opens_the_repository() {
    let mut pane = pane_from_source();

    let request = pane.on_action(AboutAction::ShowSourceCode);

    assert_eq!(request, Some(AboutRequest::OpenWebPage(REPOSITORY.to_owned())));
}

#[test]
fn show_license_shows_each_license_text_of_the_library() {
    let mut pane = pane_from_source();

    pane.on_action(AboutAction::ShowLicense(0));

    assert_eq!(
        pane.page(&english()).sections[1],
        Section {
            heading: Some("anyhow".to_owned()),
            back: Some(Button {
                id: ControlId::new(BACK_TO_LIBRARIES),
                label: "Back to the list".to_owned(),
                is_enabled: true,
            }),
            ..Section::new(vec![
                Row::Output(Output {
                    text: "MIT License text".to_owned(),
                    detail: String::new(),
                }),
                Row::Output(Output {
                    text: "Apache License text".to_owned(),
                    detail: String::new(),
                }),
            ])
        }
    );
}

#[test]
fn back_shows_the_list_of_the_libraries_again() {
    let mut pane = pane_from_source();
    pane.on_action(AboutAction::ShowLicense(0));

    pane.on_action(AboutAction::BackToLibraries);

    let heading = pane.page(&english()).sections[1].heading.clone();
    assert_eq!(heading.as_deref(), Some("Third-party software"));
}

#[test]
fn page_follows_the_writing_rules() {
    let mut license_shown = pane_from_source();
    license_shown.on_action(AboutAction::ShowLicense(0));
    let mut running = pane_from_source();
    running.on_action(AboutAction::CheckForUpdates);
    let mut install_failed = downloaded_pane();
    install_failed.on_install(Err(ReleaseInstallFailed));
    let mut installing = downloaded_pane();
    installing.on_install(Ok(()));
    let panes = [
        pane_from_source(),
        running,
        checked_pane(Ok(tag(NEWER_TAG))),
        checked_pane(Ok(tag(OLDER_TAG))),
        checked_pane(Err(ReleaseReadFailed)),
        license_shown,
        downloading_pane(),
        downloaded_pane(),
        installing,
        install_failed,
    ];

    let texts: Vec<_> = panes
        .iter()
        .flat_map(|pane| pane.page(&english()).texts())
        .collect();

    assert_eq!(writing_rule_breaks(&texts), Vec::<String>::new());
}

#[test]
fn page_shows_in_spanish() {
    let pane = checked_pane(Ok(tag(NEWER_TAG)));
    let translations = Translations::new(UiLanguage::Spanish);

    let (title, item) = (
        pane.page(&translations).title,
        version_item_in(&pane, &translations),
    );

    assert_eq!(
        (title.as_str(), item.subtitle.as_str()),
        ("Acerca de", "La versión 99.0.0 está disponible.")
    );
}

#[test]
fn installed_copy_shows_the_switch_that_keeps_it_up_to_date() {
    let pane = installed_pane(Updates::Automatic);

    let rows = pane.page(&english()).sections[0].rows.clone();

    assert_eq!(
        rows.get(1),
        Some(&Row::Switch(Switch {
            id: ControlId::new(KEEP_UP_TO_DATE),
            icon: RowIcon::CircleArrows,
            label: "Keep TypeAlike up to date".to_owned(),
            description:
                "TypeAlike checks for a new version once a day. It installs the new version and starts again."
                    .to_owned(),
            is_on: true,
            note: None,
        }))
    );
}

#[test]
fn build_from_the_source_code_has_no_switch() {
    let pane = pane_from_source();

    let rows = pane.page(&english()).sections[0].rows.clone();

    assert!(!rows.iter().any(|row| matches!(row, Row::Switch(_))));
}

#[test]
fn switch_turns_the_updates_on_and_off() {
    let pane = installed_pane(Updates::Automatic);
    let inputs = [false, true].map(|is_on| SettingsInput::Toggled {
        id: ControlId::new(KEEP_UP_TO_DATE),
        is_on,
    });

    let actions = inputs.map(|input| pane.action_for(&input));

    assert_eq!(
        actions,
        [
            Some(AboutAction::SwitchUpdates(Updates::Manual)),
            Some(AboutAction::SwitchUpdates(Updates::Automatic)),
        ]
    );
}

#[test]
fn switch_saves_the_settings() {
    let mut pane = installed_pane(Updates::Automatic);

    let request = pane.on_action(AboutAction::SwitchUpdates(Updates::Manual));

    assert_eq!(
        (request, pane.updates()),
        (Some(AboutRequest::SaveSettings), Updates::Manual)
    );
}

#[test]
fn scheduled_check_reads_the_newest_release_only_while_updates_are_automatic() {
    let mut panes = [
        installed_pane(Updates::Automatic),
        installed_pane(Updates::Manual),
        pane_from_source(),
    ];

    let requests = panes.each_mut().map(AboutPane::on_scheduled_check);

    assert_eq!(requests, [Some(AboutRequest::ReadNewestRelease), None, None]);
}

#[test]
fn scheduled_check_does_not_start_while_a_download_runs() {
    let mut pane = downloading_pane();

    let request = pane.on_scheduled_check();

    assert_eq!(request, None);
}

#[test]
fn newer_release_downloads_while_updates_are_automatic() {
    let mut pane = installed_pane(Updates::Automatic);
    pane.on_action(AboutAction::CheckForUpdates);

    let request = pane.on_newest_release(Ok(tag(NEWER_TAG)));

    assert_eq!(
        (request, version_item(&pane).subtitle),
        (
            Some(AboutRequest::DownloadRelease(tag(NEWER_TAG))),
            "Downloading version 99.0.0…".to_owned()
        )
    );
}

#[test]
fn newer_release_waits_for_the_user_while_updates_are_manual() {
    let mut pane = installed_pane(Updates::Manual);
    pane.on_action(AboutAction::CheckForUpdates);

    let request = pane.on_newest_release(Ok(tag(NEWER_TAG)));

    assert_eq!(
        (request, version_item(&pane).subtitle),
        (None, "Version 99.0.0 is available.".to_owned())
    );
}

#[test]
fn downloaded_release_waits_for_its_install() {
    let pane = downloaded_pane();

    assert_eq!(
        (pane.release_to_install(), version_item(&pane).subtitle),
        (
            Some(&downloaded_release()),
            "Version 99.0.0 installs when the model download ends.".to_owned()
        )
    );
}

#[test]
fn started_install_shows_the_install() {
    let mut pane = downloaded_pane();

    pane.on_install(Ok(()));

    assert_eq!(
        (pane.release_to_install(), version_item(&pane).subtitle),
        (None, "Installing version 99.0.0…".to_owned())
    );
}

#[test]
fn failed_download_or_install_offers_another_try() {
    let mut failed_download = downloading_pane();
    failed_download.on_release_downloaded(Err(ReleaseDownloadFailed));
    let mut failed_install = downloaded_pane();
    failed_install.on_install(Err(ReleaseInstallFailed));
    let panes = [failed_download, failed_install];

    let items = panes.map(|pane| version_item(&pane));

    let failed = (
        "TypeAlike cannot install version 99.0.0.".to_owned(),
        Accessory::Failed {
            retry: check_button("Try again", true),
        },
    );
    assert_eq!(
        items.map(|item| (item.subtitle, item.accessory)),
        [failed.clone(), failed]
    );
}

#[test]
fn switch_off_stops_the_install_of_the_running_download() {
    let mut pane = downloading_pane();

    pane.on_action(AboutAction::SwitchUpdates(Updates::Manual));
    pane.on_release_downloaded(Ok(downloaded_release()));

    assert_eq!(
        (pane.release_to_install(), version_item(&pane).subtitle),
        (None, "Version 99.0.0 is available.".to_owned())
    );
}

#[test]
fn check_does_not_start_while_a_release_waits_for_its_install() {
    let mut pane = downloaded_pane();

    let request = pane.on_action(AboutAction::CheckForUpdates);

    assert_eq!(request, None);
}
