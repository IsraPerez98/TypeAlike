//! Tests of the first run: the model that TypeAlike recommends before the first download, and
//! the first download that becomes the suggestion model.

use super::harness::Harness;
use crate::controller::Delivery;
use crate::hub::{ByteSize, HubEvent, HubRequest, ModelFile};
use crate::inference::{Backend, InferenceEvent, InferenceRequest};
use crate::settings::Settings;
use crate::settings_window::general::BACKEND;
use crate::settings_window::models::SEARCH;
use crate::settings_window::view::{Accessory, ControlId, Item, Page, Row, SettingsInput};

pub(super) fn processor_model() -> ModelFile {
    ModelFile {
        repo: "mradermacher/Qwen3.5-0.8B-Base-i1-GGUF".to_owned(),
        name: "Qwen3.5-0.8B-Base.i1-IQ4_NL.gguf".to_owned(),
    }
}

fn settings_on(backend: Backend) -> Settings {
    Settings {
        backend,
        ..Settings::default()
    }
}

fn items(page: &Page) -> Vec<&Item> {
    page.sections
        .iter()
        .flat_map(|section| &section.rows)
        .filter_map(|row| match row {
            Row::Item(item) => Some(item),
            _ => None,
        })
        .collect()
}

fn recommended_titles(harness: &mut Harness) -> Vec<String> {
    let view = harness.shown_settings();
    items(&view.pages[0])
        .iter()
        .map(|item| item.title.clone())
        .collect()
}

fn download_button(harness: &mut Harness) -> ControlId {
    let view = harness.shown_settings();
    match &items(&view.pages[0])[0].accessory {
        Accessory::Download(button) => button.id.clone(),
        other => panic!("the recommended model offers no download: {other:?}"),
    }
}

pub(super) fn download_recommended_model(harness: &mut Harness) {
    let button = download_button(harness);
    harness
        .controller
        .on_settings_input(&SettingsInput::Pressed(button), &mut harness.shell);
}

pub(super) fn finish_download(harness: &mut Harness, file: ModelFile) {
    let finished = HubEvent::DownloadFinished {
        file,
        result: Ok(ByteSize(1)),
    };
    harness
        .controller
        .on_delivery(Delivery::Hub(finished), &mut harness.shell);
}

#[test]
fn first_run_recommends_a_small_model_for_the_processor() {
    let mut harness = Harness::new(settings_on(Backend::Cpu));

    let titles = recommended_titles(&mut harness);

    assert_eq!(titles, ["Qwen3.5 0.8B"]);
}

#[test]
fn first_run_recommends_a_larger_model_for_a_graphics_card() {
    let mut harness = Harness::new(settings_on(Backend::Vulkan));

    let titles = recommended_titles(&mut harness);

    assert_eq!(titles, ["Gemma 4 E2B"]);
}

#[test]
fn first_run_on_a_graphics_card_reads_its_free_memory() {
    let harness = Harness::new(settings_on(Backend::Vulkan));

    assert_eq!(
        harness.requests(),
        [InferenceRequest::ReadGpuMemory {
            backend: Backend::Vulkan
        }]
    );
}

#[test]
fn first_run_on_the_processor_reads_no_gpu_memory() {
    let harness = Harness::new(settings_on(Backend::Cpu));

    assert_eq!(harness.requests(), []);
}

#[test]
fn first_run_recommends_the_model_that_fits_the_free_memory_of_the_graphics_card() {
    let mut harness = Harness::new(settings_on(Backend::Vulkan));
    harness.answer(InferenceEvent::GpuMemory {
        backend: Backend::Vulkan,
        result: Ok(ByteSize(8 * 1024 * 1024 * 1024)),
    });

    let titles = recommended_titles(&mut harness);

    assert_eq!(titles, ["Gemma 4 E4B"]);
}

#[test]
fn backend_change_before_the_first_download_reads_the_free_memory_of_the_new_gpu() {
    let mut harness = Harness::new(settings_on(Backend::Cpu));

    harness.pick(BACKEND, 1);

    assert_eq!(
        harness.last_request(),
        Some(InferenceRequest::ReadGpuMemory {
            backend: Backend::Vulkan
        })
    );
}

#[test]
fn recommended_model_hides_after_a_model_is_downloaded() {
    let mut harness = Harness::new(settings_on(Backend::Cpu));
    harness.install(processor_model());

    let titles = recommended_titles(&mut harness);

    assert_eq!(titles, Vec::<String>::new());
}

#[test]
fn download_button_of_the_recommended_model_downloads_it() {
    let mut harness = Harness::new(settings_on(Backend::Cpu));

    download_recommended_model(&mut harness);

    assert_eq!(
        harness.hub_requests().last(),
        Some(&HubRequest::Download {
            file: processor_model()
        })
    );
}

#[test]
fn download_button_of_the_recommended_model_works_during_a_search() {
    let mut harness = Harness::new(settings_on(Backend::Cpu));
    let button = download_button(&mut harness);
    let search = SettingsInput::Submitted {
        id: ControlId::new(SEARCH),
        text: "gemma".to_owned(),
    };
    harness.controller.on_settings_input(&search, &mut harness.shell);

    harness
        .controller
        .on_settings_input(&SettingsInput::Pressed(button), &mut harness.shell);

    assert_eq!(
        harness.hub_requests().last(),
        Some(&HubRequest::Download {
            file: processor_model()
        })
    );
}

#[test]
fn first_finished_download_starts_as_the_suggestion_model() {
    let mut harness = Harness::new(settings_on(Backend::Cpu));
    download_recommended_model(&mut harness);

    finish_download(&mut harness, processor_model());

    assert_eq!(
        harness.last_request(),
        Some(InferenceRequest::Load {
            file: processor_model(),
            backend: Backend::Cpu,
        })
    );
}

#[test]
fn finished_download_that_the_pane_did_not_start_is_no_suggestion_model() {
    let mut harness = Harness::new(settings_on(Backend::Cpu));

    finish_download(&mut harness, processor_model());

    assert_eq!(harness.last_request(), None);
}

#[test]
fn finished_download_keeps_the_suggestion_model_of_the_user() {
    let picked = ModelFile {
        repo: "owner/Picked-GGUF".to_owned(),
        name: "picked.gguf".to_owned(),
    };
    let settings = Settings {
        suggestion_model: Some(picked.clone()),
        ..settings_on(Backend::Cpu)
    };
    let mut harness = Harness::new(settings);
    download_recommended_model(&mut harness);

    finish_download(&mut harness, processor_model());

    assert_eq!(
        harness.last_request(),
        Some(InferenceRequest::Load {
            file: picked,
            backend: Backend::Cpu,
        })
    );
}
