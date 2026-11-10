## The tray menu on Windows, the menu bar menu on macOS and the first entry of the badge menu.

menu-pause = Pausar las sugerencias
menu-resume = Reanudar las sugerencias
menu-settings = Ajustes…
menu-quit = Salir de TypeAlike

## The name of TypeAlike and its status, in the tooltip of the tray icon and for screen readers at the badge.

status-active = TypeAlike
status-paused = TypeAlike (en pausa)
status-off-here = TypeAlike (desactivado aquí)
status-energy-saver = TypeAlike (desactivado mientras el equipo ahorra energía)
status-game = TypeAlike (desactivado mientras juegas)
status-full-screen = TypeAlike (desactivado mientras una app ocupa toda la pantalla)

## The menu of the badge at the caret. $app is the name of an app, and $host is the host of a website, such as mail.example.com.

badge-app-suggestions = Mostrar sugerencias en { $app }
badge-site-suggestions = Mostrar sugerencias en { $host }
badge-private-windows = Detener las sugerencias en ventanas privadas
badge-app-screen-text = Usar el texto de la pantalla en { $app }
badge-app-earlier-writing = Escritura anterior en { $app }

## The choices of the earlier writing of an app, in the Personalization pane and in the badge menu.

history-scope-off = Desactivada
history-scope-this-app = Solo esta app
history-scope-all-apps = Todas las apps

## The list of apps in the Apps and websites, Copied and screen text and Personalization panes.

app-list-heading = Apps
app-list-empty = Todavía no apareció ninguna sugerencia en ninguna app.
app-list-footer = Cuando una sugerencia aparece por primera vez en una app, TypeAlike agrega la app a esta lista.

## The General pane of the settings window.

general-title = General
general-summary = Cambia cuándo se abre TypeAlike y cómo aparecen las sugerencias.
general-autostart = Abrir al iniciar sesión
general-autostart-description = TypeAlike se abre cada vez que inicias sesión, así que no tienes que abrirlo tú.
general-language = Idioma
general-language-description = Las ventanas y los menús de TypeAlike aparecen en este idioma. Las sugerencias siguen en el idioma de tu texto.
# The first entry of the language menu. The other entries are the names of the languages, which stay the same in each language.
general-language-system = Idioma del sistema
general-display = Mostrar las palabras una a una
general-display-description = Cada palabra de una sugerencia aparece en cuanto el modelo la escribe. La primera palabra aparece antes, pero la sugerencia crece mientras la lees.
general-typo-rule = Ocultar sugerencias con erratas
general-typo-rule-description = Cuando una palabra parece tener una errata, ninguna sugerencia la completa, para que Tab no escriba la errata. Un nombre poco común también puede parecer una errata, y entonces no recibe sugerencias.
general-typo-fix = Corregir erratas
general-typo-fix-description = Cuando una palabra parece tener una errata, la sugerencia muestra la palabra tachada y luego la palabra corregida. Tab escribe la palabra corregida.
general-length = Longitud de las sugerencias
general-length-description = Una sugerencia se detiene en esta longitud. Una sugerencia más corta aparece antes.
general-length-very-short = Muy corta
general-length-short = Corta
general-length-medium = Media
general-length-long = Larga
general-length-very-long = Muy larga
general-length-processor-note = Cuando el modelo se ejecuta en el procesador, las sugerencias son cortas o muy cortas, para que aparezcan antes.
general-sentence-end = Detener al final de una oración
general-sentence-end-description = Una sugerencia termina después de la primera oración que completa.
general-sentence-end-off-note = Las sugerencias suelen ser incorrectas después de la primera oración. Activa este interruptor para detenerlas ahí.
general-reset = Restablecer todos los ajustes
general-reset-description = Todos los ajustes vuelven a sus valores predeterminados y la configuración inicial se abre otra vez. Tus modelos descargados y tu escritura anterior se mantienen.
general-reset-confirm = Presiona otra vez para restablecer

## The Languages and typos pane of the settings window, and the step of the setup that shows it. The menu "Add a language" lists the names of the languages, which stay the same in each language. $percent is a whole number from 0 to 100.

languages-title = Idiomas y erratas
languages-summary = Elige los idiomas en los que escribes y qué hacen las sugerencias con las erratas y las palabras desconocidas.
languages-heading = Idiomas en los que escribes
languages-add = Añadir un idioma
languages-add-description = TypeAlike revisa la ortografía de las sugerencias con el diccionario de cada idioma.
languages-add-description-download = Si este equipo no tiene un diccionario para un idioma, TypeAlike descarga uno.
# The first entry of the menu "Add a language". It adds no language.
languages-add-choose = Elige un idioma
languages-remove = Quitar
languages-cancel = Cancelar la descarga
languages-try-again = Reintentar
languages-downloading = { $percent } % del diccionario descargado
languages-no-dictionary = TypeAlike no tiene un diccionario para este idioma, así que no revisa la ortografía de las sugerencias en él.
languages-error-offline = TypeAlike no puede descargar el diccionario. Revisa tu conexión a internet.
languages-error-server = TypeAlike no puede obtener el diccionario ahora. Inténtalo de nuevo más tarde.
languages-error-storage = TypeAlike no puede guardar el diccionario en este equipo. Asegúrate de que el disco tenga espacio libre.
languages-typos-heading = Erratas y palabras desconocidas
languages-spell-check = Revisar la ortografía de las sugerencias
languages-spell-check-description = Una sugerencia se detiene antes de una palabra que ningún diccionario de tus idiomas conoce, como una palabra que el modelo inventó. La jerga y los nombres poco comunes también pueden detener una sugerencia.

## The menu of the suggestion model and its status, in the General, Models and Playground panes.

model-menu = Modelo de sugerencias
model-menu-select = Selecciona un modelo
model-menu-none = No hay modelos descargados
model-needed = TypeAlike necesita un modelo para escribir sugerencias. Descarga el modelo recomendado aquí abajo.
model-recommended = Recomendado para este equipo
model-recommendation-unknown-gpu = TypeAlike no puede leer la memoria de la tarjeta gráfica, así que recomienda un modelo que cabe en la mayoría de las tarjetas gráficas.
model-starting = Iniciando el modelo…
model-paused = Las sugerencias están en pausa. Para reanudarlas, selecciona «Reanudar las sugerencias» en el menú de TypeAlike.
model-energy-saver = Las sugerencias están desactivadas mientras el equipo ahorra energía. Para usarlas ahora, desactiva «Detener con ahorro de energía».
model-game = Las sugerencias se detuvieron para dejar memoria libre a un juego. Vuelven cuando escribes en un campo de texto.
model-full-screen = Las sugerencias se detuvieron para dejar memoria libre a una app en pantalla completa. Vuelven cuando escribes en un campo de texto.
model-idle = Las sugerencias se detuvieron para liberar memoria, porque no escribiste ni hiciste clic durante un tiempo. Vuelven cuando escribes en un campo de texto.
model-error-missing = TypeAlike no encuentra el archivo del modelo. Vuelve a descargar el modelo.
model-error-invalid = TypeAlike no puede usar este modelo. El archivo puede estar dañado o ser de un tipo que TypeAlike no admite. Vuelve a descargarlo o selecciona otro modelo.
model-error-too-large = El modelo no cabe en la memoria libre de la tarjeta gráfica. Selecciona un modelo más pequeño.
model-error-hardware = TypeAlike no puede ejecutar el modelo en esta tarjeta gráfica. En «Ejecutar el modelo en», selecciona otra opción.
model-error-no-model = Primero selecciona un modelo.
model-error-text-too-long = El texto es demasiado largo para este modelo. Usa un texto más corto.
model-error-failed = TypeAlike no pudo escribir una sugerencia. Inténtalo de nuevo.
model-error-superseded = TypeAlike detuvo esta sugerencia para escribir una más reciente.

## The Playground pane of the settings window. $seconds is the time that the model took to write the suggestion, with two digits after the decimal separator, such as 0.42.

playground-title = Zona de pruebas
playground-summary = Selecciona un modelo y escribe el comienzo de una oración para ver qué sugiere el modelo.
playground-text = Texto
playground-text-placeholder = Escribe el comienzo de una oración
playground-suggest = Mostrar sugerencia
playground-output = Sugerencia
playground-writing = Escribiendo una sugerencia…
playground-nothing-added = El modelo no tiene una sugerencia para este texto. Escribe más palabras y vuelve a intentarlo.
playground-time = Escrita en { $seconds } segundos

## The Models pane of the settings window. $developer is the company that made a model, such as Google. $owner is the account on Hugging Face that shares a model. $query is the text that the user searched for.

models-title = Modelos
models-summary = Un modelo en este equipo escribe las sugerencias. El texto que escribes en otras apps nunca sale de este equipo. La búsqueda y las descargas de modelos usan internet.
models-model-description = Un modelo más pequeño es más rápido y necesita menos memoria, pero sus sugerencias son incorrectas más a menudo.
models-hardware = Ejecutar el modelo en
models-hardware-description = Una tarjeta gráfica ejecuta el modelo más rápido que el procesador, pero todo el modelo debe caber en su memoria.
models-hardware-processor = Procesador (CPU)
models-hardware-graphics-card = Tarjeta gráfica (Vulkan)
models-hardware-nvidia = Tarjeta gráfica NVIDIA (CUDA)
models-search = Buscar en Hugging Face
models-search-placeholder = Nombre de un modelo
models-search-button = Buscar
models-recommended-heading = Modelos recomendados
models-recommended-footer = Si no estás seguro, descarga el modelo que TypeAlike recomienda para este equipo. Puedes eliminar un modelo en cualquier momento.
models-developer = De { $developer }
models-developer-recommended = De { $developer } · Recomendado para este equipo
models-searching = Buscando en Hugging Face…
models-results-heading = Resultados de «{ $query }»
models-results-footer = La lista muestra solo los modelos que TypeAlike puede usar. Los modelos con más descargas están arriba.
models-no-results = Ningún modelo de Hugging Face coincide con «{ $query }». Prueba con otras palabras.
models-owner = De { $owner }
models-owner-gated = De { $owner } · TypeAlike no puede descargar este modelo
models-show-files = Mostrar archivos
models-back-to-results = Volver a los resultados
models-files-loading = Obteniendo la lista de archivos…
models-files-footer = Cada archivo es el mismo modelo en otro tamaño. Un archivo más pequeño funciona más rápido, pero sus sugerencias son incorrectas más a menudo. Si no estás seguro, selecciona un archivo Q4_K_M.
models-no-files = Este modelo no tiene archivos que TypeAlike pueda usar.
models-downloaded-heading = Modelos descargados
models-download = Descargar
models-cancel-download = Cancelar la descarga
models-canceling = Cancelando la descarga…
models-delete = Eliminar
models-try-again = Reintentar
models-error-offline = TypeAlike no puede conectarse a Hugging Face. Revisa tu conexión a internet.
models-error-account = TypeAlike no puede descargar este modelo, porque necesita una cuenta de Hugging Face. Selecciona otro modelo.
models-error-not-found = Hugging Face no encuentra este modelo. Vuelve a buscar el modelo.
models-error-server = Hugging Face no está disponible. Inténtalo de nuevo más tarde.
models-error-response = Hugging Face envió una respuesta incorrecta. Inténtalo de nuevo más tarde.
models-error-storage = TypeAlike no puede guardar el modelo. Asegúrate de que el disco tenga espacio libre.
models-error-canceled = La descarga se detuvo antes de terminar.

## The numbers of the Models pane. $count is a count of downloads: a whole number below one thousand, else a number of thousands, millions or billions, with one digit after the decimal separator when it is not zero, such as 1.2 or 15. $percent is a whole number from 0 to 100. $size is a file size: a whole number of bytes, kilobytes or megabytes, or a number of gigabytes with one digit after the decimal separator, such as 2.5.

models-downloads =
    { $count ->
        [one] { $count } descarga
       *[other] { $count } descargas
    }
models-downloads-thousands = { $count } mil descargas
models-downloads-millions =
    { $count ->
        [one] { $count } millón de descargas
       *[other] { $count } millones de descargas
    }
models-downloads-billions = { $count } mil millones de descargas
models-download-progress = { $percent } % descargado
models-size-bytes =
    { $size ->
        [one] { $size } byte
       *[other] { $size } bytes
    }
models-size-kilobytes = { $size } KB
models-size-megabytes = { $size } MB
models-size-gigabytes = { $size } GB

## The Apps and websites pane of the settings window.

suggestion-places-title = Apps y sitios web
suggestion-places-summary = Las sugerencias aparecen en cada app y en cada sitio web donde escribes. Para detener las sugerencias en una app o en un sitio web, desactiva su interruptor.
suggestion-places-websites = Sitios web
suggestion-places-no-websites = Todavía no apareció ninguna sugerencia en ningún sitio web.
suggestion-places-websites-footer = Cuando una sugerencia aparece por primera vez en un sitio web, TypeAlike agrega el sitio web a esta lista.
suggestion-places-private-windows = Detener en ventanas privadas
suggestion-places-private-windows-description = En una ventana privada o de incógnito de un navegador web, TypeAlike no muestra sugerencias, no lee el texto de la pantalla y no guarda nada de lo que escribes.

## The Copied and screen text pane of the settings window.

screen-text-title = Texto copiado y de la pantalla
screen-text-summary = Las sugerencias pueden usar el texto que copias y el texto cerca del campo donde escribes, como los últimos mensajes de un chat. TypeAlike lee este texto en este equipo. Solo guarda algunas palabras de los mensajes que respondes, junto con tu escritura anterior.
screen-text-clipboard = Usar el texto copiado
screen-text-clipboard-description = Las sugerencias pueden usar el texto que copias. TypeAlike lo lee solo mientras usas un campo de texto y no lo guarda.
screen-text-switch = Usar el texto de la pantalla
screen-text-switch-description = TypeAlike lee el texto de la pantalla en cada app con el interruptor activado en la lista de abajo.
screen-text-recognition = Reconocimiento de texto
screen-text-recognition-description = Cuando TypeAlike no puede obtener el texto de una ventana, lo lee de una imagen de la ventana. «Rápido» lee alemán, español, francés, inglés, italiano y portugués. «Preciso» lee más idiomas con menos errores, pero es más lento.
screen-text-recognition-fast = Rápido (recomendado)
screen-text-recognition-accurate = Preciso
screen-text-permission = Algunas apps no dan su texto a TypeAlike. Para leerlo de una imagen de la ventana, TypeAlike necesita el permiso de Grabación de pantalla y del audio del sistema.
screen-text-open-system-settings = Abrir Ajustes del Sistema

## The Personalization pane of the settings window.

personalization-title = Personalización
personalization-summary = Las sugerencias pueden usar datos sobre ti y aprender de tu escritura anterior. TypeAlike guarda la escritura anterior cifrada en este equipo. No guarda el texto de los campos de contraseña.
personalization-name = Tu nombre
personalization-name-description = Las sugerencias pueden usar tu nombre, por ejemplo para firmar un correo.
personalization-notes = Notas sobre ti
personalization-notes-description = Escribe algunos datos breves que ayuden a las sugerencias, como tu trabajo y tus idiomas. Escríbelos en primera persona, por ejemplo: «Escribo correos breves».
personalization-notes-example = Empezar con un ejemplo
personalization-notes-example-description = Elige el ejemplo más parecido a tu forma de escribir. Después cambia las notas de abajo.
personalization-history = Usar la escritura anterior
personalization-history-description = TypeAlike guarda el texto que escribes en un archivo cifrado en este equipo. Las sugerencias usan entonces tus palabras y tu estilo. Si desactivas esto, el texto guardado se queda hasta que lo eliminas al final de esta página.
personalization-kept-texts = Texto que se guarda
personalization-kept-texts-description = TypeAlike guarda tu texto cuando lo terminas, por ejemplo cuando envías un mensaje.
personalization-kept-texts-accepted = Solo el texto donde usaste una sugerencia
personalization-kept-texts-all = Todo el texto
personalization-reply-words = Aprender de los mensajes que respondes
personalization-reply-words-description = En un chat o un correo, TypeAlike también guarda algunas palabras del mensaje que respondes. Las sugerencias usan entonces tus respuestas anteriores a mensajes parecidos. Si desactivas esto, TypeAlike elimina estas palabras.
personalization-word-choice = Elección de palabras
personalization-word-choice-description = Las sugerencias usan más las palabras que escribes a menudo. Con un nivel más alto aparecen más de tus palabras, pero las sugerencias son incorrectas más a menudo.
personalization-word-choice-off = Desactivada
personalization-word-choice-light = Leve
personalization-word-choice-medium = Media
personalization-word-choice-strong = Fuerte
personalization-word-choice-needs-history = Para usar este ajuste, activa «Usar la escritura anterior».
personalization-apps-footer = Si eliges «Desactivada» para una app, TypeAlike elimina el texto que guardó de esa app. Con «Solo esta app», la escritura anterior de una app ayuda solo en esa app. Las apps con «Todas las apps» comparten su escritura anterior. Cuando una sugerencia aparece por primera vez en una app, TypeAlike agrega la app a esta lista.
personalization-delete-description = TypeAlike elimina todo el texto que guardó. No puedes deshacer esta acción.
personalization-delete = Eliminar toda la escritura anterior
personalization-delete-confirm = Presiona otra vez para eliminar

## The Energy saving pane of the settings window.

energy-saving-title = Ahorro de energía
energy-saving-summary = TypeAlike puede detener las sugerencias cuando no escribes durante un tiempo, durante los juegos, en pantalla completa y con el ahorro de energía. Esto libera memoria y ahorra energía.
energy-saving-idle-timeout = Detener tras un tiempo sin uso
energy-saving-idle-timeout-description = Si no escribes ni haces clic durante este tiempo, las sugerencias se detienen para liberar memoria. Vuelven cuando escribes en un campo de texto.
energy-saving-idle-timeout-off = Nunca
energy-saving-idle-timeout-15 = 15 minutos
energy-saving-idle-timeout-30 = 30 minutos
energy-saving-idle-timeout-60 = 60 minutos
energy-saving-energy-saver = Detener con ahorro de energía
energy-saving-energy-saver-description = Mientras el ahorro de energía o el modo de bajo consumo está activado, las sugerencias se detienen para ahorrar energía. Vuelven cuando se desactiva.
energy-saving-energy-saver-stopped = Las sugerencias están desactivadas mientras el equipo ahorra energía.
energy-saving-game = Detener durante juegos
energy-saving-game-description = Mientras juegas, las sugerencias se detienen para dejar memoria libre al juego. Vuelven cuando escribes en un campo de texto.
energy-saving-full-screen = Detener en pantalla completa
energy-saving-full-screen-description = Mientras una app ocupa toda la pantalla, como un video o un juego, las sugerencias se detienen para liberar memoria. Vuelven cuando escribes en un campo de texto.
energy-saving-preparation = Preparar las sugerencias antes
energy-saving-preparation-description = Cuando entras en un campo de texto, el modelo lee su texto de inmediato. La primera sugerencia aparece antes, pero esto usa más energía.

## The Permissions pane of the settings window. It shows only on an OS that asks the user for permissions (macOS). The labels are the names of the permissions in System Settings.

permissions-title = Permisos
permissions-summary = TypeAlike usa estos permisos para mostrar sugerencias en otras apps. Los das en Ajustes del Sistema.
permissions-accessibility = Accesibilidad
permissions-accessibility-description = TypeAlike necesita este permiso para leer el texto donde escribes y para escribir una sugerencia cuando pulsas Tab. Sin él, no aparece ninguna sugerencia.
permissions-screen-recording = Grabación de pantalla y del audio del sistema
permissions-screen-recording-description = Algunas apps no dan su texto a TypeAlike. Con este permiso, TypeAlike lee su texto de una imagen de la ventana. Las sugerencias también funcionan sin él.
permissions-granted = Permitido
permissions-give-access = Dar acceso
permissions-footer = Cuando Ajustes del Sistema te pida que cierres y vuelvas a abrir TypeAlike, hazlo para que TypeAlike pueda usar el nuevo permiso.

## The About pane of the settings window: the version of TypeAlike, the check for a newer version, its install and the licenses of the third-party software. $version is a version number, such as 0.4.0.

about-title = Acerca de
about-summary = La versión de TypeAlike y las licencias del software que contiene.
about-version = TypeAlike { $version }
about-check-for-updates = Buscar actualizaciones
about-checking = Buscando actualizaciones…
about-up-to-date = Esta es la versión más reciente.
about-update-available = La versión { $version } está disponible.
about-download = Descargar
about-check-failed = TypeAlike no puede buscar actualizaciones. Revisa tu conexión a internet.
about-try-again = Reintentar
about-keep-up-to-date = Mantener TypeAlike actualizado
about-keep-up-to-date-description = TypeAlike busca una nueva versión una vez al día. La instala y se reinicia.
about-downloading = Descargando la versión { $version }…
about-install-waiting = La versión { $version } se instalará cuando termine la descarga del modelo.
about-installing = Instalando la versión { $version }…
about-install-failed = TypeAlike no puede instalar la versión { $version }.
about-source-code = Ver el código fuente
about-footer = TypeAlike es software libre bajo la Licencia Pública General de GNU, versión 3 o posterior.
about-libraries-heading = Software de terceros
about-libraries-footer = TypeAlike contiene este software de código abierto. Cada biblioteca tiene su propia licencia.
about-show-license = Ver licencia
about-back-to-libraries = Volver a la lista

## The examples of the notes about the writer, in the menu “Start from an example” of the Personalization pane and of the setup. The text of each example goes into the notes, in the first person, and the model reads it.

notes-example-own = Tus propias notas
notes-example-general = Escritura cotidiana
notes-example-general-text = Escribo mensajes claros y amables. Mantengo el tono de la conversación.
notes-example-work = Correos de trabajo
notes-example-work-text = Escribo correos y mensajes de trabajo. Uso un tono amable y profesional, y mis mensajes son breves y claros.
notes-example-support = Atención al cliente
notes-example-support-text = Respondo las preguntas de los clientes. Soy amable y paciente, y doy pasos claros que resuelven el problema.
notes-example-chats = Chats con familia y amigos
notes-example-chats-text = Chateo con mi familia y mis amigos. Escribo mensajes breves e informales.
notes-example-study = Estudios
notes-example-study-text = Estudio. Escribo ensayos, apuntes y correos a mis profesores con un estilo claro y correcto.
notes-example-software = Desarrollo de software
notes-example-software-text = Trabajo en desarrollo de software. Escribo revisiones de código, informes de errores y documentos técnicos. Uso los nombres exactos del código y de las herramientas.

## The setup, which shows when TypeAlike starts for the first time and after a reset of the settings. $number is the number of the step, and $count is the number of steps. The demo shows the typed text, then the suggestion after it in gray, then the key that types its next word.

setup-window-title = Configuración de TypeAlike
setup-progress = Paso { $number } de { $count }
setup-back = Atrás
setup-continue = Continuar
setup-start = Empezar
setup-finish = Terminar
setup-welcome-title = Te damos la bienvenida a TypeAlike
setup-welcome-body = TypeAlike sugiere las siguientes palabras mientras escribes, en cualquier app. Presiona Tab para escribir la siguiente palabra, o sigue escribiendo para ignorar la sugerencia.
setup-demo-typed = Gracias por la respuesta. Te envío el
setup-demo-suggestion = informe el viernes.
setup-demo-key = Tab
setup-model-title = Descarga el modelo
setup-model-select-title = Elige el modelo
setup-model-footer = TypeAlike necesita este modelo para escribir sugerencias. Lo descargas una sola vez. La configuración continúa cuando termina la descarga.
setup-model-checking = Revisando la tarjeta gráfica…
setup-other-models = Elegir otro modelo
setup-permission-title = Da a TypeAlike acceso a los campos de texto
setup-permission-body = Haz clic en «Dar acceso». Después activa TypeAlike en Ajustes del Sistema y vuelve a esta ventana.
setup-languages-title = Elige tus idiomas
setup-spelling = Revisar erratas y ortografía
setup-spelling-description = Una palabra con una errata recibe una corrección, y una sugerencia se detiene antes de una palabra que no está en ningún diccionario.
setup-about-title = Cuéntale a TypeAlike sobre ti
setup-sources-title = Qué pueden usar las sugerencias
setup-history-description = TypeAlike guarda lo que escribes en un archivo cifrado en este equipo. Así las sugerencias usan tus palabras y tu estilo. Puedes eliminar este texto en la página Personalización.
setup-screen-text-description = TypeAlike lee el texto cerca del campo donde escribes, como los últimos mensajes de un chat. Junto con tu escritura anterior, también guarda algunas palabras de los mensajes que respondes.
setup-sources-footer = TypeAlike usa internet solo para buscar y descargar modelos, para descargar diccionarios y para obtener sus nuevas versiones. No guarda el texto de los campos de contraseña.
setup-ready-title = TypeAlike está listo
setup-ready-body = Pruébalo ahora en cualquier app, como tu correo o un chat.
setup-tray-notification-area = TypeAlike está en el área de notificaciones de la barra de tareas. Usa su icono para pausar las sugerencias o para abrir los ajustes.
setup-tray-menu-bar = TypeAlike está en la barra de menús. Usa su icono para pausar las sugerencias o para abrir los ajustes.
