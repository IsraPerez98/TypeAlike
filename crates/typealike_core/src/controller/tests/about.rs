//! Tests of the check for updates of the About pane, and of the install of a new version.

use std::path::PathBuf;

use super::first_run::{download_recommended_model, finish_download, processor_model};
use super::harness::{Call, Harness};
use crate::controller::{FIRST_UPDATE_CHECK_DELAY, Timer, UPDATE_CHECK_INTERVAL};
use crate::inference::Backend;
use crate::release::{DownloadedRelease, ReleaseDownloadFailed, ReleaseReadFailed, ReleaseTag};
use crate::settings::{Settings, Updates};
use crate::settings_window::about::{CHECK_FOR_UPDATES, DOWNLOAD_UPDATE, KEEP_UP_TO_DATE};
use crate::settings_window::view::{Item, PageIcon, Row, SettingsView};

const NEWER_RELEASE: &str = "v99.0.0";

fn manual_updates() -> Settings {
    Settings {
        updates: Updates::Manual,
        ..Settings::default()
    }
}

fn downloaded_release() -> DownloadedRelease {
    DownloadedRelease(PathBuf::from("TypeAlike-99.0.0-setup.exe"))
}

/// An installed copy whose check of the day found the newer release.
fn harness_with_newer_release(settings: Settings) -> Harness {
    let mut harness = Harness::installed(settings);
    harness
        .controller
        .on_timer(Timer::UpdateCheck, &mut harness.shell);
    harness.answer_release_read(Ok(ReleaseTag(NEWER_RELEASE.to_owned())));
    harness
}

/// A harness whose check for updates found the release `tag`.
fn checked_harness(tag: &str) -> Harness {
    let mut harness = Harness::new(Settings::default());
    harness.press_button(CHECK_FOR_UPDATES);
    harness.answer_release_read(Ok(ReleaseTag(tag.to_owned())));
    harness
}

/// The row of the version on the About page of the settings that the shell rendered last.
fn rendered_version_item(harness: &mut Harness) -> Option<Item> {
    let view = harness
        .take_calls()
        .into_iter()
        .rev()
        .find_map(|call| match call {
            Call::RenderSettings(view) => Some(*view),
            _ => None,
        })?;
    version_item(&view)
}

fn version_item(view: &SettingsView) -> Option<Item> {
    let page = view.pages.iter().find(|page| page.icon == PageIcon::Info)?;
    match page.sections.first()?.rows.first()? {
        Row::Item(item) => Some(item.clone()),
        _ => None,
    }
}

#[test]
fn check_for_updates_reads_the_newest_release_once() {
    let mut harness = Harness::new(Settings::default());

    harness.press_button(CHECK_FOR_UPDATES);
    harness.press_button(CHECK_FOR_UPDATES);

    assert_eq!(harness.release_reads(), 1);
}

#[test]
fn newest_release_shows_on_the_about_page() {
    let mut harness = checked_harness(NEWER_RELEASE);

    let item = rendered_version_item(&mut harness);

    assert_eq!(
        item.map(|shown| shown.subtitle).as_deref(),
        Some("Version 99.0.0 is available.")
    );
}

#[test]
fn failed_read_shows_on_the_about_page() {
    let mut harness = Harness::new(Settings::default());
    harness.press_button(CHECK_FOR_UPDATES);

    harness.answer_release_read(Err(ReleaseReadFailed));

    assert_eq!(
        rendered_version_item(&mut harness)
            .map(|shown| shown.subtitle)
            .as_deref(),
        Some("TypeAlike cannot check for updates. Check your internet connection.")
    );
}

#[test]
fn download_opens_the_page_of_the_newest_release_in_the_browser() {
    let mut harness = checked_harness(NEWER_RELEASE);
    harness.take_calls();

    harness.press_button(DOWNLOAD_UPDATE);

    let opened: Vec<_> = harness
        .take_calls()
        .into_iter()
        .filter(|call| matches!(call, Call::OpenWebPage(_)))
        .collect();
    assert_eq!(
        opened,
        [Call::OpenWebPage(format!(
            "{}/releases/latest",
            env!("CARGO_PKG_REPOSITORY")
        ))]
    );
}

#[test]
fn launch_waits_for_the_first_check_for_a_new_version() {
    let mut harness = Harness::installed(Settings::default());

    harness.controller.on_launch(&mut harness.shell);

    assert!(
        harness
            .take_calls()
            .contains(&Call::StartTimer(Timer::UpdateCheck, FIRST_UPDATE_CHECK_DELAY))
    );
}

#[test]
fn check_of_the_day_reads_the_newest_release_and_waits_for_the_next_day() {
    let mut harness = Harness::installed(Settings::default());

    harness
        .controller
        .on_timer(Timer::UpdateCheck, &mut harness.shell);

    assert_eq!(
        (
            harness.release_reads(),
            harness
                .take_calls()
                .contains(&Call::StartTimer(Timer::UpdateCheck, UPDATE_CHECK_INTERVAL))
        ),
        (1, true)
    );
}

#[test]
fn check_of_the_day_reads_nothing_while_updates_are_manual() {
    let mut harness = Harness::installed(manual_updates());

    harness
        .controller
        .on_timer(Timer::UpdateCheck, &mut harness.shell);

    assert_eq!(harness.release_reads(), 0);
}

#[test]
fn check_of_the_day_reads_nothing_in_a_build_from_the_source_code() {
    let mut harness = Harness::new(Settings::default());

    harness
        .controller
        .on_timer(Timer::UpdateCheck, &mut harness.shell);

    assert_eq!(harness.release_reads(), 0);
}

#[test]
fn newer_release_downloads_while_updates_are_automatic() {
    let harness = harness_with_newer_release(Settings::default());

    assert_eq!(
        harness.release_downloads(),
        [ReleaseTag(NEWER_RELEASE.to_owned())]
    );
}

#[test]
fn newer_release_does_not_download_while_updates_are_manual() {
    let mut harness = Harness::installed(manual_updates());
    harness.press_button(CHECK_FOR_UPDATES);

    harness.answer_release_read(Ok(ReleaseTag(NEWER_RELEASE.to_owned())));

    assert_eq!(harness.release_downloads(), []);
}

#[test]
fn downloaded_release_installs_and_quits() {
    let mut harness = harness_with_newer_release(Settings::default());
    harness.take_calls();

    harness.answer_release_download(Ok(downloaded_release()));

    assert_eq!(
        (harness.installs(), harness.take_calls().contains(&Call::Quit)),
        (vec![downloaded_release()], true)
    );
}

#[test]
fn downloaded_release_installs_after_the_running_model_download() {
    let settings = Settings {
        backend: Backend::Cpu,
        ..Settings::default()
    };
    let mut harness = harness_with_newer_release(settings);
    download_recommended_model(&mut harness);
    harness.answer_release_download(Ok(downloaded_release()));
    let installs_during_the_model_download = harness.installs().len();

    finish_download(&mut harness, processor_model());

    assert_eq!(
        (installs_during_the_model_download, harness.installs().len()),
        (0, 1)
    );
}

#[test]
fn failed_install_shows_on_the_about_page_and_does_not_quit() {
    let mut harness = harness_with_newer_release(Settings::default());
    harness.fail_installs();

    harness.answer_release_download(Ok(downloaded_release()));

    let calls = harness.take_calls();
    let subtitle = calls.iter().rev().find_map(|call| match call {
        Call::RenderSettings(view) => version_item(view).map(|item| item.subtitle),
        _ => None,
    });
    assert_eq!(
        (subtitle.as_deref(), calls.contains(&Call::Quit)),
        (Some("TypeAlike cannot install version 99.0.0."), false)
    );
}

#[test]
fn failed_download_installs_nothing() {
    let mut harness = harness_with_newer_release(Settings::default());

    harness.answer_release_download(Err(ReleaseDownloadFailed));

    assert_eq!(harness.installs(), []);
}

#[test]
fn switch_keeps_the_updates_in_the_settings() {
    let mut harness = Harness::installed(Settings::default());

    harness.toggle(KEEP_UP_TO_DATE, false);

    assert_eq!(
        harness.saved().last().map(|settings| settings.updates),
        Some(Updates::Manual)
    );
}
