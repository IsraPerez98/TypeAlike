## The tray menu on Windows, the menu bar menu on macOS and the first entry of the badge menu.

menu-pause = Vorschläge pausieren
menu-resume = Vorschläge fortsetzen
menu-settings = Einstellungen…
menu-quit = TypeAlike beenden

## The name of TypeAlike and its status, in the tooltip of the tray icon and for screen readers at the badge.

status-active = TypeAlike
status-paused = TypeAlike (pausiert)
status-off-here = TypeAlike (hier aus)
status-energy-saver = TypeAlike (aus, solange der Computer Energie spart)
status-game = TypeAlike (aus, während du spielst)
status-full-screen = TypeAlike (aus, solange eine App den ganzen Bildschirm füllt)

## The menu of the badge at the caret. $app is the name of an app, and $host is the host of a website, such as mail.example.com.

badge-app-suggestions = Vorschläge in { $app } anzeigen
badge-site-suggestions = Vorschläge auf { $host } anzeigen
badge-private-windows = Vorschläge in privaten Fenstern stoppen
badge-app-screen-text = Bildschirmtext in { $app } verwenden
badge-app-earlier-writing = Frühere Texte in { $app }

## The choices of the earlier writing of an app, in the Personalization pane and in the badge menu.

history-scope-off = Aus
history-scope-this-app = Nur diese App
history-scope-all-apps = Alle Apps

## The list of apps in the Apps and websites, Copied and screen text and Personalization panes.

app-list-heading = Apps
app-list-empty = In einer App wurde noch kein Vorschlag angezeigt.
app-list-footer = Wenn in einer App zum ersten Mal ein Vorschlag erscheint, fügt TypeAlike die App dieser Liste hinzu.

## The General pane of the settings window.

general-title = Allgemein
general-summary = Lege fest, wann TypeAlike startet und wie Vorschläge erscheinen.
general-autostart = Bei der Anmeldung öffnen
general-autostart-description = TypeAlike startet bei jeder Anmeldung, sodass du es nicht selbst starten musst.
general-language = Sprache
general-language-description = Die Fenster und Menüs von TypeAlike erscheinen in dieser Sprache. Vorschläge bleiben in der Sprache deines Textes.
# The first entry of the language menu. The other entries are the names of the languages, which stay the same in each language.
general-language-system = Systemsprache
general-display = Wörter einzeln anzeigen
general-display-description = Jedes Wort eines Vorschlags erscheint, sobald das Modell es schreibt. Das erste Wort erscheint so früher, aber der Vorschlag wächst, während du ihn liest.
general-typo-rule = Vorschläge bei Tippfehlern ausblenden
general-typo-rule-description = Wenn ein Wort einen Tippfehler zu haben scheint, vervollständigt kein Vorschlag das Wort, damit die Tab-Taste den Tippfehler nicht einfügt. Auch ein seltener Name kann wie ein Tippfehler aussehen und bekommt dann keinen Vorschlag.
general-typo-fix = Tippfehler korrigieren
general-typo-fix-description = Wenn ein Wort einen Tippfehler zu haben scheint, zeigt der Vorschlag das Wort durchgestrichen und danach das korrigierte Wort. Die Tab-Taste fügt das korrigierte Wort ein.
general-length = Länge der Vorschläge
general-length-description = Ein Vorschlag endet bei dieser Länge. Ein kürzerer Vorschlag erscheint früher.
general-length-very-short = Sehr kurz
general-length-short = Kurz
general-length-medium = Mittel
general-length-long = Lang
general-length-very-long = Sehr lang
general-length-processor-note = Wenn das Modell auf dem Prozessor läuft, sind Vorschläge kurz oder sehr kurz, damit sie früher erscheinen.
general-sentence-end = Am Satzende stoppen
general-sentence-end-description = Ein Vorschlag endet nach dem ersten Satz, den er abschließt.
general-sentence-end-off-note = Nach dem ersten Satz sind Vorschläge oft falsch. Schalte diese Option ein, damit sie dort stoppen.
general-reset = Alle Einstellungen zurücksetzen
general-reset-description = Alle Einstellungen erhalten wieder ihre Standardwerte, und die Einrichtung öffnet sich erneut. Deine heruntergeladenen Modelle und deine früheren Texte bleiben erhalten.
general-reset-confirm = Zum Zurücksetzen erneut drücken

## The menu of the suggestion model and its status, in the General, Models and Playground panes.

model-menu = Vorschlagsmodell
model-menu-select = Modell auswählen
model-menu-none = Keine heruntergeladenen Modelle
model-needed = TypeAlike braucht ein Modell, um Vorschläge zu schreiben. Lade unten das empfohlene Modell herunter.
model-recommended = Empfohlen für diesen Computer
model-recommendation-unknown-gpu = TypeAlike kann den Speicher der Grafikkarte nicht lesen und empfiehlt daher ein Modell, das für die meisten Grafikkarten geeignet ist.
model-starting = Modell wird gestartet…
model-paused = Vorschläge sind pausiert. Um sie fortzusetzen, wähle „Vorschläge fortsetzen“ im Menü von TypeAlike.
model-energy-saver = Vorschläge sind aus, solange der Computer Energie spart. Um sie jetzt zu verwenden, schalte „Beim Energiesparen stoppen“ aus.
model-game = Vorschläge wurden gestoppt, um Speicher für ein Spiel freizugeben. Sie starten wieder, wenn du in ein Textfeld tippst.
model-full-screen = Vorschläge wurden gestoppt, um Speicher für eine App im Vollbild freizugeben. Sie starten wieder, wenn du in ein Textfeld tippst.
model-idle = Vorschläge wurden gestoppt, um Speicher freizugeben, weil du eine Zeit lang weder getippt noch geklickt hast. Sie starten wieder, wenn du in ein Textfeld tippst.
model-error-missing = TypeAlike findet die Modelldatei nicht. Lade das Modell erneut herunter.
model-error-invalid = TypeAlike kann dieses Modell nicht verwenden. Die Datei ist vielleicht beschädigt oder hat ein Format, das TypeAlike nicht unterstützt. Lade sie erneut herunter oder wähle ein anderes Modell.
model-error-too-large = Das Modell passt nicht in den freien Speicher der Grafikkarte. Wähle ein kleineres Modell.
model-error-hardware = TypeAlike kann das Modell auf dieser Grafikkarte nicht ausführen. Wähle unter „Modell ausführen auf“ eine andere Option.
model-error-no-model = Wähle zuerst ein Modell.
model-error-text-too-long = Der Text ist für dieses Modell zu lang. Verwende einen kürzeren Text.
model-error-failed = TypeAlike konnte keinen Vorschlag schreiben. Versuche es erneut.
model-error-superseded = TypeAlike hat diesen Vorschlag gestoppt, um einen neueren zu schreiben.

## The Playground pane of the settings window. $seconds is the time that the model took to write the suggestion, with two digits after the decimal separator, such as 0.42.

playground-title = Testbereich
playground-summary = Wähle ein Modell und tippe den Anfang eines Satzes, um zu sehen, was das Modell vorschlägt.
playground-text = Text
playground-text-placeholder = Tippe den Anfang eines Satzes
playground-suggest = Vorschlag anzeigen
playground-output = Vorschlag
playground-writing = Vorschlag wird geschrieben…
playground-nothing-added = Das Modell hat keinen Vorschlag für diesen Text. Tippe weitere Wörter und versuche es erneut.
playground-time = In { $seconds } Sekunden geschrieben

## The Models pane of the settings window. $developer is the company that made a model, such as Google. $owner is the account on Hugging Face that shares a model. $query is the text that the user searched for.

models-title = Modelle
models-summary = Ein Modell auf diesem Computer schreibt die Vorschläge. Der Text, den du in anderen Apps tippst, verlässt diesen Computer nie. Die Suche und das Herunterladen von Modellen verwenden das Internet.
models-model-description = Ein kleineres Modell ist schneller und braucht weniger Speicher, aber seine Vorschläge sind öfter falsch.
models-hardware = Modell ausführen auf
models-hardware-description = Eine Grafikkarte führt das Modell schneller aus als der Prozessor, aber das ganze Modell muss in ihren Speicher passen.
models-hardware-processor = Prozessor (CPU)
models-hardware-graphics-card = Grafikkarte (Vulkan)
models-hardware-nvidia = NVIDIA-Grafikkarte (CUDA)
models-search = Auf Hugging Face suchen
models-search-placeholder = Name eines Modells
models-search-button = Suchen
models-recommended-heading = Empfohlene Modelle
models-recommended-footer = Wenn du unsicher bist, lade das Modell herunter, das TypeAlike für diesen Computer empfiehlt. Du kannst ein Modell jederzeit löschen.
models-developer = Von { $developer }
models-developer-recommended = Von { $developer } · Empfohlen für diesen Computer
models-searching = Suche auf Hugging Face…
models-results-heading = Ergebnisse für „{ $query }“
models-results-footer = Die Liste zeigt nur die Modelle, die TypeAlike verwenden kann. Die Modelle mit den meisten Downloads stehen oben.
models-no-results = Kein Modell auf Hugging Face passt zu „{ $query }“. Versuche es mit anderen Wörtern.
models-owner = Von { $owner }
models-owner-gated = Von { $owner } · TypeAlike kann dieses Modell nicht herunterladen
models-show-files = Dateien anzeigen
models-back-to-results = Zurück zu den Ergebnissen
models-files-loading = Liste der Dateien wird abgerufen…
models-files-footer = Jede Datei ist dasselbe Modell in einer anderen Größe. Eine kleinere Datei läuft schneller, aber ihre Vorschläge sind öfter falsch. Wenn du unsicher bist, wähle eine Q4_K_M-Datei.
models-no-files = Dieses Modell hat keine Dateien, die TypeAlike verwenden kann.
models-downloaded-heading = Heruntergeladene Modelle
models-download = Herunterladen
models-cancel-download = Download abbrechen
models-canceling = Download wird abgebrochen…
models-delete = Löschen
models-try-again = Erneut versuchen
models-error-offline = TypeAlike kann keine Verbindung zu Hugging Face herstellen. Prüfe deine Internetverbindung.
models-error-account = TypeAlike kann dieses Modell nicht herunterladen, weil es ein Hugging-Face-Konto braucht. Wähle ein anderes Modell.
models-error-not-found = Hugging Face findet dieses Modell nicht. Suche erneut nach dem Modell.
models-error-server = Hugging Face ist nicht erreichbar. Versuche es später erneut.
models-error-response = Hugging Face hat eine fehlerhafte Antwort gesendet. Versuche es später erneut.
models-error-storage = TypeAlike kann das Modell nicht speichern. Prüfe, ob auf dem Datenträger genug Platz frei ist.
models-error-canceled = Der Download wurde vor dem Ende gestoppt.

## The numbers of the Models pane. $count is a count of downloads: a whole number below one thousand, else a number of thousands, millions or billions, with one digit after the decimal separator when it is not zero, such as 1.2 or 15. $percent is a whole number from 0 to 100. $size is a file size: a whole number of bytes, kilobytes or megabytes, or a number of gigabytes with one digit after the decimal separator, such as 2.5.

models-downloads =
    { $count ->
        [one] { $count } Download
       *[other] { $count } Downloads
    }
models-downloads-thousands = { $count } Tsd. Downloads
models-downloads-millions =
    { $count ->
        [one] { $count } Million Downloads
       *[other] { $count } Millionen Downloads
    }
models-downloads-billions =
    { $count ->
        [one] { $count } Milliarde Downloads
       *[other] { $count } Milliarden Downloads
    }
models-download-progress = { $percent } % heruntergeladen
models-size-bytes = { $size } Byte
models-size-kilobytes = { $size } KB
models-size-megabytes = { $size } MB
models-size-gigabytes = { $size } GB

## The Apps and websites pane of the settings window.

suggestion-places-title = Apps und Websites
suggestion-places-summary = Vorschläge erscheinen in jeder App und auf jeder Website, auf der du tippst. Um Vorschläge in einer App oder auf einer Website zu stoppen, schalte sie in der Liste aus.
suggestion-places-websites = Websites
suggestion-places-no-websites = Auf einer Website wurde noch kein Vorschlag angezeigt.
suggestion-places-websites-footer = Wenn auf einer Website zum ersten Mal ein Vorschlag erscheint, fügt TypeAlike die Website dieser Liste hinzu.
suggestion-places-private-windows = In privaten Fenstern stoppen
suggestion-places-private-windows-description = In einem privaten Fenster oder Inkognito-Fenster eines Webbrowsers zeigt TypeAlike keine Vorschläge, liest keinen Bildschirmtext und speichert nichts, was du tippst.

## The Copied and screen text pane of the settings window.

screen-text-title = Kopierter Text und Bildschirmtext
screen-text-summary = Vorschläge können den Text verwenden, den du kopierst, und den Text in der Nähe des Felds, in dem du tippst, etwa die letzten Nachrichten eines Chats. TypeAlike liest diesen Text auf diesem Computer. Es speichert nur einige Wörter der Nachrichten, auf die du antwortest, zusammen mit deinen früheren Texten.
screen-text-clipboard = Kopierten Text verwenden
screen-text-clipboard-description = Vorschläge können den Text verwenden, den du kopierst. TypeAlike liest ihn nur, während du ein Textfeld verwendest, und speichert ihn nicht.
screen-text-switch = Bildschirmtext verwenden
screen-text-switch-description = TypeAlike liest den Bildschirmtext in jeder App, die in der Liste unten eingeschaltet ist.
screen-text-recognition = Texterkennung
screen-text-recognition-description = Wenn TypeAlike den Text eines Fensters nicht abrufen kann, liest es den Text aus einem Bild des Fensters. „Schnell“ liest Deutsch, Englisch, Französisch, Italienisch, Portugiesisch und Spanisch. „Genau“ liest mehr Sprachen mit weniger Fehlern, ist aber langsamer.
screen-text-recognition-fast = Schnell (empfohlen)
screen-text-recognition-accurate = Genau
screen-text-permission = Manche Apps geben ihren Text nicht an TypeAlike weiter. Um ihren Text aus einem Bild des Fensters zu lesen, braucht TypeAlike die Berechtigung „Aufnahme von Bildschirm & Systemaudio“.
screen-text-open-system-settings = Systemeinstellungen öffnen

## The Personalization pane of the settings window.

personalization-title = Personalisierung
personalization-summary = Vorschläge können Angaben über dich verwenden und aus deinen früheren Texten lernen. TypeAlike bewahrt die früheren Texte verschlüsselt auf diesem Computer auf. Den Text von Passwortfeldern speichert es nicht.
personalization-name = Dein Name
personalization-name-description = Vorschläge können deinen Namen verwenden, zum Beispiel, um eine E-Mail zu unterschreiben.
personalization-notes = Notizen über dich
personalization-notes-description = Schreibe ein paar kurze Sätze über dich, die den Vorschlägen helfen, etwa über deinen Beruf und deine Sprachen. Beginne jeden Satz mit „Ich“, zum Beispiel „Ich halte meine E-Mails kurz.“
personalization-notes-example = Mit einem Beispiel beginnen
personalization-notes-example-description = Wähle das Beispiel, das deiner Art zu schreiben am nächsten kommt. Ändere danach die Notizen unten.
personalization-history = Frühere Texte verwenden
personalization-history-description = TypeAlike speichert den Text, den du schreibst, in einer verschlüsselten Datei auf diesem Computer. Vorschläge verwenden dann deine Wörter und deinen Stil. Wenn du diese Option ausschaltest, bleibt der gespeicherte Text erhalten, bis du ihn unten auf dieser Seite löschst.
personalization-kept-texts = Zu speichernder Text
personalization-kept-texts-description = TypeAlike speichert deinen Text, wenn du ihn abschließt, zum Beispiel wenn du eine Nachricht sendest.
personalization-kept-texts-accepted = Nur Texte, in denen du einen Vorschlag verwendet hast
personalization-kept-texts-all = Alle Texte
personalization-reply-words = Aus den Nachrichten lernen, auf die du antwortest
personalization-reply-words-description = In einem Chat oder einer E-Mail speichert TypeAlike auch einige Wörter der Nachricht, auf die du antwortest. Vorschläge verwenden dann deine früheren Antworten auf ähnliche Nachrichten. Wenn du diese Option ausschaltest, löscht TypeAlike diese Wörter.
personalization-word-choice = Wortwahl
personalization-word-choice-description = Vorschläge verwenden öfter die Wörter, die du häufig schreibst. Eine stärkere Stufe übernimmt mehr deiner Wörter, aber Vorschläge sind dann öfter falsch.
personalization-word-choice-off = Aus
personalization-word-choice-light = Gering
personalization-word-choice-medium = Mittel
personalization-word-choice-strong = Stark
personalization-word-choice-needs-history = Um diese Einstellung zu verwenden, schalte „Frühere Texte verwenden“ ein.
personalization-apps-footer = Wenn du eine App auf „Aus“ setzt, löscht TypeAlike den Text, den es aus dieser App gespeichert hat. Mit „Nur diese App“ helfen die früheren Texte einer App nur in dieser App. Die Apps mit „Alle Apps“ teilen ihre früheren Texte. Wenn in einer App zum ersten Mal ein Vorschlag erscheint, fügt TypeAlike die App dieser Liste hinzu.
personalization-delete-description = TypeAlike löscht den gesamten gespeicherten Text. Du kannst das nicht rückgängig machen.
personalization-delete = Alle früheren Texte löschen
personalization-delete-confirm = Zum Löschen erneut drücken

## The Energy saving pane of the settings window.

energy-saving-title = Energiesparen
energy-saving-summary = TypeAlike kann Vorschläge stoppen, wenn du eine Zeit lang nicht tippst, während Spielen, im Vollbild und beim Energiesparen. Das gibt Speicher frei und spart Strom.
energy-saving-idle-timeout = Nach einer Zeit ohne Nutzung stoppen
energy-saving-idle-timeout-description = Wenn du in dieser Zeit weder tippst noch klickst, stoppen Vorschläge, um Speicher freizugeben. Sie starten wieder, wenn du in ein Textfeld tippst.
energy-saving-idle-timeout-off = Nie
energy-saving-idle-timeout-15 = 15 Minuten
energy-saving-idle-timeout-30 = 30 Minuten
energy-saving-idle-timeout-60 = 60 Minuten
energy-saving-energy-saver = Beim Energiesparen stoppen
energy-saving-energy-saver-description = Solange der Energiesparmodus oder der Stromsparmodus an ist, stoppen Vorschläge, um Strom zu sparen. Sie starten wieder, wenn er ausgeschaltet wird.
energy-saving-energy-saver-stopped = Vorschläge sind aus, solange der Computer Energie spart.
energy-saving-game = Während Spielen stoppen
energy-saving-game-description = Während du spielst, stoppen Vorschläge, um Speicher für das Spiel freizugeben. Sie starten wieder, wenn du in ein Textfeld tippst.
energy-saving-full-screen = Im Vollbild stoppen
energy-saving-full-screen-description = Solange eine App den ganzen Bildschirm füllt, etwa ein Video oder ein Spiel, stoppen Vorschläge, um Speicher freizugeben. Sie starten wieder, wenn du in ein Textfeld tippst.
energy-saving-preparation = Vorschläge früh vorbereiten
energy-saving-preparation-description = Wenn du in ein Textfeld wechselst, liest das Modell den Text darin sofort. Der erste Vorschlag erscheint dann früher, aber das braucht mehr Strom.

## The Permissions pane of the settings window. It shows only on an OS that asks the user for permissions (macOS). The labels are the names of the permissions in System Settings.

permissions-title = Berechtigungen
permissions-summary = TypeAlike verwendet diese Berechtigungen, um Vorschläge in anderen Apps anzuzeigen. Du erteilst sie in den Systemeinstellungen.
permissions-accessibility = Bedienungshilfen
permissions-accessibility-description = TypeAlike braucht diese Berechtigung, um den Text dort zu lesen, wo du tippst, und um einen Vorschlag einzufügen, wenn du Tab drückst. Ohne sie erscheint kein Vorschlag.
permissions-screen-recording = Aufnahme von Bildschirm & Systemaudio
permissions-screen-recording-description = Manche Apps geben ihren Text nicht an TypeAlike weiter. Mit dieser Berechtigung liest TypeAlike ihren Text aus einem Bild des Fensters. Vorschläge funktionieren auch ohne sie.
permissions-granted = Erlaubt
permissions-give-access = Zugriff erlauben
permissions-footer = Wenn die Systemeinstellungen dich bitten, TypeAlike zu beenden und erneut zu öffnen, tu das, damit TypeAlike die neue Berechtigung verwenden kann.

## The About pane of the settings window: the version of TypeAlike, the check for a newer version, its install and the licenses of the third-party software. $version is a version number, such as 0.4.0.

about-title = Info
about-summary = Die Version von TypeAlike und die Lizenzen der Software, die es enthält.
about-version = TypeAlike { $version }
about-check-for-updates = Nach Updates suchen
about-checking = Suche nach Updates…
about-up-to-date = Das ist die neueste Version.
about-update-available = Version { $version } ist verfügbar.
about-download = Herunterladen
about-check-failed = TypeAlike kann nicht nach Updates suchen. Prüfe deine Internetverbindung.
about-try-again = Erneut versuchen
about-keep-up-to-date = TypeAlike aktuell halten
about-keep-up-to-date-description = TypeAlike sucht einmal am Tag nach einer neuen Version. Es installiert sie und startet neu.
about-downloading = Version { $version } wird heruntergeladen…
about-install-waiting = Version { $version } wird installiert, wenn der Download des Modells fertig ist.
about-installing = Version { $version } wird installiert…
about-install-failed = TypeAlike kann Version { $version } nicht installieren.
about-source-code = Quellcode anzeigen
about-footer = TypeAlike ist freie Software unter der GNU General Public License, Version 3 oder neuer.
about-libraries-heading = Software von Drittanbietern
about-libraries-footer = TypeAlike enthält diese Open-Source-Software. Jede Bibliothek hat ihre eigene Lizenz.
about-show-license = Lizenz anzeigen
about-back-to-libraries = Zurück zur Liste

## The examples of the notes about the writer, in the menu “Start from an example” of the Personalization pane and of the setup. The text of each example goes into the notes, in the first person, and the model reads it.

notes-example-own = Eigene Notizen
notes-example-general = Alltägliches Schreiben
notes-example-general-text = Ich schreibe klare und freundliche Nachrichten. Ich behalte den Ton des Gesprächs bei.
notes-example-work = Geschäftliche E-Mails
notes-example-work-text = Ich schreibe E-Mails und Nachrichten bei der Arbeit. Ich verwende einen höflichen und professionellen Ton und halte meine Nachrichten kurz und klar.
notes-example-support = Kundensupport
notes-example-support-text = Ich beantworte Fragen von Kunden. Ich bin freundlich und geduldig und erkläre klar die Schritte, die das Problem lösen.
notes-example-chats = Chats mit Freunden und Familie
notes-example-chats-text = Ich chatte mit Freunden und Familie. Ich schreibe kurze und lockere Nachrichten.
notes-example-study = Schule und Studium
notes-example-study-text = Ich lerne an einer Schule oder Hochschule. Ich schreibe Aufsätze, Notizen und E-Mails an meine Lehrkräfte in einem klaren und korrekten Stil.
notes-example-software = Softwareentwicklung
notes-example-software-text = Ich arbeite in der Softwareentwicklung. Ich schreibe Code-Reviews, Fehlerberichte und technische Dokumente. Ich verwende die genauen Namen von Code und Tools.

## The setup, which shows when TypeAlike starts for the first time and after a reset of the settings. $number is the number of the step, and $count is the number of steps. The demo shows the typed text, then the suggestion after it in gray, then the key that types its next word.

setup-window-title = TypeAlike einrichten
setup-progress = Schritt { $number } von { $count }
setup-back = Zurück
setup-continue = Weiter
setup-start = Einrichtung starten
setup-finish = Einrichtung abschließen
setup-welcome-title = Willkommen bei TypeAlike
setup-welcome-body = TypeAlike schlägt in jeder App die nächsten Wörter vor, während du tippst. Drücke Tab, um das nächste Wort einzufügen, oder tippe weiter, um den Vorschlag zu ignorieren.
setup-demo-typed = Danke für die schnelle Antwort. Ich schicke dir den
setup-demo-suggestion = Bericht bis Freitag.
setup-demo-key = Tab
setup-model-title = Modell herunterladen
setup-model-select-title = Modell auswählen
setup-model-footer = TypeAlike braucht dieses Modell, um Vorschläge zu schreiben. Du lädst es nur einmal herunter. Die Einrichtung geht weiter, wenn der Download fertig ist.
setup-model-checking = Grafikkarte wird geprüft…
setup-other-models = Anderes Modell auswählen
setup-permission-title = TypeAlike Zugriff auf Textfelder geben
setup-permission-body = Klicke auf „Zugriff erlauben“. Schalte dann TypeAlike in den Systemeinstellungen ein und kehre zu diesem Fenster zurück.
setup-about-title = Erzähl TypeAlike von dir
setup-sources-title = Was Vorschläge verwenden können
setup-history-description = TypeAlike speichert, was du schreibst, in einer verschlüsselten Datei auf diesem Computer. So verwenden Vorschläge deine Wörter und deinen Stil. Du kannst diesen Text auf der Seite „Personalisierung“ löschen.
setup-screen-text-description = TypeAlike liest den Text in der Nähe des Felds, in dem du tippst, etwa die letzten Nachrichten eines Chats. Mit deinen früheren Texten speichert es auch einige Wörter der Nachrichten, auf die du antwortest.
setup-sources-footer = TypeAlike verwendet das Internet nur, um Modelle zu finden und herunterzuladen und um neue Versionen von TypeAlike zu laden. Den Text von Passwortfeldern speichert es nicht.
setup-ready-title = TypeAlike ist bereit
setup-ready-body = Probier es jetzt in einer beliebigen App aus, etwa in deinem E-Mail-Programm oder einem Chat.
setup-tray-notification-area = TypeAlike bleibt im Infobereich der Taskleiste. Über sein Symbol kannst du Vorschläge pausieren oder die Einstellungen öffnen.
setup-tray-menu-bar = TypeAlike bleibt in der Menüleiste. Über sein Symbol kannst du Vorschläge pausieren oder die Einstellungen öffnen.
