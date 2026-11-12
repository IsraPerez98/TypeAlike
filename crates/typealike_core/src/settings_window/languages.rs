//! The Languages and typos pane of the settings window: the languages that the user writes, the
//! downloads of their dictionaries, and what suggestions do with typos.

use crate::hub::{ByteSize, HubError};
use crate::language::{closest_languages, language_of, same_tag};
use crate::localization::{MessageArgument, Translations};
use crate::settings::SpellingSettings;
use crate::settings_window::models::{permille, size_text};
use crate::settings_window::view::{
    Accessory, Button, ChoiceText, ControlId, Item, Page, PageIcon, Picker, Row, RowIcon, Section,
    SettingsInput,
};
use crate::spelling::{
    DictionaryEvent, DictionaryRequest, DownloadableDictionary, WRITING_LANGUAGES, is_main_entry,
    language_name, writing_language_of,
};

pub(crate) const ADD_LANGUAGE: &str = "language-add";
/// The start of the id of the button that removes a language, before the tag of the language.
const REMOVE_PREFIX: &str = "language-remove:";
/// The start of the id of the button that stops the download of a dictionary.
const CANCEL_PREFIX: &str = "language-cancel:";
/// The start of the id of the button that starts a failed download again.
const RETRY_PREFIX: &str = "language-retry:";

/// Something that the user did in the Languages and typos pane, or in the step of the setup that
/// shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LanguagesAction {
    /// Added the language with this tag. Its dictionary downloads when the OS has none.
    Add(String),
    /// Removed the language with this tag, or stopped the download of its dictionary.
    Remove(String),
    /// Started the failed download of the dictionary of the language with this tag again.
    RetryDownload(String),
}

/// The languages of the OS: the languages of the user in its settings, and the languages that it
/// has a dictionary for.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OsLanguages {
    /// The languages of the user in the settings of the OS, such as `es-CL`, most preferred
    /// first.
    pub preferred: Vec<String>,
    /// The languages that the OS has a dictionary for, such as `en-US` or `en_GB`.
    pub available: Vec<String>,
}

/// What the controller must do after a change of the pane.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LanguagesOutcome {
    /// Whether the settings of the pane changed, so that they must be kept.
    pub is_settings_change: bool,
    /// Whether the dictionaries that check the words of suggestions changed, so that the
    /// controller must open them again ([`LanguagesPane::dictionaries`]).
    pub is_dictionary_change: bool,
    /// The work for the dictionary hub.
    pub requests: Vec<DictionaryRequest>,
}

/// The dictionaries that check the words of suggestions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Dictionaries {
    /// The languages of the OS whose dictionaries the OS opens, as the OS names them.
    pub os: Vec<String>,
    /// The languages whose downloaded dictionaries the hub opens.
    pub downloaded: Vec<String>,
}

/// The state of the download of the dictionary of a language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Download {
    Running { received: ByteSize },
    Failed(HubError),
}

/// State of the Languages and typos pane: the languages that the user writes and the
/// dictionaries that check them. The OS gives the dictionaries that it has, and the hub
/// downloads the others.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguagesPane {
    settings: SpellingSettings,
    os: OsLanguages,
    catalog: Vec<DownloadableDictionary>,
    /// The languages of the downloaded dictionaries, after the hub listed them.
    installed: Option<Vec<String>>,
    /// The languages of the downloaded dictionaries that the hub read, so that it can check them.
    loaded: Vec<String>,
    downloads: Vec<(String, Download)>,
}

impl LanguagesPane {
    /// Creates a pane that shows `settings`, with the languages of the `os` and the `catalog` of
    /// the dictionaries that the hub can download.
    #[must_use]
    pub fn new(settings: SpellingSettings, os: OsLanguages, catalog: Vec<DownloadableDictionary>) -> Self {
        Self {
            settings,
            os,
            catalog,
            installed: None,
            loaded: Vec::new(),
            downloads: Vec::new(),
        }
    }

    /// The settings that the pane shows.
    #[must_use]
    pub fn settings(&self) -> &SpellingSettings {
        &self.settings
    }

    /// Gives the pane the settings of the first start. The downloaded dictionaries stay.
    pub fn reset(&mut self, settings: SpellingSettings) -> LanguagesOutcome {
        self.settings = settings;
        LanguagesOutcome {
            is_settings_change: false,
            is_dictionary_change: true,
            requests: self.requests_for_languages(),
        }
    }

    /// The work for the hub when TypeAlike starts: the list of the downloaded dictionaries, when
    /// the hub can download any.
    #[must_use]
    pub fn start(&self) -> Vec<DictionaryRequest> {
        if self.catalog.is_empty() {
            Vec::new()
        } else {
            vec![DictionaryRequest::ListInstalled]
        }
    }

    /// The languages that the user writes: the languages that the user picked, else the languages
    /// of the user in the settings of the OS that the OS has a dictionary for.
    #[must_use]
    pub fn languages(&self) -> Vec<String> {
        if let Some(languages) = &self.settings.languages {
            return languages.clone();
        }
        let mut languages = Vec::<String>::new();
        let os_languages = self
            .os
            .preferred
            .iter()
            .filter_map(|tag| self.os_writing_language(tag));
        for language in os_languages {
            if !languages.iter().any(|known| same_tag(known, language)) {
                languages.push(language.to_owned());
            }
        }
        languages
    }

    /// The dictionaries that check the words of suggestions: the downloaded dictionary of a
    /// language when the hub read it, else the dictionary of the OS for that language, else the
    /// dictionary of the OS for another region of that language.
    #[must_use]
    pub fn dictionaries(&self) -> Dictionaries {
        let mut dictionaries = Dictionaries::default();
        for language in self.languages() {
            if self.loaded.iter().any(|loaded| same_tag(loaded, &language)) {
                dictionaries.downloaded.push(language);
            } else if let Some(os) = self
                .os_dictionary(&language)
                .or_else(|| self.os_fallback(&language))
                && !dictionaries.os.contains(&os)
            {
                dictionaries.os.push(os);
            }
        }
        dictionaries
    }

    /// Handles `event` of the hub.
    pub fn on_event(&mut self, event: DictionaryEvent) -> LanguagesOutcome {
        match event {
            DictionaryEvent::Installed(languages) => {
                self.installed = Some(languages);
                LanguagesOutcome {
                    requests: self.requests_for_languages(),
                    ..LanguagesOutcome::default()
                }
            }
            DictionaryEvent::Loaded { language, result } => {
                if let Err(error) = result {
                    self.set_download(&language, Some(Download::Failed(error)));
                    return LanguagesOutcome::default();
                }
                if !self.loaded.contains(&language) {
                    self.loaded.push(language);
                }
                LanguagesOutcome {
                    is_dictionary_change: true,
                    ..LanguagesOutcome::default()
                }
            }
            DictionaryEvent::DownloadProgress { language, received } => {
                if self.download(&language).is_some() {
                    self.set_download(&language, Some(Download::Running { received }));
                }
                LanguagesOutcome::default()
            }
            DictionaryEvent::DownloadFinished { language, result } => {
                self.on_download_finished(language, result)
            }
        }
    }

    fn on_download_finished(
        &mut self,
        language: String,
        result: Result<ByteSize, HubError>,
    ) -> LanguagesOutcome {
        match result {
            Ok(_) => {
                self.set_download(&language, None);
                self.installed.get_or_insert_with(Vec::new).push(language.clone());
                LanguagesOutcome {
                    requests: vec![DictionaryRequest::Load { language }],
                    ..LanguagesOutcome::default()
                }
            }
            Err(HubError::Canceled) => {
                self.set_download(&language, None);
                LanguagesOutcome::default()
            }
            Err(error) => {
                if self.download(&language).is_some() {
                    self.set_download(&language, Some(Download::Failed(error)));
                }
                LanguagesOutcome::default()
            }
        }
    }

    /// Handles `action`.
    pub fn on_action(&mut self, action: LanguagesAction) -> LanguagesOutcome {
        match action {
            LanguagesAction::Add(language) => self.add(&language),
            LanguagesAction::Remove(language) => self.remove(&language),
            LanguagesAction::RetryDownload(language) => LanguagesOutcome {
                requests: self.download_request(&language).into_iter().collect(),
                ..LanguagesOutcome::default()
            },
        }
    }

    fn add(&mut self, language: &str) -> LanguagesOutcome {
        let mut languages = self.languages();
        if languages.iter().any(|known| same_tag(known, language)) {
            return LanguagesOutcome::default();
        }
        languages.push(language.to_owned());
        self.settings.languages = Some(languages);
        LanguagesOutcome {
            is_settings_change: true,
            is_dictionary_change: true,
            requests: self.request_for(language).into_iter().collect(),
        }
    }

    fn remove(&mut self, language: &str) -> LanguagesOutcome {
        let languages = self
            .languages()
            .into_iter()
            .filter(|known| !same_tag(known, language))
            .collect();
        self.settings.languages = Some(languages);
        let mut requests = Vec::new();
        if self
            .download(language)
            .is_some_and(|download| matches!(download, Download::Running { .. }))
        {
            requests.push(DictionaryRequest::CancelDownload {
                language: language.to_owned(),
            });
        }
        self.set_download(language, None);
        if self.is_installed(language) {
            requests.push(DictionaryRequest::Delete {
                language: language.to_owned(),
            });
        }
        if let Some(installed) = &mut self.installed {
            installed.retain(|known| !same_tag(known, language));
        }
        self.loaded.retain(|known| !same_tag(known, language));
        LanguagesOutcome {
            is_settings_change: true,
            is_dictionary_change: true,
            requests,
        }
    }

    /// The work for the hub for each language that needs a downloaded dictionary: a read of a
    /// downloaded dictionary, and a download of a missing one in the list that the user picked.
    /// Before the hub listed its dictionaries, there is none.
    fn requests_for_languages(&mut self) -> Vec<DictionaryRequest> {
        self.languages()
            .iter()
            .filter_map(|language| self.request_for(language))
            .collect()
    }

    /// The work for the hub for `language`, when it needs a downloaded dictionary that the hub did
    /// not read: a read of the downloaded dictionary, else a download, unless one runs.
    fn request_for(&mut self, language: &str) -> Option<DictionaryRequest> {
        if !self.needs_download(language) || self.loaded.iter().any(|loaded| same_tag(loaded, language)) {
            return None;
        }
        if self.is_installed(language) {
            return Some(DictionaryRequest::Load {
                language: language.to_owned(),
            });
        }
        if self.installed.is_none() || self.settings.languages.is_none() || self.download(language).is_some()
        {
            return None;
        }
        self.download_request(language)
    }

    fn download_request(&mut self, language: &str) -> Option<DictionaryRequest> {
        self.catalog_size(language)?;
        self.set_download(
            language,
            Some(Download::Running {
                received: ByteSize(0),
            }),
        );
        Some(DictionaryRequest::Download {
            language: language.to_owned(),
        })
    }

    /// Whether `language` needs a dictionary of the hub: the OS has no dictionary of its own for
    /// it, and the hub can download one.
    fn needs_download(&self, language: &str) -> bool {
        self.os_dictionary(language).is_none() && self.catalog_size(language).is_some()
    }

    /// The dictionary of the OS that serves `language` as its own: the dictionary with the same
    /// tag, else for the main entry of a language, the closest dictionary of that language.
    fn os_dictionary(&self, language: &str) -> Option<String> {
        let available = &self.os.available;
        available
            .iter()
            .find(|tag| same_tag(tag, language))
            .cloned()
            .or_else(|| {
                is_main_entry(language)
                    .then(|| self.os_fallback(language))
                    .flatten()
            })
    }

    /// The closest dictionary of the OS for the language of `language`, in any region.
    fn os_fallback(&self, language: &str) -> Option<String> {
        closest_languages(&[language.to_owned()], &self.os.available)
            .into_iter()
            .next()
    }

    /// The entry of [`WRITING_LANGUAGES`] for `tag`, a language of the user in the settings of the
    /// OS, that a dictionary of the OS serves: the entry of `tag`, else the main entry of its
    /// language.
    fn os_writing_language(&self, tag: &str) -> Option<&'static str> {
        let own = writing_language_of(tag).filter(|entry| self.os_dictionary(entry).is_some());
        own.or_else(|| {
            let language = language_of(tag);
            WRITING_LANGUAGES
                .iter()
                .map(|(entry, _)| *entry)
                .find(|entry| language_of(entry).eq_ignore_ascii_case(language))
                .filter(|entry| self.os_dictionary(entry).is_some())
        })
    }

    fn catalog_size(&self, language: &str) -> Option<ByteSize> {
        self.catalog
            .iter()
            .find(|dictionary| same_tag(&dictionary.language, language))
            .map(|dictionary| dictionary.size)
    }

    fn is_installed(&self, language: &str) -> bool {
        self.installed
            .as_ref()
            .is_some_and(|installed| installed.iter().any(|known| same_tag(known, language)))
    }

    fn download(&self, language: &str) -> Option<Download> {
        self.downloads
            .iter()
            .find(|(known, _)| same_tag(known, language))
            .map(|(_, download)| *download)
    }

    fn set_download(&mut self, language: &str, download: Option<Download>) {
        self.downloads.retain(|(known, _)| !same_tag(known, language));
        if let Some(download) = download {
            self.downloads.push((language.to_owned(), download));
        }
    }

    /// The languages that the menu "Add a language" offers, sorted by name: the languages that the
    /// user does not write yet, with a dictionary of the OS or of the hub.
    fn offered_languages(&self) -> Vec<&'static str> {
        let written = self.languages();
        let mut offered = WRITING_LANGUAGES
            .iter()
            .filter(|(tag, _)| !written.iter().any(|known| same_tag(known, tag)))
            .filter(|(tag, _)| self.os_dictionary(tag).is_some() || self.catalog_size(tag).is_some())
            .collect::<Vec<_>>();
        offered.sort_by_key(|(_, name)| name_order(name));
        offered.into_iter().map(|(tag, _)| *tag).collect()
    }

    /// Returns the action of `input`, when `input` is for a control of the pane.
    #[must_use]
    pub fn action_for(&self, input: &SettingsInput) -> Option<LanguagesAction> {
        match input {
            SettingsInput::Picked { id, index } if id.as_str() == ADD_LANGUAGE => {
                let language = self.offered_languages().get(index.checked_sub(1)?)?.to_string();
                Some(LanguagesAction::Add(language))
            }
            SettingsInput::Pressed(id) => {
                let id = id.as_str();
                id.strip_prefix(REMOVE_PREFIX)
                    .or_else(|| id.strip_prefix(CANCEL_PREFIX))
                    .map(|language| LanguagesAction::Remove(language.to_owned()))
                    .or_else(|| {
                        id.strip_prefix(RETRY_PREFIX)
                            .map(|language| LanguagesAction::RetryDownload(language.to_owned()))
                    })
            }
            _ => None,
        }
    }

    /// Returns the page of the pane: the languages, then the switches of the `typo_rows` of the
    /// General pane.
    #[must_use]
    pub fn page(&self, typo_rows: Vec<Row>, translations: &Translations) -> Page {
        Page {
            title: translations.text("languages-title"),
            icon: PageIcon::Languages,
            summary: translations.text("languages-summary"),
            search: None,
            sections: vec![
                self.languages_section(translations),
                Section {
                    heading: Some(translations.text("languages-typos-heading")),
                    back: None,
                    rows: typo_rows,
                    footer: None,
                },
            ],
        }
    }

    /// Returns the section of the languages that the user writes, with the menu that adds one, for
    /// the page and the setup.
    #[must_use]
    pub fn languages_section(&self, translations: &Translations) -> Section {
        let languages = self.languages();
        let items = languages
            .iter()
            .map(|language| Row::Item(self.language_item(language, translations)));
        let add = Row::Picker(self.add_picker(translations));
        Section {
            heading: Some(translations.text("languages-heading")),
            back: None,
            rows: items.chain([add]).collect(),
            footer: None,
        }
    }

    fn language_item(&self, language: &str, translations: &Translations) -> Item {
        let title = language_name(language).unwrap_or(language).to_owned();
        let size = self.catalog_size(language);
        let button = |prefix: &str, label: &str| Button {
            id: ControlId::from(format!("{prefix}{language}")),
            label: translations.text(label),
            is_enabled: true,
        };
        let remove = || Accessory::Action(button(REMOVE_PREFIX, "languages-remove"));
        let (subtitle, accessory) = match self.download(language) {
            Some(Download::Running { received }) => {
                let progress = permille(received, size.unwrap_or(ByteSize(0)));
                let percent = MessageArgument::Number(u32::from(progress.0 / 10));
                (
                    translations.text_with("models-download-progress", &[("percent", percent)]),
                    Accessory::Downloading {
                        progress,
                        cancel: button(CANCEL_PREFIX, "models-cancel-download"),
                    },
                )
            }
            Some(Download::Failed(error)) => (
                translations.text(error_message(error)),
                Accessory::Failed {
                    retry: button(RETRY_PREFIX, "models-try-again"),
                },
            ),
            None if self.has_dictionary(language) => (String::new(), remove()),
            None => (translations.text("languages-no-dictionary"), remove()),
        };
        let detail = size
            .filter(|_| self.needs_download(language))
            .map(|size| size_text(size, translations))
            .unwrap_or_default();
        Item {
            title,
            subtitle,
            detail,
            accessory,
        }
    }

    /// Whether `language` has a dictionary: the dictionary of the OS for it, or a dictionary that
    /// the hub can download. A dictionary that downloads, or that failed to download, shows its
    /// own state.
    fn has_dictionary(&self, language: &str) -> bool {
        self.os_dictionary(language).is_some() || self.catalog_size(language).is_some()
    }

    fn add_picker(&self, translations: &Translations) -> Picker {
        let names = self
            .offered_languages()
            .into_iter()
            .map(|tag| language_name(tag).unwrap_or(tag).to_owned());
        Picker {
            id: ControlId::new(ADD_LANGUAGE),
            icon: RowIcon::Globe,
            label: translations.text("languages-add"),
            description: if self.catalog.is_empty() {
                String::new()
            } else {
                translations.text("languages-add-description-download")
            },
            choices: [translations.text("languages-add-choose")]
                .into_iter()
                .chain(names)
                .collect(),
            choice_text: ChoiceText::Name,
            selected: 0,
            is_enabled: true,
            note: None,
        }
    }
}

fn error_message(error: HubError) -> &'static str {
    match error {
        HubError::Offline => "languages-error-offline",
        HubError::Storage => "languages-error-storage",
        HubError::AccessDenied
        | HubError::NotFound
        | HubError::Server
        | HubError::InvalidResponse
        | HubError::Canceled => "languages-error-server",
    }
}

/// The key that sorts `name`, the name of a language, by its letters: each Latin letter in
/// lowercase and without its accent, so that "Čeština" comes after "Català" and before "Dansk".
fn name_order(name: &str) -> String {
    name.to_lowercase().chars().map(letter_without_accent).collect()
}

/// The letter of `letter`, a lowercase letter of Latin-1 or Latin Extended-A, without its accent.
/// Other characters stay.
fn letter_without_accent(letter: char) -> char {
    match letter {
        'à'..='å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'ď' | 'đ' => 'd',
        'è'..='ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'ĥ' | 'ħ' => 'h',
        'ì'..='ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => 'i',
        'ĵ' => 'j',
        'ķ' => 'k',
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => 'l',
        'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
        'ò'..='ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => 'o',
        'ŕ' | 'ŗ' | 'ř' => 'r',
        'ś' | 'ŝ' | 'ş' | 'š' => 's',
        'ţ' | 'ť' | 'ŧ' => 't',
        'ù'..='ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'ŵ' => 'w',
        'ý' | 'ÿ' | 'ŷ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        other => other,
    }
}

#[cfg(test)]
mod tests;
