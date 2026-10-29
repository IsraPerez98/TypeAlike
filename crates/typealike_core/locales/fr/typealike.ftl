## The tray menu on Windows, the menu bar menu on macOS and the first entry of the badge menu.

menu-pause = Suspendre les suggestions
menu-resume = Reprendre les suggestions
menu-settings = Réglages…
menu-quit = Quitter TypeAlike

## The name of TypeAlike and its status, in the tooltip of the tray icon and for screen readers at the badge.

status-active = TypeAlike
status-paused = TypeAlike (en pause)
status-off-here = TypeAlike (désactivé ici)
status-energy-saver = TypeAlike (désactivé pour économiser l’énergie)
status-game = TypeAlike (désactivé pendant un jeu)

## The menu of the badge at the caret. $app is the name of an app, and $host is the host of a website, such as mail.example.com.

badge-app-suggestions = Afficher les suggestions dans { $app }
badge-site-suggestions = Afficher les suggestions sur { $host }
badge-app-screen-text = Utiliser le texte de l’écran dans { $app }
badge-app-earlier-writing = Textes précédents dans { $app }

## The choices of the earlier writing of an app, in the Personalization pane and in the badge menu.

history-scope-off = Désactivé
history-scope-this-app = Cette app uniquement
history-scope-all-apps = Toutes les apps

## The list of apps in the Apps and websites, Copied and screen text and Personalization panes.

app-list-heading = Apps
app-list-empty = Aucune suggestion ne s’est encore affichée dans une app.
app-list-footer = Quand une suggestion s’affiche pour la première fois dans une app, TypeAlike ajoute l’app à cette liste.

## The General pane of the settings window.

general-title = Général
general-summary = Choisissez quand TypeAlike s’ouvre et comment les suggestions s’affichent.
general-autostart = Ouvrir avec la session
general-autostart-description = TypeAlike s’ouvre à chaque ouverture de session. Vous n’avez donc pas besoin de l’ouvrir vous-même.
general-language = Langue
general-language-description = Les fenêtres et les menus de TypeAlike s’affichent dans cette langue. Les suggestions restent dans la langue de votre texte.
# The first entry of the language menu. The other entries are the names of the languages, which stay the same in each language.
general-language-system = Langue du système
general-display = Afficher les mots un par un
general-display-description = Chaque mot d’une suggestion s’affiche dès que le modèle l’écrit. Le premier mot s’affiche plus tôt, mais la suggestion s’allonge pendant que vous la lisez.
general-typo-rule = Masquer les suggestions en cas de faute de frappe
general-typo-rule-description = Quand un mot semble contenir une faute de frappe, aucune suggestion ne le complète, pour que la touche Tab n’insère pas la faute. Un nom rare peut aussi ressembler à une faute, et ne reçoit alors aucune suggestion.
general-typo-fix = Corriger les fautes de frappe
general-typo-fix-description = Quand un mot semble contenir une faute de frappe, la suggestion affiche le mot barré, puis le mot corrigé. La touche Tab insère le mot corrigé.
general-length = Longueur des suggestions
general-length-description = Une suggestion s’arrête à cette longueur. Une suggestion plus courte s’affiche plus tôt.
general-length-very-short = Très courte
general-length-short = Courte
general-length-medium = Moyenne
general-length-long = Longue
general-length-very-long = Très longue
general-length-processor-note = Quand le modèle s’exécute sur le processeur, les suggestions sont courtes ou très courtes, pour s’afficher plus tôt.
general-sentence-end = Arrêter à la fin d’une phrase
general-sentence-end-description = Une suggestion se termine après la première phrase qu’elle complète.
general-sentence-end-off-note = Les suggestions sont souvent fausses après la première phrase. Activez cette option pour les arrêter là.
general-reset = Réinitialiser tous les réglages
general-reset-description = Tous les réglages reprennent leurs valeurs par défaut, et la configuration s’ouvre à nouveau. Vos modèles téléchargés et vos textes précédents sont conservés.
general-reset-confirm = Appuyez à nouveau pour réinitialiser

## The menu of the suggestion model and its status, in the General, Models and Playground panes.

model-menu = Modèle de suggestions
model-menu-select = Choisir un modèle
model-menu-none = Aucun modèle téléchargé
model-needed = TypeAlike a besoin d’un modèle pour écrire des suggestions. Téléchargez le modèle recommandé ci-dessous.
model-recommended = Recommandé pour cet ordinateur
model-recommendation-unknown-gpu = TypeAlike ne peut pas lire la mémoire de la carte graphique. Il recommande donc un modèle adapté à la plupart des cartes graphiques.
model-starting = Démarrage du modèle…
model-paused = Les suggestions sont en pause. Pour les reprendre, choisissez « Reprendre les suggestions » dans le menu de TypeAlike.
model-energy-saver = Les suggestions sont désactivées pendant que l’ordinateur économise l’énergie. Pour les utiliser maintenant, désactivez « Arrêter en mode économie d’énergie ».
model-game = Les suggestions se sont arrêtées pour laisser de la mémoire à un jeu. Elles reprennent quand vous tapez dans un champ de texte.
model-idle = Les suggestions se sont arrêtées pour libérer de la mémoire, car vous n’avez ni tapé ni cliqué depuis un moment. Elles reprennent quand vous tapez dans un champ de texte.
model-error-missing = TypeAlike ne trouve pas le fichier du modèle. Téléchargez à nouveau le modèle.
model-error-invalid = TypeAlike ne peut pas utiliser ce modèle. Le fichier est peut-être endommagé, ou d’un type que TypeAlike ne prend pas en charge. Téléchargez-le à nouveau ou choisissez un autre modèle.
model-error-too-large = Le modèle ne tient pas dans la mémoire libre de la carte graphique. Choisissez un modèle plus petit.
model-error-hardware = TypeAlike ne peut pas exécuter le modèle sur cette carte graphique. Dans « Exécuter le modèle sur », choisissez une autre option.
model-error-no-model = Choisissez d’abord un modèle.
model-error-text-too-long = Le texte est trop long pour ce modèle. Utilisez un texte plus court.
model-error-failed = TypeAlike n’a pas pu écrire de suggestion. Réessayez.
model-error-superseded = TypeAlike a arrêté cette suggestion pour en écrire une plus récente.

## The Playground pane of the settings window. $seconds is the time that the model took to write the suggestion, with two digits after the decimal separator, such as 0.42.

playground-title = Bac à sable
playground-summary = Choisissez un modèle, puis tapez le début d’une phrase pour voir ce que le modèle suggère.
playground-text = Texte
playground-text-placeholder = Tapez le début d’une phrase
playground-suggest = Afficher la suggestion
playground-output = Suggestion
playground-writing = Écriture d’une suggestion…
playground-nothing-added = Le modèle n’a pas de suggestion pour ce texte. Tapez d’autres mots, puis réessayez.
playground-time = Écrite en { $seconds } secondes

## The Models pane of the settings window. $developer is the company that made a model, such as Google. $owner is the account on Hugging Face that shares a model. $query is the text that the user searched for.

models-title = Modèles
models-summary = Un modèle sur cet ordinateur écrit les suggestions. Le texte que vous tapez dans d’autres apps ne quitte jamais cet ordinateur. Seuls la recherche et le téléchargement de modèles utilisent Internet.
models-model-description = Un modèle plus petit est plus rapide et utilise moins de mémoire, mais ses suggestions sont plus souvent fausses.
models-hardware = Exécuter le modèle sur
models-hardware-description = Une carte graphique exécute le modèle plus vite que le processeur, mais le modèle entier doit tenir dans sa mémoire.
models-hardware-processor = Processeur (CPU)
models-hardware-graphics-card = Carte graphique (Vulkan)
models-hardware-nvidia = Carte graphique NVIDIA (CUDA)
models-search = Rechercher sur Hugging Face
models-search-placeholder = Nom d’un modèle
models-search-button = Rechercher
models-recommended-heading = Modèles recommandés
models-recommended-footer = En cas de doute, téléchargez le modèle que TypeAlike recommande pour cet ordinateur. Vous pouvez supprimer un modèle à tout moment.
models-developer = Par { $developer }
models-developer-recommended = Par { $developer } · Recommandé pour cet ordinateur
models-searching = Recherche sur Hugging Face…
models-results-heading = Résultats pour « { $query } »
models-results-footer = La liste affiche seulement les modèles que TypeAlike peut utiliser. Les modèles les plus téléchargés sont en haut.
models-no-results = Aucun modèle sur Hugging Face ne correspond à « { $query } ». Essayez d’autres mots.
models-owner = Par { $owner }
models-owner-gated = Par { $owner } · TypeAlike ne peut pas télécharger ce modèle
models-show-files = Afficher les fichiers
models-back-to-results = Retour aux résultats
models-files-loading = Récupération de la liste des fichiers…
models-files-footer = Chaque fichier est le même modèle dans une autre taille. Un fichier plus petit est plus rapide, mais ses suggestions sont plus souvent fausses. En cas de doute, choisissez un fichier Q4_K_M.
models-no-files = Ce modèle n’a aucun fichier que TypeAlike puisse utiliser.
models-downloaded-heading = Modèles téléchargés
models-download = Télécharger
models-cancel-download = Annuler le téléchargement
models-canceling = Annulation du téléchargement…
models-delete = Supprimer
models-try-again = Réessayer
models-error-offline = TypeAlike ne peut pas se connecter à Hugging Face. Vérifiez votre connexion Internet.
models-error-account = TypeAlike ne peut pas télécharger ce modèle, car il nécessite un compte Hugging Face. Choisissez un autre modèle.
models-error-not-found = Hugging Face ne trouve pas ce modèle. Recherchez à nouveau le modèle.
models-error-server = Hugging Face n’est pas disponible. Réessayez plus tard.
models-error-response = Hugging Face a envoyé une réponse incorrecte. Réessayez plus tard.
models-error-storage = TypeAlike ne peut pas enregistrer le modèle. Vérifiez qu’il reste de l’espace libre sur le disque.
models-error-canceled = Le téléchargement s’est arrêté avant la fin.

## The numbers of the Models pane. $count is a count of downloads: a whole number below one thousand, else a number of thousands, millions or billions, with one digit after the decimal separator when it is not zero, such as 1.2 or 15. $percent is a whole number from 0 to 100. $size is a file size: a whole number of bytes, kilobytes or megabytes, or a number of gigabytes with one digit after the decimal separator, such as 2.5.

models-downloads =
    { $count ->
        [one] { $count } téléchargement
       *[other] { $count } téléchargements
    }
models-downloads-thousands = { $count } k téléchargements
models-downloads-millions =
    { $count ->
        [one] { $count } million de téléchargements
       *[other] { $count } millions de téléchargements
    }
models-downloads-billions =
    { $count ->
        [one] { $count } milliard de téléchargements
       *[other] { $count } milliards de téléchargements
    }
models-download-progress = { $percent } % téléchargé
models-size-bytes =
    { $size ->
        [one] { $size } octet
       *[other] { $size } octets
    }
models-size-kilobytes = { $size } Ko
models-size-megabytes = { $size } Mo
models-size-gigabytes = { $size } Go

## The Apps and websites pane of the settings window.

suggestion-places-title = Apps et sites web
suggestion-places-summary = Les suggestions s’affichent dans chaque app et sur chaque site web où vous tapez. Pour arrêter les suggestions dans une app ou sur un site web, désactivez son option.
suggestion-places-websites = Sites web
suggestion-places-no-websites = Aucune suggestion ne s’est encore affichée sur un site web.
suggestion-places-websites-footer = Quand une suggestion s’affiche pour la première fois sur un site web, TypeAlike ajoute le site web à cette liste.

## The Copied and screen text pane of the settings window.

screen-text-title = Texte copié et texte de l’écran
screen-text-summary = Les suggestions peuvent utiliser le texte que vous copiez et le texte près du champ où vous tapez, comme les derniers messages d’une discussion. TypeAlike lit ce texte sur cet ordinateur et ne l’enregistre pas.
screen-text-clipboard = Utiliser le texte copié
screen-text-clipboard-description = Les suggestions peuvent utiliser le texte que vous copiez. TypeAlike le lit seulement pendant que vous utilisez un champ de texte, et ne l’enregistre pas.
screen-text-switch = Utiliser le texte de l’écran
screen-text-switch-description = TypeAlike lit le texte de l’écran dans chaque app dont l’option est activée dans la liste ci-dessous.
screen-text-recognition = Reconnaissance de texte
screen-text-recognition-description = Quand TypeAlike ne peut pas obtenir le texte d’une fenêtre, il le lit dans une image de la fenêtre. « Rapide » lit l’allemand, l’anglais, l’espagnol, le français, l’italien et le portugais. « Précis » lit plus de langues avec moins d’erreurs, mais il est plus lent.
screen-text-recognition-fast = Rapide (recommandé)
screen-text-recognition-accurate = Précis
screen-text-permission = Certaines apps ne donnent pas leur texte à TypeAlike. Pour lire leur texte dans une image de la fenêtre, TypeAlike a besoin de l’autorisation Enregistrement de l’écran et des sons du système.
screen-text-open-system-settings = Ouvrir Réglages Système

## The Personalization pane of the settings window.

personalization-title = Personnalisation
personalization-summary = Les suggestions peuvent utiliser des informations sur vous et apprendre de vos textes précédents. TypeAlike conserve vos textes précédents chiffrés sur cet ordinateur. Il n’enregistre pas le texte des champs de mot de passe.
personalization-name = Votre nom
personalization-name-description = Les suggestions peuvent utiliser votre nom, par exemple pour signer un e-mail.
personalization-notes = Notes sur vous
personalization-notes-description = Écrivez quelques phrases courtes sur vous qui aident les suggestions, comme votre métier et vos langues. Commencez chaque phrase par « Je », par exemple « J’écris des e-mails courts. »
personalization-notes-example = Partir d’un exemple
personalization-notes-example-description = Choisissez l’exemple le plus proche de votre façon d’écrire. Modifiez ensuite les notes ci-dessous.
personalization-history = Utiliser les textes précédents
personalization-history-description = TypeAlike enregistre le texte que vous écrivez dans un fichier chiffré sur cet ordinateur. Les suggestions utilisent alors vos mots et votre style. Si vous désactivez cette option, le texte enregistré reste jusqu’à ce que vous le supprimiez en bas de cette page.
personalization-kept-texts = Texte à enregistrer
personalization-kept-texts-description = TypeAlike enregistre votre texte quand vous le terminez, par exemple quand vous envoyez un message.
personalization-kept-texts-accepted = Seulement le texte où vous avez utilisé une suggestion
personalization-kept-texts-all = Tout le texte
personalization-word-choice = Choix des mots
personalization-word-choice-description = Les suggestions utilisent davantage les mots que vous écrivez souvent. Un réglage plus fort reprend davantage vos mots, mais les suggestions sont alors plus souvent fausses.
personalization-word-choice-off = Désactivé
personalization-word-choice-light = Léger
personalization-word-choice-medium = Moyen
personalization-word-choice-strong = Fort
personalization-word-choice-needs-history = Pour utiliser ce réglage, activez « Utiliser les textes précédents ».
personalization-apps-footer = Quand vous réglez une app sur « Désactivé », TypeAlike supprime le texte qu’il a enregistré depuis cette app. Avec « Cette app uniquement », les textes précédents d’une app servent seulement dans cette app. Les apps réglées sur « Toutes les apps » partagent leurs textes précédents. Quand une suggestion s’affiche pour la première fois dans une app, TypeAlike ajoute l’app à cette liste.
personalization-delete-description = TypeAlike supprime tout le texte qu’il a enregistré. Vous ne pouvez pas annuler cette action.
personalization-delete = Supprimer tous les textes précédents
personalization-delete-confirm = Appuyez à nouveau pour supprimer

## The Energy saving pane of the settings window.

energy-saving-title = Économie d’énergie
energy-saving-summary = TypeAlike peut arrêter les suggestions quand vous ne tapez pas pendant un moment, pendant les jeux et en mode économie d’énergie. Cela libère de la mémoire et économise l’énergie.
energy-saving-idle-timeout = Arrêter après une période sans activité
energy-saving-idle-timeout-description = Si vous ne tapez rien et ne cliquez pas pendant cette durée, les suggestions s’arrêtent pour libérer de la mémoire. Elles reprennent quand vous tapez dans un champ de texte.
energy-saving-idle-timeout-off = Jamais
energy-saving-idle-timeout-15 = 15 minutes
energy-saving-idle-timeout-30 = 30 minutes
energy-saving-idle-timeout-60 = 60 minutes
energy-saving-energy-saver = Arrêter en mode économie d’énergie
energy-saving-energy-saver-description = Quand l’économiseur d’énergie ou le mode Économie d’énergie est activé, les suggestions s’arrêtent pour économiser l’énergie. Elles reprennent quand il se désactive.
energy-saving-energy-saver-stopped = Les suggestions sont désactivées pendant que l’ordinateur économise l’énergie.
energy-saving-game = Arrêter pendant les jeux
energy-saving-game-description = Pendant que vous jouez, les suggestions s’arrêtent pour laisser de la mémoire au jeu. Elles reprennent quand vous tapez dans un champ de texte.
energy-saving-preparation = Préparer les suggestions à l’avance
energy-saving-preparation-description = Quand vous arrivez dans un champ de texte, le modèle lit tout de suite son texte. La première suggestion s’affiche alors plus tôt, mais cela consomme plus d’énergie.

## The Permissions pane of the settings window. It shows only on an OS that asks the user for permissions (macOS). The labels are the names of the permissions in System Settings.

permissions-title = Autorisations
permissions-summary = TypeAlike utilise ces autorisations pour afficher des suggestions dans d’autres apps. Vous les accordez dans Réglages Système.
permissions-accessibility = Accessibilité
permissions-accessibility-description = TypeAlike a besoin de cette autorisation pour lire le texte où vous tapez et pour insérer une suggestion quand vous appuyez sur Tab. Sans elle, aucune suggestion ne s’affiche.
permissions-screen-recording = Enregistrement de l’écran et des sons du système
permissions-screen-recording-description = Certaines apps ne donnent pas leur texte à TypeAlike. Avec cette autorisation, TypeAlike lit leur texte dans une image de la fenêtre. Les suggestions fonctionnent aussi sans elle.
permissions-granted = Autorisé
permissions-give-access = Autoriser l’accès
permissions-footer = Quand Réglages Système vous demande de quitter et de rouvrir TypeAlike, faites-le pour que TypeAlike puisse utiliser la nouvelle autorisation.

## The examples of the notes about the writer, in the menu “Start from an example” of the Personalization pane and of the setup. The text of each example goes into the notes, in the first person, and the model reads it.

notes-example-own = Vos propres notes
notes-example-general = Écriture de tous les jours
notes-example-general-text = J’écris des messages clairs et chaleureux. Je garde le ton de la conversation.
notes-example-work = E-mails professionnels
notes-example-work-text = J’écris des e-mails et des messages au travail. J’utilise un ton poli et professionnel, et mes messages sont courts et clairs.
notes-example-support = Service client
notes-example-support-text = Je réponds aux questions des clients. Je reste aimable et calme, et j’explique clairement les étapes pour résoudre le problème.
notes-example-chats = Discussions avec les amis et la famille
notes-example-chats-text = Je discute avec mes amis et ma famille. J’écris des messages courts et décontractés.
notes-example-study = École et études
notes-example-study-text = Je fais des études. J’écris des dissertations, des notes et des e-mails à mes professeurs dans un style clair et correct.
notes-example-software = Développement logiciel
notes-example-software-text = Je travaille dans le développement logiciel. J’écris des revues de code, des rapports de bugs et des documents techniques. J’utilise les noms exacts du code et des outils.

## The setup, which shows when TypeAlike starts for the first time and after a reset of the settings. $number is the number of the step, and $count is the number of steps. The demo shows the typed text, then the suggestion after it in gray, then the key that types its next word.

setup-window-title = Configuration de TypeAlike
setup-progress = Étape { $number } sur { $count }
setup-back = Retour
setup-continue = Continuer
setup-start = Commencer
setup-finish = Terminer
setup-welcome-title = Bienvenue dans TypeAlike
setup-welcome-body = TypeAlike suggère les mots suivants pendant que vous tapez, dans n’importe quelle app. Appuyez sur Tab pour insérer le mot suivant, ou continuez à taper pour ignorer la suggestion.
setup-demo-typed = Merci pour votre réponse rapide. Je vous envoie le
setup-demo-suggestion = rapport d’ici vendredi.
setup-demo-key = Tab
setup-model-title = Téléchargez le modèle
setup-model-select-title = Choisissez le modèle
setup-model-footer = TypeAlike a besoin de ce modèle pour écrire des suggestions. Vous le téléchargez une seule fois. La configuration continue à la fin du téléchargement.
setup-model-checking = Vérification de la carte graphique…
setup-other-models = Choisir un autre modèle
setup-permission-title = Donnez à TypeAlike l’accès aux champs de texte
setup-permission-body = Cliquez sur « Autoriser l’accès ». Activez ensuite TypeAlike dans Réglages Système, puis revenez à cette fenêtre.
setup-about-title = Présentez-vous à TypeAlike
setup-sources-title = Ce que les suggestions peuvent utiliser
setup-history-description = TypeAlike enregistre ce que vous écrivez dans un fichier chiffré sur cet ordinateur. Les suggestions utilisent alors vos mots et votre style. Vous pouvez supprimer ce texte dans la page Personnalisation.
setup-screen-text-description = TypeAlike lit le texte près du champ où vous tapez, comme les derniers messages d’une discussion. Il n’enregistre pas ce texte.
setup-sources-footer = TypeAlike utilise Internet seulement pour trouver et télécharger des modèles. Il n’enregistre pas le texte des champs de mot de passe.
setup-ready-title = TypeAlike est prêt
setup-ready-body = Essayez-le maintenant dans n’importe quelle app, comme votre messagerie ou une discussion.
setup-tray-notification-area = TypeAlike reste dans la zone de notification de la barre des tâches. Utilisez son icône pour suspendre les suggestions ou ouvrir les réglages.
setup-tray-menu-bar = TypeAlike reste dans la barre des menus. Utilisez son icône pour suspendre les suggestions ou ouvrir les réglages.
