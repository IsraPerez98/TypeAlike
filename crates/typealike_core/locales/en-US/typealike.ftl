## The tray menu on Windows, the menu bar menu on macOS and the first entry of the badge menu.

menu-pause = Pause suggestions
menu-resume = Resume suggestions
menu-settings = Settings…
menu-quit = Quit TypeAlike

## The name of TypeAlike and its status, in the tooltip of the tray icon and for screen readers at the badge.

status-active = TypeAlike
status-paused = TypeAlike (paused)
status-off-here = TypeAlike (off here)
status-energy-saver = TypeAlike (off to save energy)
status-game = TypeAlike (off during a game)

## The menu of the badge at the caret. $app is the name of an app, and $host is the host of a website, such as mail.example.com.

badge-app-suggestions = Show suggestions in { $app }
badge-site-suggestions = Show suggestions on { $host }
badge-app-screen-text = Use screen text in { $app }
badge-app-earlier-writing = Earlier writing in { $app }

## The choices of the earlier writing of an app, in the Personalization pane and in the badge menu.

history-scope-off = Off
history-scope-this-app = Only this app
history-scope-all-apps = All apps

## The list of apps in the Apps and websites, Copied and screen text and Personalization panes.

app-list-heading = Apps
app-list-empty = No suggestion showed in an app yet.
app-list-footer = When a suggestion shows in an app for the first time, TypeAlike adds the app to this list.

## The General pane of the settings window.

general-title = General
general-summary = Change when TypeAlike starts and how suggestions show.
general-autostart = Open at login
general-autostart-description = TypeAlike starts each time you log in, so you do not need to start it yourself.
general-display = Show words early
general-display-description = Each word of a suggestion shows as soon as the model writes it. The first word then shows sooner, but the suggestion grows while you read it.
general-typo-rule = Hide suggestions on typos
general-typo-rule-description = When a word looks like it has a typo, no suggestion finishes it, so that Tab does not type the typo. A rare name can also look like a typo, and then gets no suggestion.
general-typo-fix = Fix typos
general-typo-fix-description = When a word looks like it has a typo, the suggestion shows the word with a line through it, then the fixed word. Tab types the fixed word.
general-length = Suggestion length
general-length-description = A suggestion stops at this length. A shorter suggestion shows sooner.
general-length-very-short = Very short
general-length-short = Short
general-length-medium = Medium
general-length-long = Long
general-length-very-long = Very long
general-length-processor-note = When the model runs on the processor, suggestions are short or very short, so that they show sooner.
general-sentence-end = Stop at the end of a sentence
general-sentence-end-description = A suggestion ends after the first sentence that it finishes.
general-sentence-end-off-note = Suggestions are often wrong after the first sentence. Turn on this switch to stop them there.
general-reset = Reset all settings
general-reset-description = All settings go back to their first values, and the setup opens again. Your models and your earlier writing stay.
general-reset-confirm = Press again to reset

## The menu of the suggestion model and its status, in the General, Models and Playground panes.

model-menu = Suggestion model
model-menu-select = Select a model
model-menu-none = No downloaded models
model-needed = TypeAlike needs a model to write suggestions. Download the recommended model below.
model-recommended = Recommended for this computer
model-recommendation-unknown-gpu = TypeAlike cannot read the graphics card of this computer. Thus it recommends a model that fits in most graphics cards.
model-starting = Starting the model…
model-paused = Suggestions are paused. To resume them, select “Resume suggestions” in the TypeAlike menu.
model-energy-saver = Suggestions are off while the computer saves energy. To use them now, turn off “Stop in energy saver”.
model-game = Suggestions stopped to free memory for a game. They start again when you type in a text field.
model-idle = Suggestions stopped to free memory, because you did not type or click for some time. They start again when you type in a text field.
model-error-missing = TypeAlike cannot find the model file. Download the model again.
model-error-invalid = TypeAlike cannot use this model. The file can be damaged, or of a type that TypeAlike does not support. Download it again, or select another model.
model-error-too-large = The model does not fit in the free memory of the graphics card. Select a smaller model.
model-error-hardware = TypeAlike cannot run the model on this graphics card. In “Run the model on”, select another choice.
model-error-no-model = Select a model first.
model-error-text-too-long = The text is too long for this model. Use shorter text.
model-error-failed = TypeAlike could not write a suggestion. Try again.
model-error-superseded = TypeAlike stopped this suggestion to write a newer one.

## The Playground pane of the settings window. $seconds is the time that the model took to write the suggestion, with two digits after the decimal separator, such as 0.42.

playground-title = Playground
playground-summary = Select a model, then type the start of a sentence to see what the model suggests.
playground-text = Text
playground-text-placeholder = Type the start of a sentence
playground-suggest = Show suggestion
playground-output = Suggestion
playground-writing = Writing a suggestion…
playground-nothing-added = The model has no suggestion for this text. Type more words, then try again.
playground-time = Written in { $seconds } seconds

## The Models pane of the settings window. $developer is the company that made a model, such as Google. $owner is the account on Hugging Face that shares a model. $query is the text that the user searched for.

models-title = Models
models-summary = A model on this computer writes the suggestions. The text that you type in other apps never leaves this computer. Only the search and the downloads of models use the internet.
models-model-description = A smaller model is faster and needs less memory, but its suggestions are wrong more often.
models-hardware = Run the model on
models-hardware-description = A graphics card runs the model faster than the processor, but the whole model must fit in its memory.
models-hardware-processor = Processor (CPU)
models-hardware-graphics-card = Graphics card (Vulkan)
models-hardware-nvidia = NVIDIA graphics card (CUDA)
models-search = Search Hugging Face
models-recommended-heading = Recommended models
models-recommended-footer = If you are not sure, download the model that TypeAlike recommends for this computer. You can delete a model at any time.
models-developer = By { $developer }
models-developer-recommended = By { $developer } · Recommended for this computer
models-searching = Searching Hugging Face…
models-results-heading = Results for “{ $query }”
models-results-footer = The list shows only the models that TypeAlike can use. The models with the most downloads are at the top.
models-no-results = No models on Hugging Face match “{ $query }”. Try other words.
models-owner = By { $owner }
models-owner-gated = By { $owner } · TypeAlike cannot download this model
models-show-files = Show files
models-back-to-results = Back to results
models-files-loading = Getting the list of files…
models-files-footer = Each file is the same model in a different size. A smaller file runs faster, but its suggestions are wrong more often. If you are not sure, select a Q4_K_M file.
models-no-files = This model has no files that TypeAlike can use.
models-downloaded-heading = Downloaded models
models-download = Download
models-cancel-download = Cancel download
models-canceling = Canceling the download…
models-delete = Delete
models-try-again = Try again
models-error-offline = TypeAlike cannot connect to Hugging Face. Check your internet connection.
models-error-account = TypeAlike cannot download this model, because it needs a Hugging Face account. Select another model.
models-error-not-found = Hugging Face cannot find this model. Search for the model again.
models-error-server = Hugging Face is not available. Try again later.
models-error-response = Hugging Face sent an incorrect response. Try again later.
models-error-storage = TypeAlike cannot save the model. Make sure that the disk has free space.
models-error-canceled = The download stopped before it finished.

## The numbers of the Models pane. $count is a count of downloads: a whole number below one thousand, else a number of thousands, millions or billions, with one digit after the decimal separator when it is not zero, such as 1.2 or 15. $percent is a whole number from 0 to 100. $size is a file size: a whole number of bytes, kilobytes or megabytes, or a number of gigabytes with one digit after the decimal separator, such as 2.5.

models-downloads =
    { $count ->
        [one] { $count } download
       *[other] { $count } downloads
    }
models-downloads-thousands = { $count }K downloads
models-downloads-millions = { $count }M downloads
models-downloads-billions = { $count }B downloads
models-download-progress = { $percent }% downloaded
models-size-bytes =
    { $size ->
        [one] { $size } byte
       *[other] { $size } bytes
    }
models-size-kilobytes = { $size } KB
models-size-megabytes = { $size } MB
models-size-gigabytes = { $size } GB

## The Apps and websites pane of the settings window.

suggestion-places-title = Apps and websites
suggestion-places-summary = Suggestions show in each app and on each website where you type. To stop suggestions in an app or on a website, turn off its switch.
suggestion-places-websites = Websites
suggestion-places-no-websites = No suggestion showed on a website yet.
suggestion-places-websites-footer = When a suggestion shows on a website for the first time, TypeAlike adds the website to this list.

## The Copied and screen text pane of the settings window.

screen-text-title = Copied and screen text
screen-text-summary = Suggestions can use the text that you copy and the text near the field where you type, such as the last messages of a chat. TypeAlike reads this text on this computer and does not save it.
screen-text-clipboard = Use copied text
screen-text-clipboard-description = Suggestions can use the text that you copy. TypeAlike reads it only while you use a text field, and does not save it.
screen-text-switch = Use screen text
screen-text-switch-description = TypeAlike reads the screen text in each app whose switch is on in the list below.
screen-text-recognition = Text recognition
screen-text-recognition-description = When TypeAlike cannot get the text of a window, it reads the text from an image of the window. “Fast” reads English, French, German, Italian, Portuguese and Spanish. “Accurate” reads more languages with fewer errors, but it is slower.
screen-text-recognition-fast = Fast (recommended)
screen-text-recognition-accurate = Accurate
screen-text-permission = Some apps do not give their text to TypeAlike. To read their text from an image of the window, TypeAlike needs the Screen & System Audio Recording permission.
screen-text-open-system-settings = Open System Settings

## The Personalization pane of the settings window.

personalization-title = Personalization
personalization-summary = Suggestions can use facts about you and learn from your earlier writing. TypeAlike keeps the earlier writing encrypted on this computer. It does not save the text of password fields.
personalization-name = Your name
personalization-name-description = Suggestions can use your name, for example to sign an email.
personalization-notes = Notes about you
personalization-notes-description = Write a few short facts that help suggestions, such as your job and your languages. Start each fact with “I”, for example “I keep my emails short.”
personalization-notes-example = Start from an example
personalization-notes-example-description = Select the example that is nearest to your writing. Then change the notes so that they tell more about you.
personalization-history = Use earlier writing
personalization-history-description = TypeAlike saves the text that you write in an encrypted file on this computer. Suggestions then use your words and your style. When you turn this off, the saved text stays until you delete it at the bottom of this page.
personalization-kept-texts = Text to save
personalization-kept-texts-description = TypeAlike saves your text when you finish it, for example when you send a message.
personalization-kept-texts-accepted = Only text where you used a suggestion
personalization-kept-texts-all = All text
personalization-word-choice = Word choice
personalization-word-choice-description = Suggestions use more of the words that you often write. A stronger setting gives more of your words, but suggestions are then wrong more often.
personalization-word-choice-off = Off
personalization-word-choice-light = Light
personalization-word-choice-medium = Medium
personalization-word-choice-strong = Strong
personalization-word-choice-needs-history = To use this setting, turn on “Use earlier writing”.
personalization-apps-footer = When you set an app to “Off”, TypeAlike deletes the text that it saved from that app. With “Only this app”, the earlier writing of an app helps only in that app. The apps with “All apps” share their earlier writing. When a suggestion shows in an app for the first time, TypeAlike adds the app to this list.
personalization-delete-description = TypeAlike deletes all the text that it saved. You cannot undo this.
personalization-delete = Delete all earlier writing
personalization-delete-confirm = Press again to delete

## The Energy saving pane of the settings window.

energy-saving-title = Energy saving
energy-saving-summary = TypeAlike can stop suggestions when you do not type for some time, during games and in energy saver. This frees memory and saves power.
energy-saving-idle-timeout = Stop after a time without use
energy-saving-idle-timeout-description = When you do not type or click for this time, suggestions stop to free memory. They start again when you type in a text field.
energy-saving-idle-timeout-off = Off
energy-saving-idle-timeout-15 = 15 minutes
energy-saving-idle-timeout-30 = 30 minutes
energy-saving-idle-timeout-60 = 60 minutes
energy-saving-energy-saver = Stop in energy saver
energy-saving-energy-saver-description = While energy saver or Low Power Mode is on, suggestions stop to save power. They start again when it goes off.
energy-saving-energy-saver-stopped = Suggestions are off while the computer saves energy.
energy-saving-game = Stop during games
energy-saving-game-description = While you play a game, suggestions stop to free memory for the game. They start again when you type in a text field.
energy-saving-preparation = Prepare suggestions early
energy-saving-preparation-description = When you go to a text field, the model reads the text in it at once. The first suggestion then shows sooner, but this uses more power.

## The Permissions pane of the settings window. It shows only on an OS that asks the user for permissions (macOS). The labels are the names of the permissions in System Settings.

permissions-title = Permissions
permissions-summary = TypeAlike uses these permissions to show suggestions in other apps. You give them in System Settings.
permissions-accessibility = Accessibility
permissions-accessibility-description = TypeAlike needs this permission to read the text where you type and to type a suggestion when you press Tab. Without it, no suggestion shows.
permissions-screen-recording = Screen & System Audio Recording
permissions-screen-recording-description = Some apps do not give their text to TypeAlike. With this permission, TypeAlike reads their text from an image of the window. Suggestions also work without it.
permissions-granted = Allowed
permissions-give-access = Give access
permissions-footer = When System Settings asks you to quit and reopen TypeAlike, do it, so that TypeAlike can use the new permission.

## The examples of the notes about the writer, in the menu “Start from an example” of the Personalization pane and of the setup. The text of each example goes into the notes, in the first person, and the model reads it.

notes-example-own = Your own notes
notes-example-general = General
notes-example-general-text = I write clear and friendly messages. I keep the tone of the conversation.
notes-example-work = Work emails
notes-example-work-text = I write emails and messages at work. I use a polite and professional tone, and I keep my messages short and clear.
notes-example-support = Customer support
notes-example-support-text = I answer the questions of customers. I am friendly and patient, and I give clear steps that solve the problem.
notes-example-chats = Chats with friends and family
notes-example-chats-text = I chat with friends and family. I write short and casual messages.
notes-example-study = School and study
notes-example-study-text = I am a student. I write essays, notes and emails to my teachers in a clear and correct style.
notes-example-software = Software development
notes-example-software-text = I am a software developer. I write code reviews, bug reports and technical documents. I use the exact names of code and tools.

## The setup, which shows when TypeAlike starts for the first time and after a reset of the settings. $number is the number of the step, and $count is the number of steps. The demo shows the typed text, then the suggestion after it in gray, then the key that types the suggestion.

setup-window-title = TypeAlike setup
setup-progress = Step { $number } of { $count }
setup-back = Back
setup-continue = Continue
setup-start = Start setup
setup-finish = Finish setup
setup-welcome-title = Welcome to TypeAlike
setup-welcome-body = TypeAlike suggests the next words while you type in any app. Press Tab to type the suggestion. To ignore it, keep typing.
setup-welcome-privacy = The model runs on this computer. The text that you type never leaves it.
setup-demo-typed = Thanks for the quick reply. I will send you the
setup-demo-suggestion = report by Friday.
setup-demo-key = Tab
setup-model-title = Download the model
setup-model-body = A model on this computer writes the suggestions. TypeAlike recommends the model that fits this computer.
setup-model-footer = You can continue the setup while the model downloads. Later, you can download other models in the settings.
setup-model-checking = Checking the graphics card…
setup-model-ready = The model is on this computer. You can select another model later in the settings.
setup-permission-title = Allow access to text fields
setup-permission-body = Click “Give access”. Then turn on TypeAlike in System Settings, and come back to this window.
setup-about-title = Tell TypeAlike about you
setup-about-body = Suggestions use your name and your notes to write as you do. You can change them later in the settings.
setup-sources-heading = What suggestions can use
setup-history-description = TypeAlike saves what you write in an encrypted file on this computer. Suggestions then use your words and your style. You can delete this text in the settings.
setup-screen-text-description = TypeAlike reads the text near the field where you type, such as the last messages of a chat. It does not save this text.
setup-sources-footer = TypeAlike does not save the text of password fields.
setup-ready-title = TypeAlike is ready
setup-ready-body = Suggestions show in each app where you type.
setup-downloading-title = TypeAlike is almost ready
setup-downloading-body = Suggestions start when the model download finishes.
setup-no-model-title = TypeAlike needs a model
setup-no-model-body = Download the recommended model below. Suggestions start when the download finishes.
setup-ready-step-type = Type in any app, such as your email or a chat.
setup-ready-step-tab = When gray text shows after your text, press Tab to type its next word.
setup-ready-step-ignore = To ignore a suggestion, keep typing.
setup-tray-notification-area = TypeAlike stays in the notification area of the taskbar. Use its icon to pause suggestions or to open the settings.
setup-tray-menu-bar = TypeAlike stays in the menu bar. Use its icon to pause suggestions or to open the settings.
