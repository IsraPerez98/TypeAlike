//! The tests of the Models pane.

use super::*;

const REPO: &str = "owner/Model-GGUF";

fn file(name: &str) -> ModelFile {
    ModelFile {
        repo: REPO.to_owned(),
        name: name.to_owned(),
    }
}

fn sized(name: &str, size: u64) -> SizedFile {
    SizedFile {
        file: file(name),
        size: ByteSize(size),
    }
}

fn repo(id: &str, downloads: u64, is_gated: bool) -> RepoSummary {
    RepoSummary {
        id: id.to_owned(),
        downloads,
        is_gated,
    }
}

fn button(label: &'static str, action: &ModelsAction) -> Button {
    Button {
        id: button_id(action),
        label,
        is_enabled: true,
    }
}

fn downloading(name: &str, size: u64) -> ModelsPane {
    let mut pane = ModelsPane::new();
    pane.on_action(ModelsAction::Download(sized(name, size)));
    pane
}

fn installed(name: &str, size: u64) -> ModelsPane {
    let mut pane = ModelsPane::new();
    pane.on_event(HubEvent::Installed(vec![sized(name, size)]));
    pane
}

fn searched(query: &str, repos: Vec<RepoSummary>) -> ModelsPane {
    let mut pane = ModelsPane::new();
    pane.on_action(ModelsAction::Search(query.to_owned()));
    pane.on_event(HubEvent::SearchFinished {
        query: query.to_owned(),
        result: Ok(repos),
    });
    pane
}

fn opening_repo() -> ModelsPane {
    let mut pane = searched("model", vec![repo(REPO, 10, false)]);
    pane.on_action(ModelsAction::ShowFiles(REPO.to_owned()));
    pane
}

fn browsing(files: Vec<SizedFile>) -> ModelsPane {
    let mut pane = opening_repo();
    pane.on_event(HubEvent::FilesListed {
        repo: REPO.to_owned(),
        result: Ok(files),
    });
    pane
}

fn items(section: Section) -> Vec<Item> {
    section
        .rows
        .into_iter()
        .map(|row| match row {
            Row::Item(item) => item,
            other => panic!("the list shows {other:?}, not items"),
        })
        .collect()
}

fn browse(pane: &ModelsPane) -> Section {
    pane.page().sections.remove(0)
}

fn downloaded(pane: &ModelsPane) -> Option<Section> {
    pane.page().sections.into_iter().nth(1)
}

fn browse_rows(pane: &ModelsPane) -> Vec<Item> {
    items(browse(pane))
}

fn downloaded_row(pane: &ModelsPane) -> Item {
    items(downloaded(pane).expect("a file is tracked")).remove(0)
}

fn titles(items: Vec<Item>) -> Vec<String> {
    items.into_iter().map(|item| item.title).collect()
}

fn message(text: &str, button: Option<Button>) -> Vec<Row> {
    vec![Row::Message(Message {
        text: text.to_owned(),
        button,
    })]
}

fn accessory_button(item: Item) -> Button {
    match item.accessory {
        Accessory::Download(button)
        | Accessory::Open(button)
        | Accessory::Downloading { cancel: button, .. }
        | Accessory::Installed { delete: button }
        | Accessory::Failed { retry: button } => button,
        other => panic!("the item shows no button: {other:?}"),
    }
}

fn retry_button(section: Section) -> Button {
    match section.rows.into_iter().next() {
        Some(Row::Message(Message {
            button: Some(retry), ..
        })) => retry,
        other => panic!("the list shows no retry: {other:?}"),
    }
}

fn failed_search(error: HubError) -> ModelsPane {
    let mut pane = ModelsPane::new();
    pane.on_action(ModelsAction::Search("x".to_owned()));
    pane.on_event(HubEvent::SearchFinished {
        query: "x".to_owned(),
        result: Err(error),
    });
    pane
}

fn press(pane: &ModelsPane, button: Button) -> Option<ModelsAction> {
    pane.action_for(&SettingsInput::Pressed(button.id))
}

#[test]
fn opening_lists_downloaded_files() {
    let pane = ModelsPane::new();

    assert_eq!(pane.on_open(), HubRequest::ListInstalled);
}

#[test]
fn pane_explains_itself_above_the_search_field() {
    let shown = ModelsPane::new().page();

    assert_eq!(
        (shown.title, shown.icon, shown.summary, shown.search),
        (
            "Models",
            PageIcon::Cube,
            "GhostType uses a language model on this computer to suggest text.",
            Some(SearchField {
                id: ControlId::new(SEARCH),
                placeholder: "Search Hugging Face",
            })
        )
    );
}

#[test]
fn empty_search_browses_suggested_models() {
    let shown = browse(&ModelsPane::new());

    assert_eq!(
        (shown.heading, shown.back, shown.footer),
        (
            Some("Suggested models".to_owned()),
            None,
            Some(
                "Smaller models are faster and use less memory. Larger models give better suggestions. When the model runs on the CPU, select a model with 1B parameters or fewer, so that suggestions show sooner."
            )
        )
    );
}

#[test]
fn every_suggested_model_has_a_row() {
    let pane = ModelsPane::new();

    assert_eq!(browse_rows(&pane).len(), SUGGESTED_MODELS.len());
}

#[test]
fn suggested_model_offers_its_file_for_download() {
    let pane = ModelsPane::new();
    let model = &SUGGESTED_MODELS[0];

    let row = browse_rows(&pane).remove(0);

    assert_eq!(
        row,
        Item {
            title: model.name.to_owned(),
            subtitle: model.description(),
            detail: model.size.to_string(),
            accessory: Accessory::Download(button("Download", &ModelsAction::Download(model.sized_file()))),
        }
    );
}

#[test]
fn suggested_model_shows_its_download_state() {
    let model = &SUGGESTED_MODELS[0];
    let mut pane = ModelsPane::new();
    pane.on_action(ModelsAction::Download(model.sized_file()));

    let row = browse_rows(&pane).remove(0);

    assert_eq!(
        row.accessory,
        Accessory::Downloading {
            progress: Permille(0),
            cancel: button("Cancel download", &ModelsAction::Cancel(model.sized_file().file)),
        }
    );
}

#[test]
fn download_action_requests_download() {
    let mut pane = ModelsPane::new();

    let request = pane.on_action(ModelsAction::Download(sized("a.gguf", 100)));

    assert_eq!(request, Some(HubRequest::Download { file: file("a.gguf") }));
}

#[test]
fn downloaded_list_has_a_heading_and_no_footer() {
    let pane = installed("a.gguf", 100);

    let shown = downloaded(&pane).expect("a file is installed");

    assert_eq!(
        (shown.heading, shown.back, shown.footer),
        (Some("Downloaded models".to_owned()), None, None)
    );
}

#[test]
fn running_download_shows_progress_and_cancel() {
    let mut pane = downloading("a.gguf", 2_000_000);

    pane.on_event(HubEvent::DownloadProgress {
        file: file("a.gguf"),
        received: ByteSize(500_000),
    });

    assert_eq!(
        downloaded_row(&pane),
        Item {
            title: "a".to_owned(),
            subtitle: REPO.to_owned(),
            detail: "500 KB of 2 MB".to_owned(),
            accessory: Accessory::Downloading {
                progress: Permille(250),
                cancel: button("Cancel download", &ModelsAction::Cancel(file("a.gguf"))),
            },
        }
    );
}

#[test]
fn progress_past_expected_size_shows_as_complete() {
    let mut pane = downloading("a.gguf", 100);

    pane.on_event(HubEvent::DownloadProgress {
        file: file("a.gguf"),
        received: ByteSize(150),
    });

    assert!(matches!(
        downloaded_row(&pane).accessory,
        Accessory::Downloading {
            progress: Permille(1000),
            ..
        }
    ));
}

#[test]
fn empty_expected_size_shows_no_progress_yet() {
    let mut pane = downloading("a.gguf", 0);

    pane.on_event(HubEvent::DownloadProgress {
        file: file("a.gguf"),
        received: ByteSize(150),
    });

    assert!(matches!(
        downloaded_row(&pane).accessory,
        Accessory::Downloading {
            progress: Permille(0),
            ..
        }
    ));
}

#[test]
fn running_download_is_not_started_twice() {
    let mut pane = downloading("a.gguf", 100);

    let request = pane.on_action(ModelsAction::Download(sized("a.gguf", 100)));

    assert_eq!(request, None);
}

#[test]
fn cancel_action_requests_cancel() {
    let mut pane = downloading("a.gguf", 100);

    let request = pane.on_action(ModelsAction::Cancel(file("a.gguf")));

    assert_eq!(request, Some(HubRequest::CancelDownload { file: file("a.gguf") }));
}

#[test]
fn canceling_download_shows_it_is_busy() {
    let mut pane = downloading("a.gguf", 100);

    pane.on_action(ModelsAction::Cancel(file("a.gguf")));

    let row = downloaded_row(&pane);
    assert_eq!(
        (row.detail.as_str(), row.accessory),
        ("Canceling…", Accessory::Canceling)
    );
}

#[test]
fn cancel_without_running_download_is_ignored() {
    let mut pane = installed("a.gguf", 100);

    let request = pane.on_action(ModelsAction::Cancel(file("a.gguf")));

    assert_eq!(request, None);
}

#[test]
fn progress_after_cancel_is_ignored() {
    let mut pane = downloading("a.gguf", 100);
    pane.on_action(ModelsAction::Cancel(file("a.gguf")));

    pane.on_event(HubEvent::DownloadProgress {
        file: file("a.gguf"),
        received: ByteSize(50),
    });

    assert_eq!(downloaded_row(&pane).accessory, Accessory::Canceling);
}

#[test]
fn canceled_download_is_forgotten() {
    let mut pane = downloading("a.gguf", 100);
    pane.on_action(ModelsAction::Cancel(file("a.gguf")));

    pane.on_event(HubEvent::DownloadFinished {
        file: file("a.gguf"),
        result: Err(HubError::Canceled),
    });

    assert_eq!(downloaded(&pane), None);
}

#[test]
fn failure_while_canceling_is_forgotten() {
    let mut pane = downloading("a.gguf", 100);
    pane.on_action(ModelsAction::Cancel(file("a.gguf")));

    pane.on_event(HubEvent::DownloadFinished {
        file: file("a.gguf"),
        result: Err(HubError::Offline),
    });

    assert_eq!(downloaded(&pane), None);
}

#[test]
fn download_finished_before_cancel_is_kept() {
    let mut pane = downloading("a.gguf", 100);
    pane.on_action(ModelsAction::Cancel(file("a.gguf")));

    pane.on_event(HubEvent::DownloadFinished {
        file: file("a.gguf"),
        result: Ok(ByteSize(100)),
    });

    assert_eq!(
        downloaded_row(&pane).accessory,
        Accessory::Installed {
            delete: button("Delete", &ModelsAction::Delete(file("a.gguf")))
        }
    );
}

#[test]
fn finished_download_offers_delete() {
    let mut pane = downloading("a.gguf", 100);

    pane.on_event(HubEvent::DownloadFinished {
        file: file("a.gguf"),
        result: Ok(ByteSize(120)),
    });

    assert_eq!(
        downloaded_row(&pane),
        Item {
            title: "a".to_owned(),
            subtitle: REPO.to_owned(),
            detail: "120 bytes".to_owned(),
            accessory: Accessory::Installed {
                delete: button("Delete", &ModelsAction::Delete(file("a.gguf")))
            },
        }
    );
}

#[test]
fn failed_download_explains_and_offers_retry() {
    let mut pane = downloading("a.gguf", 100);

    pane.on_event(HubEvent::DownloadFinished {
        file: file("a.gguf"),
        result: Err(HubError::Offline),
    });

    assert_eq!(
        downloaded_row(&pane),
        Item {
            title: "a".to_owned(),
            subtitle: "GhostType cannot connect to Hugging Face. Check your internet connection.".to_owned(),
            detail: "100 bytes".to_owned(),
            accessory: Accessory::Failed {
                retry: button("Try again", &ModelsAction::Download(sized("a.gguf", 100)))
            },
        }
    );
}

#[test]
fn failed_download_can_restart() {
    let mut pane = downloading("a.gguf", 100);
    pane.on_event(HubEvent::DownloadFinished {
        file: file("a.gguf"),
        result: Err(HubError::Storage),
    });

    let request = pane.on_action(ModelsAction::Download(sized("a.gguf", 100)));

    assert_eq!(request, Some(HubRequest::Download { file: file("a.gguf") }));
}

#[test]
fn finish_of_unknown_download_is_ignored() {
    let mut pane = ModelsPane::new();

    pane.on_event(HubEvent::DownloadFinished {
        file: file("a.gguf"),
        result: Ok(ByteSize(100)),
    });

    assert_eq!(downloaded(&pane), None);
}

#[test]
fn second_finish_does_not_change_downloaded_file() {
    let mut pane = installed("a.gguf", 100);

    pane.on_event(HubEvent::DownloadFinished {
        file: file("a.gguf"),
        result: Err(HubError::Offline),
    });

    assert_eq!(downloaded_row(&pane).detail, "100 bytes");
}

#[test]
fn hub_errors_explain_themselves() {
    let cases = [
        (HubError::AccessDenied, "This model needs a Hugging Face account."),
        (HubError::NotFound, "Hugging Face cannot find this model."),
        (
            HubError::Server,
            "Hugging Face is not available. Try again later.",
        ),
        (
            HubError::InvalidResponse,
            "Hugging Face sent an incorrect response. Try again later.",
        ),
        (
            HubError::Storage,
            "GhostType cannot save the model. Make sure that the disk has free space.",
        ),
        (HubError::Canceled, "You canceled the download."),
    ];

    let messages = cases.map(|(error, _)| browse(&failed_search(error)).rows);

    assert_eq!(
        messages,
        cases.map(|(_, text)| message(
            text,
            Some(button("Try again", &ModelsAction::Search("x".to_owned())))
        ))
    );
}

#[test]
fn delete_action_requests_delete_and_forgets_file() {
    let mut pane = installed("a.gguf", 100);

    let request = pane.on_action(ModelsAction::Delete(file("a.gguf")));

    assert_eq!(
        (request, downloaded(&pane)),
        (Some(HubRequest::Delete { file: file("a.gguf") }), None)
    );
}

#[test]
fn delete_of_running_download_is_ignored() {
    let mut pane = downloading("a.gguf", 100);

    let request = pane.on_action(ModelsAction::Delete(file("a.gguf")));

    assert_eq!(request, None);
}

#[test]
fn nothing_downloaded_hides_downloaded_list() {
    let pane = ModelsPane::new();

    assert_eq!(downloaded(&pane), None);
}

#[test]
fn installed_files_are_listed_by_repo_then_name() {
    let mut pane = ModelsPane::new();
    let other = SizedFile {
        file: ModelFile {
            repo: "another/Repo".to_owned(),
            name: "z.gguf".to_owned(),
        },
        size: ByteSize(1),
    };

    pane.on_event(HubEvent::Installed(vec![
        sized("b.gguf", 1),
        other,
        sized("a.gguf", 1),
    ]));

    let shown = downloaded(&pane).expect("files are installed");
    assert_eq!(titles(items(shown)), ["z", "a", "b"]);
}

#[test]
fn only_complete_downloads_are_installed_files() {
    let mut pane = downloading("b.gguf", 100);
    pane.on_event(HubEvent::Installed(vec![sized("a.gguf", 1)]));

    let installed = pane.installed_files();

    assert_eq!(installed, [file("a.gguf")]);
}

#[test]
fn installed_list_forgets_files_deleted_elsewhere() {
    let mut pane = installed("a.gguf", 100);

    pane.on_event(HubEvent::Installed(Vec::new()));

    assert_eq!(downloaded(&pane), None);
}

#[test]
fn installed_list_keeps_running_download() {
    let mut pane = downloading("a.gguf", 100);

    pane.on_event(HubEvent::Installed(vec![sized("a.gguf", 40)]));

    assert!(matches!(
        downloaded_row(&pane).accessory,
        Accessory::Downloading { .. }
    ));
}

#[test]
fn installed_list_replaces_failed_download() {
    let mut pane = downloading("a.gguf", 100);
    pane.on_event(HubEvent::DownloadFinished {
        file: file("a.gguf"),
        result: Err(HubError::Offline),
    });

    pane.on_event(HubEvent::Installed(vec![sized("a.gguf", 100)]));

    assert!(matches!(
        downloaded_row(&pane).accessory,
        Accessory::Installed { .. }
    ));
}

#[test]
fn search_action_requests_trimmed_query() {
    let mut pane = ModelsPane::new();

    let request = pane.on_action(ModelsAction::Search("  qwen ".to_owned()));

    assert_eq!(
        request,
        Some(HubRequest::Search {
            query: "qwen".to_owned()
        })
    );
}

#[test]
fn running_search_replaces_suggestions_with_loading_results() {
    let mut pane = ModelsPane::new();

    pane.on_action(ModelsAction::Search("qwen".to_owned()));

    let shown = browse(&pane);
    assert_eq!(
        (shown.heading, shown.rows),
        (
            Some("Results for “qwen”".to_owned()),
            vec![Row::Loading("Searching…")]
        )
    );
}

#[test]
fn blank_search_returns_to_suggestions() {
    let mut pane = searched("qwen", vec![repo(REPO, 1, false)]);

    let request = pane.on_action(ModelsAction::Search(" ".to_owned()));

    assert_eq!(
        (request, browse(&pane).heading),
        (None, Some("Suggested models".to_owned()))
    );
}

#[test]
fn search_result_shows_repo_owner_and_downloads() {
    let pane = searched("qwen", vec![repo(REPO, 1_234_567, false)]);

    let rows = browse_rows(&pane);

    assert_eq!(
        rows,
        [Item {
            title: "Model-GGUF".to_owned(),
            subtitle: "by owner".to_owned(),
            detail: "1.2M downloads".to_owned(),
            accessory: Accessory::Open(button("Show files", &ModelsAction::ShowFiles(REPO.to_owned()))),
        }]
    );
}

#[test]
fn search_results_explain_their_order() {
    let shown = browse(&searched("qwen", vec![repo(REPO, 1, false)]));

    assert_eq!(
        (shown.heading, shown.footer),
        (
            Some("Results for “qwen”".to_owned()),
            Some("The list shows only GGUF models. The models with the most downloads are at the top.")
        )
    );
}

#[test]
fn download_counts_are_abbreviated() {
    let counts = [999, 1_000, 4_581, 80_347, 999_950, 1_229_091, 2_000_000_000];

    let details = counts.map(|count| {
        browse_rows(&searched("q", vec![repo(REPO, count, false)]))
            .remove(0)
            .detail
    });

    assert_eq!(
        details,
        [
            "999 downloads",
            "1K downloads",
            "4.6K downloads",
            "80.3K downloads",
            "1M downloads",
            "1.2M downloads",
            "2B downloads"
        ]
    );
}

#[test]
fn gated_repo_cannot_be_opened() {
    let pane = searched("gemma", vec![repo(REPO, 5, true)]);

    let row = browse_rows(&pane).remove(0);

    assert_eq!(
        (row.subtitle.as_str(), row.accessory),
        ("by owner · Needs a Hugging Face account", Accessory::Unavailable)
    );
}

#[test]
fn search_without_results_says_so() {
    let pane = searched("nothing", Vec::new());

    assert_eq!(browse(&pane).rows, message("No models match “nothing”.", None));
}

#[test]
fn result_of_older_query_is_dropped() {
    let mut pane = ModelsPane::new();
    pane.on_action(ModelsAction::Search("old".to_owned()));
    pane.on_action(ModelsAction::Search("new".to_owned()));

    pane.on_event(HubEvent::SearchFinished {
        query: "old".to_owned(),
        result: Ok(vec![repo(REPO, 1, false)]),
    });

    assert_eq!(browse(&pane).rows, vec![Row::Loading("Searching…")]);
}

#[test]
fn show_files_requests_listing() {
    let mut pane = searched("model", vec![repo(REPO, 1, false)]);

    let request = pane.on_action(ModelsAction::ShowFiles(REPO.to_owned()));

    assert_eq!(
        request,
        Some(HubRequest::ListFiles {
            repo: REPO.to_owned()
        })
    );
}

#[test]
fn opened_repo_takes_over_the_browse_list() {
    let shown = browse(&opening_repo());

    assert_eq!(
        shown,
        Section {
            heading: Some(REPO.to_owned()),
            back: Some(button("Results", &ModelsAction::BackToResults)),
            rows: vec![Row::Loading("Loading the files…")],
            footer: Some(
                "Smaller files are faster. Larger files give better suggestions. If you are not sure, select a Q4_K_M file."
            ),
        }
    );
}

#[test]
fn show_files_without_results_is_ignored() {
    let mut pane = ModelsPane::new();
    pane.on_action(ModelsAction::Search("model".to_owned()));

    let request = pane.on_action(ModelsAction::ShowFiles(REPO.to_owned()));

    assert_eq!(request, None);
}

#[test]
fn listed_file_offers_download_with_size() {
    let pane = browsing(vec![sized("Model-Q4_K_M.gguf", 2_000_000)]);

    assert_eq!(
        browse_rows(&pane),
        [Item {
            title: "Model-Q4_K_M".to_owned(),
            subtitle: String::new(),
            detail: "2 MB".to_owned(),
            accessory: Accessory::Download(button(
                "Download",
                &ModelsAction::Download(sized("Model-Q4_K_M.gguf", 2_000_000))
            )),
        }]
    );
}

#[test]
fn listed_files_keep_single_file_language_models_smallest_first() {
    let pane = browsing(vec![
        sized("Model-Q8_0.gguf", 8),
        sized("README.md", 1),
        sized("mmproj-Model-F16.gguf", 2),
        sized("mtp-Model-Q4_0.gguf", 2),
        sized("Model-Q4_K_M.gguf", 4),
        sized("Model-F16-00001-of-00002.gguf", 3),
    ]);

    assert_eq!(titles(browse_rows(&pane)), ["Model-Q4_K_M", "Model-Q8_0"]);
}

#[test]
fn name_resembling_split_part_is_kept() {
    let pane = browsing(vec![sized("Best-of-Breed-Q4.gguf", 1)]);

    assert_eq!(browse_rows(&pane).len(), 1);
}

#[test]
fn repo_without_usable_files_says_so() {
    let pane = browsing(vec![sized("README.md", 1)]);

    assert_eq!(
        browse(&pane).rows,
        message("This model has no GGUF files that GhostType can use.", None)
    );
}

#[test]
fn failed_listing_offers_retry() {
    let mut pane = opening_repo();

    pane.on_event(HubEvent::FilesListed {
        repo: REPO.to_owned(),
        result: Err(HubError::NotFound),
    });

    assert_eq!(
        browse(&pane).rows,
        message(
            "Hugging Face cannot find this model.",
            Some(button("Try again", &ModelsAction::ShowFiles(REPO.to_owned())))
        )
    );
}

#[test]
fn retried_listing_loads_again() {
    let mut pane = opening_repo();
    pane.on_event(HubEvent::FilesListed {
        repo: REPO.to_owned(),
        result: Err(HubError::Offline),
    });

    let request = pane.on_action(ModelsAction::ShowFiles(REPO.to_owned()));

    assert_eq!(
        (request, browse(&pane).rows),
        (
            Some(HubRequest::ListFiles {
                repo: REPO.to_owned()
            }),
            vec![Row::Loading("Loading the files…")]
        )
    );
}

#[test]
fn listing_of_other_repo_is_dropped() {
    let mut pane = opening_repo();

    pane.on_event(HubEvent::FilesListed {
        repo: "someone/Else".to_owned(),
        result: Ok(vec![sized("a.gguf", 1)]),
    });

    assert_eq!(browse(&pane).rows, vec![Row::Loading("Loading the files…")]);
}

#[test]
fn back_returns_to_results() {
    let mut pane = browsing(vec![sized("a.gguf", 1)]);

    pane.on_action(ModelsAction::BackToResults);

    let shown = browse(&pane);
    let back = shown.back.clone();
    assert_eq!(
        (back, titles(items(shown))),
        (None, vec!["Model-GGUF".to_owned()])
    );
}

#[test]
fn submitting_the_search_field_searches() {
    let pane = ModelsPane::new();
    let input = SettingsInput::Submitted {
        id: ControlId::new(SEARCH),
        text: "qwen".to_owned(),
    };

    let action = pane.action_for(&input);

    assert_eq!(action, Some(ModelsAction::Search("qwen".to_owned())));
}

#[test]
fn pressing_download_starts_download() {
    let pane = ModelsPane::new();
    let download = accessory_button(browse_rows(&pane).remove(0));

    let action = press(&pane, download);

    assert_eq!(
        action,
        Some(ModelsAction::Download(SUGGESTED_MODELS[0].sized_file()))
    );
}

#[test]
fn pressing_cancel_cancels_the_download() {
    let pane = downloading("a.gguf", 100);
    let cancel = accessory_button(downloaded_row(&pane));

    let action = press(&pane, cancel);

    assert_eq!(action, Some(ModelsAction::Cancel(file("a.gguf"))));
}

#[test]
fn pressing_delete_deletes_the_file() {
    let pane = installed("a.gguf", 100);
    let delete = accessory_button(downloaded_row(&pane));

    let action = press(&pane, delete);

    assert_eq!(action, Some(ModelsAction::Delete(file("a.gguf"))));
}

#[test]
fn pressing_show_files_opens_the_repo() {
    let pane = searched("qwen", vec![repo(REPO, 1, false)]);
    let show_files = accessory_button(browse_rows(&pane).remove(0));

    let action = press(&pane, show_files);

    assert_eq!(action, Some(ModelsAction::ShowFiles(REPO.to_owned())));
}

#[test]
fn pressing_back_returns_to_results() {
    let pane = browsing(vec![sized("a.gguf", 1)]);
    let back = browse(&pane).back.expect("the files list goes back");

    let action = press(&pane, back);

    assert_eq!(action, Some(ModelsAction::BackToResults));
}

#[test]
fn pressing_retry_after_a_failed_search_searches_again() {
    let pane = failed_search(HubError::Offline);
    let retry = retry_button(browse(&pane));

    let action = press(&pane, retry);

    assert_eq!(action, Some(ModelsAction::Search("x".to_owned())));
}

#[test]
fn pressing_a_button_that_the_pane_does_not_show_does_nothing() {
    let pane = ModelsPane::new();

    let action = press(&pane, button("Delete", &ModelsAction::Delete(file("a.gguf"))));

    assert_eq!(action, None);
}
