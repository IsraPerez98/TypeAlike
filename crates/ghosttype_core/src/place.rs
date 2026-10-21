//! Where the user types: the kind of writing that the app or the web page holds, and the fields
//! of an email that get no suggestions.

use crate::desktop::Surroundings;

/// The kind of writing in a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Email,
    Chat,
    Document,
    WebPage,
}

/// The format of the apps that GhostType knows, by the start of their app id in lowercase: the
/// bundle identifier on macOS and the program file name on Windows.
const APP_FORMATS: [(&str, Format); 59] = [
    ("com.apple.mail", Format::Email),
    ("com.microsoft.outlook", Format::Email),
    ("com.readdle.smartemail", Format::Email),
    ("com.airmailapp.airmail", Format::Email),
    ("org.mozilla.thunderbird", Format::Email),
    ("com.superhuman.", Format::Email),
    ("outlook.exe", Format::Email),
    ("olk.exe", Format::Email),
    ("hxoutlook.exe", Format::Email),
    ("thunderbird.exe", Format::Email),
    ("com.tinyspeck.slackmacgap", Format::Chat),
    ("com.microsoft.teams", Format::Chat),
    ("com.hnc.discord", Format::Chat),
    ("com.apple.mobilesms", Format::Chat),
    ("ru.keepcoder.telegram", Format::Chat),
    ("org.telegram.desktop", Format::Chat),
    ("net.whatsapp.whatsapp", Format::Chat),
    ("desktop.whatsapp", Format::Chat),
    ("org.whispersystems.signal-desktop", Format::Chat),
    ("slack.exe", Format::Chat),
    ("ms-teams.exe", Format::Chat),
    ("teams.exe", Format::Chat),
    ("discord.exe", Format::Chat),
    ("whatsapp.exe", Format::Chat),
    ("whatsapp.root.exe", Format::Chat),
    ("telegram.exe", Format::Chat),
    ("signal.exe", Format::Chat),
    ("com.apple.iwork.pages", Format::Document),
    ("com.microsoft.word", Format::Document),
    ("com.microsoft.onenote", Format::Document),
    ("com.apple.notes", Format::Document),
    ("com.apple.textedit", Format::Document),
    ("md.obsidian", Format::Document),
    ("notion.id", Format::Document),
    ("net.shinyfrog.bear", Format::Document),
    ("org.libreoffice.script", Format::Document),
    ("winword.exe", Format::Document),
    ("onenote.exe", Format::Document),
    ("notepad.exe", Format::Document),
    ("wordpad.exe", Format::Document),
    ("obsidian.exe", Format::Document),
    ("notion.exe", Format::Document),
    ("soffice.bin", Format::Document),
    ("com.apple.safari", Format::WebPage),
    ("com.google.chrome", Format::WebPage),
    ("org.chromium.chromium", Format::WebPage),
    ("com.microsoft.edgemac", Format::WebPage),
    ("org.mozilla.firefox", Format::WebPage),
    ("com.brave.browser", Format::WebPage),
    ("company.thebrowser.", Format::WebPage),
    ("com.vivaldi.vivaldi", Format::WebPage),
    ("com.operasoftware.opera", Format::WebPage),
    ("chrome.exe", Format::WebPage),
    ("msedge.exe", Format::WebPage),
    ("firefox.exe", Format::WebPage),
    ("brave.exe", Format::WebPage),
    ("vivaldi.exe", Format::WebPage),
    ("opera.exe", Format::WebPage),
    ("arc.exe", Format::WebPage),
];

/// The format of the web pages that GhostType knows, by their domain. A subdomain has the format
/// of its domain.
const DOMAIN_FORMATS: [(&str, Format); 17] = [
    ("mail.google.com", Format::Email),
    ("outlook.live.com", Format::Email),
    ("outlook.office.com", Format::Email),
    ("outlook.office365.com", Format::Email),
    ("mail.yahoo.com", Format::Email),
    ("mail.proton.me", Format::Email),
    ("app.fastmail.com", Format::Email),
    ("app.slack.com", Format::Chat),
    ("teams.microsoft.com", Format::Chat),
    ("teams.live.com", Format::Chat),
    ("discord.com", Format::Chat),
    ("web.whatsapp.com", Format::Chat),
    ("web.telegram.org", Format::Chat),
    ("messenger.com", Format::Chat),
    ("chat.google.com", Format::Chat),
    ("docs.google.com", Format::Document),
    ("notion.so", Format::Document),
];

/// The labels of the address and subject fields of an email, in lowercase and without a colon.
const ADDRESS_AND_SUBJECT_LABELS: [&str; 15] = [
    "to",
    "cc",
    "bcc",
    "subject",
    "to recipients",
    "cc recipients",
    "bcc recipients",
    "add a subject",
    "para",
    "cco",
    "asunto",
    "destinatarios",
    "agregar un asunto",
    "añadir un asunto",
    "agrega un asunto",
];

impl Format {
    /// The format of the web page in `surroundings`, else of its app. `None` for a place that
    /// GhostType does not know.
    pub(crate) fn of(surroundings: &Surroundings) -> Option<Self> {
        Self::of_place(surroundings.app_id.as_deref(), surroundings.domain.as_deref())
    }

    /// The format of the web page with `domain`, else of the app with `app_id`. `None` for a
    /// place that GhostType does not know.
    pub(crate) fn of_place(app_id: Option<&str>, domain: Option<&str>) -> Option<Self> {
        let domain = domain.map(str::to_lowercase);
        let app_id = app_id.map(str::to_lowercase);
        let domain_format = domain.as_deref().and_then(|domain| {
            DOMAIN_FORMATS
                .iter()
                .find(|(known, _)| is_same_or_subdomain(domain, known))
        });
        let app_format = || {
            let app_id = app_id.as_deref()?;
            APP_FORMATS.iter().find(|(start, _)| app_id.starts_with(start))
        };
        domain_format.or_else(app_format).map(|&(_, format)| format)
    }

    /// The name of the format for the model.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Chat => "chat",
            Self::Document => "document",
            Self::WebPage => "web page",
        }
    }
}

/// Whether the focused field is an address or the subject of an email. These fields get no
/// suggestions, because Tab moves the focus between them.
pub(crate) fn is_address_or_subject_field(surroundings: &Surroundings) -> bool {
    Format::of(surroundings) == Some(Format::Email)
        && field_labels(surroundings)
            .iter()
            .any(|label| ADDRESS_AND_SUBJECT_LABELS.contains(&comparable(label).as_str()))
}

/// The labels of the focused field that are not empty, each on one line and one time, from the
/// most specific: the caption, the name, the placeholder and the help text. Two labels are the
/// same when they differ only in case or in a colon at the end.
pub(crate) fn field_labels(surroundings: &Surroundings) -> Vec<String> {
    let labels = &surroundings.field_labels;
    let lines = [&labels.caption, &labels.name, &labels.placeholder, &labels.help]
        .into_iter()
        .flatten()
        .map(|label| label.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|label| !label.is_empty());
    let mut kept: Vec<String> = Vec::new();
    for label in lines {
        if kept.iter().all(|known| comparable(known) != comparable(&label)) {
            kept.push(label);
        }
    }
    kept
}

/// Returns `label` in lowercase, without the colon at its end.
fn comparable(label: &str) -> String {
    label.trim().trim_end_matches(':').trim_end().to_lowercase()
}

fn is_same_or_subdomain(domain: &str, known: &str) -> bool {
    domain
        .strip_suffix(known)
        .is_some_and(|start| start.is_empty() || start.ends_with('.'))
}

/// Where the user wrote a text: a web page in any browser, else an app. `Text` is `&str` for a
/// place that borrows the names, and `String` for a key of a table.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Place<Text = String> {
    /// The host of the web page.
    WebPage(Text),
    /// The id of the app.
    App(Text),
}

impl<'text> Place<&'text str> {
    /// The place of a text that the user wrote in the app with `app_id`, on the web page of
    /// `domain` if any.
    pub(crate) fn of(app_id: &'text str, domain: Option<&'text str>) -> Self {
        domain.map_or(Self::App(app_id), Self::WebPage)
    }

    /// The place of the field of `surroundings`, or `None` when it has no app id and no domain.
    pub(crate) fn of_field(surroundings: &'text Surroundings) -> Option<Self> {
        match (&surroundings.domain, &surroundings.app_id) {
            (Some(domain), _) => Some(Self::WebPage(domain)),
            (None, Some(app_id)) => Some(Self::App(app_id)),
            (None, None) => None,
        }
    }

    pub(crate) fn owned(self) -> Place {
        match self {
            Self::WebPage(domain) => Place::WebPage(domain.to_owned()),
            Self::App(app_id) => Place::App(app_id.to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::FieldLabels;

    fn place(app_id: Option<&str>, domain: Option<&str>, field_name: Option<&str>) -> Surroundings {
        Surroundings {
            app_id: app_id.map(str::to_owned),
            domain: domain.map(str::to_owned),
            field_labels: FieldLabels {
                name: field_name.map(str::to_owned),
                ..FieldLabels::default()
            },
            ..Surroundings::default()
        }
    }

    fn labeled(field_labels: FieldLabels) -> Surroundings {
        Surroundings {
            app_id: Some("com.apple.mail".to_owned()),
            field_labels,
            ..Surroundings::default()
        }
    }

    #[test]
    fn field_labels_go_from_the_caption_to_the_help_text() {
        let surroundings = labeled(FieldLabels {
            caption: Some("Comments:".to_owned()),
            name: Some("Your comments".to_owned()),
            placeholder: Some(" Tell us\n more ".to_owned()),
            help: Some("At most 500 characters".to_owned()),
        });

        let labels = field_labels(&surroundings);

        assert_eq!(
            labels,
            [
                "Comments:",
                "Your comments",
                "Tell us more",
                "At most 500 characters"
            ]
        );
    }

    #[test]
    fn field_labels_keep_one_of_each_label() {
        let surroundings = labeled(FieldLabels {
            caption: Some("Message:".to_owned()),
            name: Some("message".to_owned()),
            placeholder: Some(" \n ".to_owned()),
            help: Some("Message".to_owned()),
        });

        let labels = field_labels(&surroundings);

        assert_eq!(labels, ["Message:"]);
    }

    #[test]
    fn any_label_finds_an_address_or_subject_field() {
        let cases = [
            FieldLabels {
                placeholder: Some("Add a subject".to_owned()),
                ..FieldLabels::default()
            },
            FieldLabels {
                caption: Some("Cc:".to_owned()),
                name: Some("Carbon copy".to_owned()),
                ..FieldLabels::default()
            },
        ];

        let found = cases.map(|labels| is_address_or_subject_field(&labeled(labels)));

        assert_eq!(found, [true; 2]);
    }

    #[test]
    fn known_apps_have_a_format() {
        let cases = [
            ("com.apple.mail", Some(Format::Email)),
            ("OUTLOOK.EXE", Some(Format::Email)),
            ("com.tinyspeck.slackmacgap", Some(Format::Chat)),
            ("com.microsoft.teams2", Some(Format::Chat)),
            ("WINWORD.EXE", Some(Format::Document)),
            ("com.google.Chrome.canary", Some(Format::WebPage)),
            ("msedge.exe", Some(Format::WebPage)),
            ("com.example.Unknown", None),
        ];

        let formats = cases.map(|(app_id, _)| Format::of(&place(Some(app_id), None, None)));

        assert_eq!(formats, cases.map(|(_, format)| format));
    }

    #[test]
    fn web_page_gives_its_format_before_the_app() {
        let cases = [
            ("mail.google.com", Some(Format::Email)),
            ("app.slack.com", Some(Format::Chat)),
            ("ptb.discord.com", Some(Format::Chat)),
            ("www.notion.so", Some(Format::Document)),
            ("fakemail.google.com", Some(Format::WebPage)),
            ("example.com", Some(Format::WebPage)),
        ];

        let formats =
            cases.map(|(domain, _)| Format::of(&place(Some("com.google.Chrome"), Some(domain), None)));

        assert_eq!(formats, cases.map(|(_, format)| format));
    }

    #[test]
    fn place_without_app_or_web_page_has_no_format() {
        let format = Format::of(&place(None, None, Some("Message")));

        assert_eq!(format, None);
    }

    #[test]
    fn formats_have_names_for_the_model() {
        let formats = [Format::Email, Format::Chat, Format::Document, Format::WebPage];

        let names = formats.map(Format::name);

        assert_eq!(names, ["email", "chat", "document", "web page"]);
    }

    #[test]
    fn address_and_subject_fields_of_an_email_are_found() {
        let cases = [
            (Some("com.apple.mail"), None, "To:"),
            (Some("OUTLOOK.EXE"), None, " Cc "),
            (Some("com.microsoft.Outlook"), None, "Subject"),
            (
                Some("com.google.Chrome"),
                Some("mail.google.com"),
                "To recipients",
            ),
            (Some("com.google.Chrome"), Some("mail.google.com"), "Asunto"),
            (Some("olk.exe"), None, "Add a subject"),
        ];

        let found = cases
            .map(|(app_id, domain, label)| is_address_or_subject_field(&place(app_id, domain, Some(label))));

        assert_eq!(found, [true; 6]);
    }

    #[test]
    fn other_fields_are_not_address_or_subject_fields() {
        let cases = [
            (Some("com.apple.mail"), None, Some("Message")),
            (Some("com.apple.mail"), None, None),
            (Some("com.google.Chrome"), Some("example.com"), Some("Subject")),
            (Some("com.tinyspeck.slackmacgap"), None, Some("To")),
            (None, None, Some("Subject")),
        ];

        let found =
            cases.map(|(app_id, domain, label)| is_address_or_subject_field(&place(app_id, domain, label)));

        assert_eq!(found, [false; 5]);
    }
}
