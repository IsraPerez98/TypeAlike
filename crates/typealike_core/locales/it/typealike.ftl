## The tray menu on Windows, the menu bar menu on macOS and the first entry of the badge menu.

menu-pause = Sospendi i suggerimenti
menu-resume = Riprendi i suggerimenti
menu-settings = Impostazioni…
menu-quit = Esci da TypeAlike

## The name of TypeAlike and its status, in the tooltip of the tray icon and for screen readers at the badge.

status-active = TypeAlike
status-paused = TypeAlike (in pausa)
status-off-here = TypeAlike (disattivato qui)
status-energy-saver = TypeAlike (disattivato mentre il computer risparmia energia)
status-game = TypeAlike (disattivato mentre giochi)
status-full-screen = TypeAlike (disattivato mentre un’app occupa tutto lo schermo)

## The menu of the badge at the caret. $app is the name of an app, and $host is the host of a website, such as mail.example.com.

badge-app-suggestions = Mostra i suggerimenti in { $app }
badge-site-suggestions = Mostra i suggerimenti su { $host }
badge-app-screen-text = Usa il testo dello schermo in { $app }
badge-app-earlier-writing = Testi precedenti in { $app }

## The choices of the earlier writing of an app, in the Personalization pane and in the badge menu.

history-scope-off = Disattivato
history-scope-this-app = Solo questa app
history-scope-all-apps = Tutte le app

## The list of apps in the Apps and websites, Copied and screen text and Personalization panes.

app-list-heading = App
app-list-empty = Nessun suggerimento è ancora comparso in un’app.
app-list-footer = Quando un suggerimento compare per la prima volta in un’app, TypeAlike aggiunge l’app a questo elenco.

## The General pane of the settings window.

general-title = Generali
general-summary = Scegli quando si apre TypeAlike e come compaiono i suggerimenti.
general-autostart = Apri al login
general-autostart-description = TypeAlike si apre ogni volta che accedi, così non devi aprirlo tu.
general-language = Lingua
general-language-description = Le finestre e i menu di TypeAlike compaiono in questa lingua. I suggerimenti restano nella lingua del tuo testo.
# The first entry of the language menu. The other entries are the names of the languages, which stay the same in each language.
general-language-system = Lingua del sistema
general-display = Mostra le parole una alla volta
general-display-description = Ogni parola di un suggerimento compare appena il modello la scrive. La prima parola compare prima, ma il suggerimento cresce mentre lo leggi.
general-typo-rule = Nascondi i suggerimenti in caso di errori di battitura
general-typo-rule-description = Quando una parola sembra contenere un errore di battitura, nessun suggerimento la completa, così il tasto Tab non inserisce l’errore. Anche un nome raro può sembrare un errore, e allora non riceve suggerimenti.
general-typo-fix = Correggi gli errori di battitura
general-typo-fix-description = Quando una parola sembra contenere un errore di battitura, il suggerimento mostra la parola barrata e poi la parola corretta. Il tasto Tab inserisce la parola corretta.
general-length = Lunghezza dei suggerimenti
general-length-description = Un suggerimento si ferma a questa lunghezza. Un suggerimento più breve compare prima.
general-length-very-short = Molto breve
general-length-short = Breve
general-length-medium = Media
general-length-long = Lunga
general-length-very-long = Molto lunga
general-length-processor-note = Quando il modello viene eseguito sul processore, i suggerimenti sono brevi o molto brevi, così compaiono prima.
general-sentence-end = Interrompi a fine frase
general-sentence-end-description = Un suggerimento termina dopo la prima frase che completa.
general-sentence-end-off-note = Dopo la prima frase i suggerimenti sono spesso sbagliati. Attiva questa opzione per fermarli lì.
general-reset = Ripristina tutte le impostazioni
general-reset-description = Tutte le impostazioni tornano ai valori predefiniti e la configurazione si riapre. I modelli scaricati e i testi precedenti restano.
general-reset-confirm = Premi di nuovo per ripristinare

## The menu of the suggestion model and its status, in the General, Models and Playground panes.

model-menu = Modello dei suggerimenti
model-menu-select = Seleziona un modello
model-menu-none = Nessun modello scaricato
model-needed = TypeAlike ha bisogno di un modello per scrivere i suggerimenti. Scarica qui sotto il modello consigliato.
model-recommended = Consigliato per questo computer
model-recommendation-unknown-gpu = TypeAlike non riesce a leggere la memoria della scheda video, quindi consiglia un modello adatto alla maggior parte delle schede video.
model-starting = Avvio del modello…
model-paused = I suggerimenti sono in pausa. Per riprenderli, seleziona “Riprendi i suggerimenti” nel menu di TypeAlike.
model-energy-saver = I suggerimenti sono disattivati mentre il computer risparmia energia. Per usarli ora, disattiva “Interrompi con il risparmio energetico”.
model-game = I suggerimenti si sono fermati per lasciare memoria a un gioco. Ripartono quando scrivi in un campo di testo.
model-full-screen = I suggerimenti si sono fermati per lasciare memoria a un’app a schermo intero. Ripartono quando scrivi in un campo di testo.
model-idle = I suggerimenti si sono fermati per liberare memoria, perché non hai scritto né fatto clic per un po’. Ripartono quando scrivi in un campo di testo.
model-error-missing = TypeAlike non trova il file del modello. Scarica di nuovo il modello.
model-error-invalid = TypeAlike non può usare questo modello. Il file potrebbe essere danneggiato o di un tipo che TypeAlike non supporta. Scaricalo di nuovo o seleziona un altro modello.
model-error-too-large = Il modello non entra nella memoria libera della scheda video. Seleziona un modello più piccolo.
model-error-hardware = TypeAlike non riesce a eseguire il modello su questa scheda video. In “Esegui il modello su”, seleziona un’altra opzione.
model-error-no-model = Seleziona prima un modello.
model-error-text-too-long = Il testo è troppo lungo per questo modello. Usa un testo più breve.
model-error-failed = TypeAlike non è riuscito a scrivere un suggerimento. Riprova.
model-error-superseded = TypeAlike ha fermato questo suggerimento per scriverne uno più recente.

## The Playground pane of the settings window. $seconds is the time that the model took to write the suggestion, with two digits after the decimal separator, such as 0.42.

playground-title = Area di prova
playground-summary = Seleziona un modello, poi scrivi l’inizio di una frase per vedere cosa suggerisce il modello.
playground-text = Testo
playground-text-placeholder = Scrivi l’inizio di una frase
playground-suggest = Mostra il suggerimento
playground-output = Suggerimento
playground-writing = Scrittura di un suggerimento…
playground-nothing-added = Il modello non ha suggerimenti per questo testo. Scrivi altre parole e riprova.
playground-time = Scritto in { $seconds } secondi

## The Models pane of the settings window. $developer is the company that made a model, such as Google. $owner is the account on Hugging Face that shares a model. $query is the text that the user searched for.

models-title = Modelli
models-summary = Un modello su questo computer scrive i suggerimenti. Il testo che scrivi nelle altre app non lascia mai questo computer. La ricerca e il download dei modelli usano Internet.
models-model-description = Un modello più piccolo è più veloce e richiede meno memoria, ma i suoi suggerimenti sono sbagliati più spesso.
models-hardware = Esegui il modello su
models-hardware-description = Una scheda video esegue il modello più velocemente del processore, ma tutto il modello deve entrare nella sua memoria.
models-hardware-processor = Processore (CPU)
models-hardware-graphics-card = Scheda video (Vulkan)
models-hardware-nvidia = Scheda video NVIDIA (CUDA)
models-search = Cerca su Hugging Face
models-search-placeholder = Nome di un modello
models-search-button = Cerca
models-recommended-heading = Modelli consigliati
models-recommended-footer = Nel dubbio, scarica il modello che TypeAlike consiglia per questo computer. Puoi eliminare un modello in qualsiasi momento.
models-developer = Di { $developer }
models-developer-recommended = Di { $developer } · Consigliato per questo computer
models-searching = Ricerca su Hugging Face…
models-results-heading = Risultati per “{ $query }”
models-results-footer = L’elenco mostra solo i modelli che TypeAlike può usare. I modelli con più download sono in alto.
models-no-results = Nessun modello su Hugging Face corrisponde a “{ $query }”. Prova con altre parole.
models-owner = Di { $owner }
models-owner-gated = Di { $owner } · TypeAlike non può scaricare questo modello
models-show-files = Mostra i file
models-back-to-results = Torna ai risultati
models-files-loading = Recupero dell’elenco dei file…
models-files-footer = Ogni file è lo stesso modello in una dimensione diversa. Un file più piccolo è più veloce, ma i suoi suggerimenti sono sbagliati più spesso. Nel dubbio, seleziona un file Q4_K_M.
models-no-files = Questo modello non ha file che TypeAlike possa usare.
models-downloaded-heading = Modelli scaricati
models-download = Scarica
models-cancel-download = Annulla il download
models-canceling = Annullamento del download…
models-delete = Elimina
models-try-again = Riprova
models-error-offline = TypeAlike non riesce a connettersi a Hugging Face. Controlla la connessione a Internet.
models-error-account = TypeAlike non può scaricare questo modello, perché richiede un account Hugging Face. Seleziona un altro modello.
models-error-not-found = Hugging Face non trova questo modello. Cerca di nuovo il modello.
models-error-server = Hugging Face non è disponibile. Riprova più tardi.
models-error-response = Hugging Face ha inviato una risposta non valida. Riprova più tardi.
models-error-storage = TypeAlike non riesce a salvare il modello. Controlla che il disco abbia spazio libero.
models-error-canceled = Il download si è fermato prima della fine.

## The numbers of the Models pane. $count is a count of downloads: a whole number below one thousand, else a number of thousands, millions or billions, with one digit after the decimal separator when it is not zero, such as 1.2 or 15. $percent is a whole number from 0 to 100. $size is a file size: a whole number of bytes, kilobytes or megabytes, or a number of gigabytes with one digit after the decimal separator, such as 2.5.

models-downloads = { $count } download
models-downloads-thousands =
    { $count ->
        [one] mille download
       *[other] { $count } mila download
    }
models-downloads-millions =
    { $count ->
        [one] { $count } milione di download
       *[other] { $count } milioni di download
    }
models-downloads-billions =
    { $count ->
        [one] { $count } miliardo di download
       *[other] { $count } miliardi di download
    }
models-download-progress = { $percent }% scaricato
models-size-bytes = { $size } byte
models-size-kilobytes = { $size } KB
models-size-megabytes = { $size } MB
models-size-gigabytes = { $size } GB

## The Apps and websites pane of the settings window.

suggestion-places-title = App e siti web
suggestion-places-summary = I suggerimenti compaiono in ogni app e su ogni sito web dove scrivi. Per fermare i suggerimenti in un’app o su un sito web, disattiva la sua opzione.
suggestion-places-websites = Siti web
suggestion-places-no-websites = Nessun suggerimento è ancora comparso su un sito web.
suggestion-places-websites-footer = Quando un suggerimento compare per la prima volta su un sito web, TypeAlike aggiunge il sito web a questo elenco.

## The Copied and screen text pane of the settings window.

screen-text-title = Testo copiato e dello schermo
screen-text-summary = I suggerimenti possono usare il testo che copi e il testo vicino al campo dove scrivi, come gli ultimi messaggi di una chat. TypeAlike legge questo testo su questo computer. Salva solo alcune parole dei messaggi a cui rispondi, insieme ai tuoi testi precedenti.
screen-text-clipboard = Usa il testo copiato
screen-text-clipboard-description = I suggerimenti possono usare il testo che copi. TypeAlike lo legge solo mentre usi un campo di testo e non lo salva.
screen-text-switch = Usa il testo dello schermo
screen-text-switch-description = TypeAlike legge il testo dello schermo in ogni app con l’opzione attivata nell’elenco qui sotto.
screen-text-recognition = Riconoscimento del testo
screen-text-recognition-description = Quando TypeAlike non riesce a ottenere il testo di una finestra, lo legge da un’immagine della finestra. “Veloce” legge francese, inglese, italiano, portoghese, spagnolo e tedesco. “Accurato” legge più lingue con meno errori, ma è più lento.
screen-text-recognition-fast = Veloce (consigliato)
screen-text-recognition-accurate = Accurato
screen-text-permission = Alcune app non danno il loro testo a TypeAlike. Per leggerlo da un’immagine della finestra, TypeAlike ha bisogno del permesso Registrazione schermo e audio di sistema.
screen-text-open-system-settings = Apri Impostazioni di Sistema

## The Personalization pane of the settings window.

personalization-title = Personalizzazione
personalization-summary = I suggerimenti possono usare informazioni su di te e imparare dai tuoi testi precedenti. TypeAlike conserva i testi precedenti cifrati su questo computer. Non salva il testo dei campi password.
personalization-name = Il tuo nome
personalization-name-description = I suggerimenti possono usare il tuo nome, per esempio per firmare un’email.
personalization-notes = Note su di te
personalization-notes-description = Scrivi alcune brevi informazioni su di te che aiutino i suggerimenti, come il tuo lavoro e le tue lingue. Scrivile in prima persona, per esempio: “Scrivo email brevi.”
personalization-notes-example = Parti da un esempio
personalization-notes-example-description = Seleziona l’esempio più vicino al tuo modo di scrivere. Poi modifica le note qui sotto.
personalization-history = Usa i testi precedenti
personalization-history-description = TypeAlike salva il testo che scrivi in un file cifrato su questo computer. I suggerimenti usano poi le tue parole e il tuo stile. Se disattivi questa opzione, il testo salvato resta finché non lo elimini in fondo a questa pagina.
personalization-kept-texts = Testo da salvare
personalization-kept-texts-description = TypeAlike salva il tuo testo quando lo finisci, per esempio quando invii un messaggio.
personalization-kept-texts-accepted = Solo il testo in cui hai usato un suggerimento
personalization-kept-texts-all = Tutto il testo
personalization-reply-words = Impara dai messaggi a cui rispondi
personalization-reply-words-description = In una chat o in un’email, TypeAlike salva anche alcune parole del messaggio a cui rispondi. I suggerimenti usano poi le tue risposte precedenti a messaggi simili. Se disattivi questa opzione, TypeAlike elimina queste parole.
personalization-word-choice = Scelta delle parole
personalization-word-choice-description = I suggerimenti usano di più le parole che scrivi spesso. Un livello più forte usa più parole tue, ma i suggerimenti sono poi sbagliati più spesso.
personalization-word-choice-off = Disattivata
personalization-word-choice-light = Leggera
personalization-word-choice-medium = Media
personalization-word-choice-strong = Forte
personalization-word-choice-needs-history = Per usare questa impostazione, attiva “Usa i testi precedenti”.
personalization-apps-footer = Quando imposti un’app su “Disattivato”, TypeAlike elimina il testo che ha salvato da quell’app. Con “Solo questa app”, i testi precedenti di un’app aiutano solo in quell’app. Le app con “Tutte le app” condividono i loro testi precedenti. Quando un suggerimento compare per la prima volta in un’app, TypeAlike aggiunge l’app a questo elenco.
personalization-delete-description = TypeAlike elimina tutto il testo che ha salvato. Non puoi annullare questa azione.
personalization-delete = Elimina tutti i testi precedenti
personalization-delete-confirm = Premi di nuovo per eliminare

## The Energy saving pane of the settings window.

energy-saving-title = Risparmio energetico
energy-saving-summary = TypeAlike può fermare i suggerimenti quando non scrivi per un po’, durante i giochi, a schermo intero e con il risparmio energetico. Questo libera memoria e risparmia energia.
energy-saving-idle-timeout = Interrompi dopo un periodo di inattività
energy-saving-idle-timeout-description = Se non scrivi e non fai clic per questo tempo, i suggerimenti si fermano per liberare memoria. Ripartono quando scrivi in un campo di testo.
energy-saving-idle-timeout-off = Mai
energy-saving-idle-timeout-15 = 15 minuti
energy-saving-idle-timeout-30 = 30 minuti
energy-saving-idle-timeout-60 = 60 minuti
energy-saving-energy-saver = Interrompi con il risparmio energetico
energy-saving-energy-saver-description = Mentre il risparmio energetico è attivo, i suggerimenti si fermano per risparmiare energia. Ripartono quando si disattiva.
energy-saving-energy-saver-stopped = I suggerimenti sono disattivati mentre il computer risparmia energia.
energy-saving-game = Interrompi durante i giochi
energy-saving-game-description = Mentre giochi, i suggerimenti si fermano per lasciare memoria al gioco. Ripartono quando scrivi in un campo di testo.
energy-saving-full-screen = Interrompi a schermo intero
energy-saving-full-screen-description = Mentre un’app occupa tutto lo schermo, come un video o un gioco, i suggerimenti si fermano per liberare memoria. Ripartono quando scrivi in un campo di testo.
energy-saving-preparation = Prepara i suggerimenti in anticipo
energy-saving-preparation-description = Quando entri in un campo di testo, il modello legge subito il testo al suo interno. Il primo suggerimento compare prima, ma questo consuma più energia.

## The Permissions pane of the settings window. It shows only on an OS that asks the user for permissions (macOS). The labels are the names of the permissions in System Settings.

permissions-title = Permessi
permissions-summary = TypeAlike usa questi permessi per mostrare i suggerimenti nelle altre app. Li concedi in Impostazioni di Sistema.
permissions-accessibility = Accessibilità
permissions-accessibility-description = TypeAlike ha bisogno di questo permesso per leggere il testo dove scrivi e per inserire un suggerimento quando premi Tab. Senza questo permesso, non compare nessun suggerimento.
permissions-screen-recording = Registrazione schermo e audio di sistema
permissions-screen-recording-description = Alcune app non danno il loro testo a TypeAlike. Con questo permesso, TypeAlike legge il loro testo da un’immagine della finestra. I suggerimenti funzionano anche senza.
permissions-granted = Consentito
permissions-give-access = Consenti l’accesso
permissions-footer = Quando Impostazioni di Sistema ti chiede di uscire da TypeAlike e riaprirlo, fallo, così TypeAlike può usare il nuovo permesso.

## The About pane of the settings window: the version of TypeAlike, the check for a newer version, its install and the licenses of the third-party software. $version is a version number, such as 0.4.0.

about-title = Informazioni
about-summary = La versione di TypeAlike e le licenze del software che contiene.
about-version = TypeAlike { $version }
about-check-for-updates = Cerca aggiornamenti
about-checking = Ricerca degli aggiornamenti…
about-up-to-date = Questa è la versione più recente.
about-update-available = La versione { $version } è disponibile.
about-download = Scarica
about-check-failed = TypeAlike non riesce a cercare gli aggiornamenti. Controlla la connessione a Internet.
about-try-again = Riprova
about-keep-up-to-date = Mantieni TypeAlike aggiornato
about-keep-up-to-date-description = TypeAlike cerca una nuova versione una volta al giorno. La installa e si riavvia.
about-downloading = Download della versione { $version }…
about-install-waiting = La versione { $version } verrà installata alla fine del download del modello.
about-installing = Installazione della versione { $version }…
about-install-failed = TypeAlike non riesce a installare la versione { $version }.
about-source-code = Mostra il codice sorgente
about-footer = TypeAlike è software libero con la GNU General Public License, versione 3 o successiva.
about-libraries-heading = Software di terze parti
about-libraries-footer = TypeAlike contiene questo software open source. Ogni libreria ha la sua licenza.
about-show-license = Mostra licenza
about-back-to-libraries = Torna all’elenco

## The examples of the notes about the writer, in the menu “Start from an example” of the Personalization pane and of the setup. The text of each example goes into the notes, in the first person, and the model reads it.

notes-example-own = Le tue note
notes-example-general = Scrittura di tutti i giorni
notes-example-general-text = Scrivo messaggi chiari e cordiali. Mantengo il tono della conversazione.
notes-example-work = Email di lavoro
notes-example-work-text = Scrivo email e messaggi di lavoro. Uso un tono cortese e professionale, e scrivo messaggi brevi e chiari.
notes-example-support = Assistenza clienti
notes-example-support-text = Rispondo alle domande dei clienti. Sono gentile e paziente, e spiego in modo chiaro i passaggi per risolvere il problema.
notes-example-chats = Chat con amici e famiglia
notes-example-chats-text = Chatto con amici e familiari. Scrivo messaggi brevi e informali.
notes-example-study = Scuola e studio
notes-example-study-text = Studio a scuola o all’università. Scrivo temi, appunti ed email ai miei insegnanti con uno stile chiaro e corretto.
notes-example-software = Sviluppo software
notes-example-software-text = Lavoro nello sviluppo software. Scrivo revisioni del codice, segnalazioni di bug e documenti tecnici. Uso i nomi esatti del codice e degli strumenti.

## The setup, which shows when TypeAlike starts for the first time and after a reset of the settings. $number is the number of the step, and $count is the number of steps. The demo shows the typed text, then the suggestion after it in gray, then the key that types its next word.

setup-window-title = Configurazione di TypeAlike
setup-progress = Passaggio { $number } di { $count }
setup-back = Indietro
setup-continue = Continua
setup-start = Inizia
setup-finish = Termina
setup-welcome-title = Ti diamo il benvenuto in TypeAlike
setup-welcome-body = TypeAlike suggerisce le prossime parole mentre scrivi, in qualsiasi app. Premi Tab per inserire la parola successiva, o continua a scrivere per ignorare il suggerimento.
setup-demo-typed = Grazie per la risposta veloce. Ti invio il
setup-demo-suggestion = resoconto entro venerdì.
setup-demo-key = Tab
setup-model-title = Scarica il modello
setup-model-select-title = Seleziona il modello
setup-model-footer = TypeAlike ha bisogno di questo modello per scrivere i suggerimenti. Lo scarichi una sola volta. La configurazione continua quando finisce il download.
setup-model-checking = Controllo della scheda video…
setup-other-models = Scegli un altro modello
setup-permission-title = Dai a TypeAlike l’accesso ai campi di testo
setup-permission-body = Fai clic su “Consenti l’accesso”. Poi attiva TypeAlike in Impostazioni di Sistema e torna a questa finestra.
setup-about-title = Parla di te a TypeAlike
setup-sources-title = Cosa possono usare i suggerimenti
setup-history-description = TypeAlike salva quello che scrivi in un file cifrato su questo computer. Così i suggerimenti usano le tue parole e il tuo stile. Puoi eliminare questo testo nella pagina Personalizzazione.
setup-screen-text-description = TypeAlike legge il testo vicino al campo dove scrivi, come gli ultimi messaggi di una chat. Insieme ai tuoi testi precedenti, salva anche alcune parole dei messaggi a cui rispondi.
setup-sources-footer = TypeAlike usa Internet solo per trovare e scaricare i modelli e per scaricare le sue nuove versioni. Non salva il testo dei campi password.
setup-ready-title = TypeAlike è pronto
setup-ready-body = Provalo ora in qualsiasi app, come la tua email o una chat.
setup-tray-notification-area = TypeAlike resta nell’area di notifica della barra delle applicazioni. Usa la sua icona per sospendere i suggerimenti o aprire le impostazioni.
setup-tray-menu-bar = TypeAlike resta nella barra dei menu. Usa la sua icona per sospendere i suggerimenti o aprire le impostazioni.
