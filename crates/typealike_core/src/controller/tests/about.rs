//! Tests of the check for updates of the About pane.

use super::harness::{Call, Harness};
use crate::release::{ReleaseReadFailed, ReleaseTag};
use crate::settings::Settings;
use crate::settings_window::about::{CHECK_FOR_UPDATES, DOWNLOAD_UPDATE};
use crate::settings_window::view::{Item, PageIcon, Row, SettingsView};

const NEWER_RELEASE: &str = "v99.0.0";

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
