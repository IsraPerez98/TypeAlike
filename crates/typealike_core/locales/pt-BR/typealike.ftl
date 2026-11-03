## The tray menu on Windows, the menu bar menu on macOS and the first entry of the badge menu.

menu-pause = Pausar sugestões
menu-resume = Retomar sugestões
menu-settings = Ajustes…
menu-quit = Sair do TypeAlike

## The name of TypeAlike and its status, in the tooltip of the tray icon and for screen readers at the badge.

status-active = TypeAlike
status-paused = TypeAlike (pausado)
status-off-here = TypeAlike (desativado aqui)
status-energy-saver = TypeAlike (desativado para economizar energia)
status-game = TypeAlike (desativado durante um jogo)
status-full-screen = TypeAlike (desativado em tela cheia)

## The menu of the badge at the caret. $app is the name of an app, and $host is the host of a website, such as mail.example.com.

badge-app-suggestions = Mostrar sugestões em { $app }
badge-site-suggestions = Mostrar sugestões em { $host }
badge-app-screen-text = Usar o texto da tela em { $app }
badge-app-earlier-writing = Textos anteriores em { $app }

## The choices of the earlier writing of an app, in the Personalization pane and in the badge menu.

history-scope-off = Desativado
history-scope-this-app = Apenas este app
history-scope-all-apps = Todos os apps

## The list of apps in the Apps and websites, Copied and screen text and Personalization panes.

app-list-heading = Apps
app-list-empty = Ainda não apareceu nenhuma sugestão em um app.
app-list-footer = Quando uma sugestão aparece em um app pela primeira vez, o TypeAlike adiciona o app a esta lista.

## The General pane of the settings window.

general-title = Geral
general-summary = Defina quando o TypeAlike abre e como as sugestões aparecem.
general-autostart = Abrir no início da sessão
general-autostart-description = O TypeAlike abre sempre que você inicia a sessão, sem que você precise abri-lo.
general-language = Idioma
general-language-description = As janelas e os menus do TypeAlike aparecem neste idioma. As sugestões continuam no idioma do seu texto.
# The first entry of the language menu. The other entries are the names of the languages, which stay the same in each language.
general-language-system = Idioma do sistema
general-display = Mostrar palavra por palavra
general-display-description = Cada palavra de uma sugestão aparece assim que o modelo a escreve. A primeira palavra aparece mais rápido, mas a sugestão cresce enquanto você a lê.
general-typo-rule = Ocultar sugestões em erros de digitação
general-typo-rule-description = Quando uma palavra parece ter um erro de digitação, nenhuma sugestão a completa, para que a tecla Tab não insira o erro. Um nome raro também pode parecer um erro e, nesse caso, fica sem sugestão.
general-typo-fix = Corrigir erros de digitação
general-typo-fix-description = Quando uma palavra parece ter um erro de digitação, a sugestão mostra a palavra riscada e, depois, a palavra corrigida. A tecla Tab insere a palavra corrigida.
general-length = Tamanho das sugestões
general-length-description = Uma sugestão termina neste tamanho. Uma sugestão mais curta aparece mais rápido.
general-length-very-short = Muito curto
general-length-short = Curto
general-length-medium = Médio
general-length-long = Longo
general-length-very-long = Muito longo
general-length-processor-note = Quando o modelo é executado no processador, as sugestões são curtas ou muito curtas, para que apareçam mais rápido.
general-sentence-end = Parar no fim de uma frase
general-sentence-end-description = Uma sugestão termina depois da primeira frase que ela completa.
general-sentence-end-off-note = As sugestões costumam errar depois da primeira frase. Ative esta opção para que elas parem ali.
general-reset = Redefinir todos os ajustes
general-reset-description = Todos os ajustes voltam ao padrão, e a configuração inicial abre de novo. Seus modelos baixados e seus textos anteriores são mantidos.
general-reset-confirm = Pressione de novo para redefinir

## The menu of the suggestion model and its status, in the General, Models and Playground panes.

model-menu = Modelo de sugestões
model-menu-select = Selecione um modelo
model-menu-none = Nenhum modelo baixado
model-needed = O TypeAlike precisa de um modelo para escrever sugestões. Baixe o modelo recomendado abaixo.
model-recommended = Recomendado para este computador
model-recommendation-unknown-gpu = O TypeAlike não consegue ler a memória da placa de vídeo, então recomenda um modelo que cabe na maioria das placas de vídeo.
model-starting = Iniciando o modelo…
model-paused = As sugestões estão pausadas. Para retomá-las, selecione “Retomar sugestões” no menu do TypeAlike.
model-energy-saver = As sugestões ficam desativadas enquanto o computador economiza energia. Para usá-las agora, desative “Parar na economia de energia”.
model-game = As sugestões pararam para liberar memória para um jogo. Elas voltam quando você digitar em um campo de texto.
model-full-screen = As sugestões pararam para liberar memória para um app em tela cheia. Elas voltam quando você digitar em um campo de texto.
model-idle = As sugestões pararam para liberar memória, porque você não digitou nem clicou por algum tempo. Elas voltam quando você digitar em um campo de texto.
model-error-missing = O TypeAlike não encontra o arquivo do modelo. Baixe o modelo de novo.
model-error-invalid = O TypeAlike não consegue usar este modelo. O arquivo pode estar danificado ou ser de um tipo que o TypeAlike não aceita. Baixe-o de novo ou selecione outro modelo.
model-error-too-large = O modelo não cabe na memória livre da placa de vídeo. Selecione um modelo menor.
model-error-hardware = O TypeAlike não consegue executar o modelo nesta placa de vídeo. Em “Executar o modelo em”, selecione outra opção.
model-error-no-model = Selecione um modelo primeiro.
model-error-text-too-long = O texto é longo demais para este modelo. Use um texto mais curto.
model-error-failed = O TypeAlike não conseguiu escrever uma sugestão. Tente de novo.
model-error-superseded = O TypeAlike parou esta sugestão para escrever uma mais recente.

## The Playground pane of the settings window. $seconds is the time that the model took to write the suggestion, with two digits after the decimal separator, such as 0.42.

playground-title = Área de testes
playground-summary = Selecione um modelo e digite o começo de uma frase para ver o que o modelo sugere.
playground-text = Texto
playground-text-placeholder = Digite o começo de uma frase
playground-suggest = Mostrar sugestão
playground-output = Sugestão
playground-writing = Escrevendo uma sugestão…
playground-nothing-added = O modelo não tem sugestão para este texto. Digite mais palavras e tente de novo.
playground-time = Escrita em { $seconds } segundos

## The Models pane of the settings window. $developer is the company that made a model, such as Google. $owner is the account on Hugging Face that shares a model. $query is the text that the user searched for.

models-title = Modelos
models-summary = Um modelo neste computador escreve as sugestões. O texto que você digita em outros apps nunca sai deste computador. A pesquisa e os downloads de modelos usam a internet.
models-model-description = Um modelo menor é mais rápido e precisa de menos memória, mas suas sugestões erram com mais frequência.
models-hardware = Executar o modelo em
models-hardware-description = Uma placa de vídeo executa o modelo mais rápido que o processador, mas o modelo inteiro precisa caber na memória dela.
models-hardware-processor = Processador (CPU)
models-hardware-graphics-card = Placa de vídeo (Vulkan)
models-hardware-nvidia = Placa de vídeo NVIDIA (CUDA)
models-search = Pesquisar no Hugging Face
models-search-placeholder = Nome de um modelo
models-search-button = Pesquisar
models-recommended-heading = Modelos recomendados
models-recommended-footer = Se estiver em dúvida, baixe o modelo que o TypeAlike recomenda para este computador. Você pode excluir um modelo a qualquer momento.
models-developer = Por { $developer }
models-developer-recommended = Por { $developer } · Recomendado para este computador
models-searching = Pesquisando no Hugging Face…
models-results-heading = Resultados para “{ $query }”
models-results-footer = A lista mostra só os modelos que o TypeAlike pode usar. Os modelos com mais downloads aparecem primeiro.
models-no-results = Nenhum modelo do Hugging Face corresponde a “{ $query }”. Tente outras palavras.
models-owner = Por { $owner }
models-owner-gated = Por { $owner } · O TypeAlike não pode baixar este modelo
models-show-files = Mostrar arquivos
models-back-to-results = Voltar aos resultados
models-files-loading = Obtendo a lista de arquivos…
models-files-footer = Cada arquivo é o mesmo modelo em outro tamanho. Um arquivo menor é mais rápido, mas suas sugestões erram com mais frequência. Se estiver em dúvida, selecione um arquivo Q4_K_M.
models-no-files = Este modelo não tem arquivos que o TypeAlike possa usar.
models-downloaded-heading = Modelos baixados
models-download = Baixar
models-cancel-download = Cancelar download
models-canceling = Cancelando o download…
models-delete = Excluir
models-try-again = Tentar de novo
models-error-offline = O TypeAlike não consegue se conectar ao Hugging Face. Verifique sua conexão com a internet.
models-error-account = O TypeAlike não pode baixar este modelo, porque ele exige uma conta do Hugging Face. Selecione outro modelo.
models-error-not-found = O Hugging Face não encontra este modelo. Pesquise o modelo de novo.
models-error-server = O Hugging Face não está disponível. Tente de novo mais tarde.
models-error-response = O Hugging Face enviou uma resposta inválida. Tente de novo mais tarde.
models-error-storage = O TypeAlike não consegue salvar o modelo. Verifique se há espaço livre no disco.
models-error-canceled = O download parou antes de terminar.

## The numbers of the Models pane. $count is a count of downloads: a whole number below one thousand, else a number of thousands, millions or billions, with one digit after the decimal separator when it is not zero, such as 1.2 or 15. $percent is a whole number from 0 to 100. $size is a file size: a whole number of bytes, kilobytes or megabytes, or a number of gigabytes with one digit after the decimal separator, such as 2.5.

models-downloads =
    { $count ->
        [0] { $count } downloads
        [one] { $count } download
       *[other] { $count } downloads
    }
models-downloads-thousands = { $count } mil downloads
models-downloads-millions =
    { $count ->
        [one] { $count } milhão de downloads
       *[other] { $count } milhões de downloads
    }
models-downloads-billions =
    { $count ->
        [one] { $count } bilhão de downloads
       *[other] { $count } bilhões de downloads
    }
models-download-progress = { $percent }% baixado
models-size-bytes =
    { $size ->
        [0] { $size } bytes
        [one] { $size } byte
       *[other] { $size } bytes
    }
models-size-kilobytes = { $size } KB
models-size-megabytes = { $size } MB
models-size-gigabytes = { $size } GB

## The Apps and websites pane of the settings window.

suggestion-places-title = Apps e sites
suggestion-places-summary = As sugestões aparecem em cada app e em cada site onde você digita. Para parar as sugestões em um app ou em um site, desative a opção dele.
suggestion-places-websites = Sites
suggestion-places-no-websites = Ainda não apareceu nenhuma sugestão em um site.
suggestion-places-websites-footer = Quando uma sugestão aparece em um site pela primeira vez, o TypeAlike adiciona o site a esta lista.

## The Copied and screen text pane of the settings window.

screen-text-title = Texto copiado e da tela
screen-text-summary = As sugestões podem usar o texto que você copia e o texto perto do campo onde você digita, como as últimas mensagens de um chat. O TypeAlike lê esse texto neste computador. Ele salva apenas algumas palavras das mensagens que você responde, junto com seus textos anteriores.
screen-text-clipboard = Usar o texto copiado
screen-text-clipboard-description = As sugestões podem usar o texto que você copia. O TypeAlike o lê só enquanto você usa um campo de texto e não o salva.
screen-text-switch = Usar o texto da tela
screen-text-switch-description = O TypeAlike lê o texto da tela em cada app com a opção ativada na lista abaixo.
screen-text-recognition = Reconhecimento de texto
screen-text-recognition-description = Quando o TypeAlike não consegue obter o texto de uma janela, ele lê o texto em uma imagem da janela. “Rápido” lê alemão, espanhol, francês, inglês, italiano e português. “Preciso” lê mais idiomas com menos erros, mas é mais lento.
screen-text-recognition-fast = Rápido (recomendado)
screen-text-recognition-accurate = Preciso
screen-text-permission = Alguns apps não dão o texto deles ao TypeAlike. Para ler esse texto em uma imagem da janela, o TypeAlike precisa da permissão Gravação do Áudio do Sistema e da Tela.
screen-text-open-system-settings = Abrir Ajustes do Sistema

## The Personalization pane of the settings window.

personalization-title = Personalização
personalization-summary = As sugestões podem usar informações sobre você e aprender com seus textos anteriores. O TypeAlike guarda os textos anteriores criptografados neste computador. Ele não salva o texto de campos de senha.
personalization-name = Seu nome
personalization-name-description = As sugestões podem usar seu nome, por exemplo, para assinar um e-mail.
personalization-notes = Notas sobre você
personalization-notes-description = Escreva alguns fatos curtos que ajudem as sugestões, como sua profissão e seus idiomas. Escreva em primeira pessoa, por exemplo: “Escrevo e-mails curtos.”
personalization-notes-example = Começar com um exemplo
personalization-notes-example-description = Selecione o exemplo mais parecido com seu jeito de escrever. Depois, altere as notas abaixo.
personalization-history = Usar textos anteriores
personalization-history-description = O TypeAlike salva o texto que você escreve em um arquivo criptografado neste computador. As sugestões passam a usar suas palavras e seu estilo. Se você desativar esta opção, o texto salvo fica guardado até você excluí-lo no fim desta página.
personalization-kept-texts = Texto a salvar
personalization-kept-texts-description = O TypeAlike salva seu texto quando você o termina, por exemplo, quando envia uma mensagem.
personalization-kept-texts-accepted = Só o texto em que você usou uma sugestão
personalization-kept-texts-all = Todo o texto
personalization-reply-words = Aprender com as mensagens que você responde
personalization-reply-words-description = Em um chat ou e-mail, o TypeAlike também salva algumas palavras da mensagem que você responde. As sugestões passam a usar suas respostas anteriores a mensagens parecidas. Se você desativar esta opção, o TypeAlike exclui essas palavras.
personalization-word-choice = Escolha de palavras
personalization-word-choice-description = As sugestões usam mais as palavras que você escreve com frequência. Um nível mais forte usa mais palavras suas, mas as sugestões erram com mais frequência.
personalization-word-choice-off = Desativada
personalization-word-choice-light = Leve
personalization-word-choice-medium = Média
personalization-word-choice-strong = Forte
personalization-word-choice-needs-history = Para usar este ajuste, ative “Usar textos anteriores”.
personalization-apps-footer = Quando você define um app como “Desativado”, o TypeAlike exclui o texto que salvou desse app. Com “Apenas este app”, os textos anteriores de um app ajudam só nesse app. Os apps com “Todos os apps” compartilham os textos anteriores. Quando uma sugestão aparece em um app pela primeira vez, o TypeAlike adiciona o app a esta lista.
personalization-delete-description = O TypeAlike exclui todo o texto que salvou. Não é possível desfazer esta ação.
personalization-delete = Excluir todos os textos anteriores
personalization-delete-confirm = Pressione de novo para excluir

## The Energy saving pane of the settings window.

energy-saving-title = Economia de energia
energy-saving-summary = O TypeAlike pode parar as sugestões quando você não digita por algum tempo, durante jogos, em tela cheia e na economia de energia. Isso libera memória e economiza energia.
energy-saving-idle-timeout = Parar após um tempo sem uso
energy-saving-idle-timeout-description = Se você não digitar nem clicar durante esse tempo, as sugestões param para liberar memória. Elas voltam quando você digitar em um campo de texto.
energy-saving-idle-timeout-off = Nunca
energy-saving-idle-timeout-15 = 15 minutos
energy-saving-idle-timeout-30 = 30 minutos
energy-saving-idle-timeout-60 = 60 minutos
energy-saving-energy-saver = Parar na economia de energia
energy-saving-energy-saver-description = Enquanto a economia de energia ou o Modo Pouca Energia estiver ativado, as sugestões param para economizar energia. Elas voltam quando ele é desativado.
energy-saving-energy-saver-stopped = As sugestões ficam desativadas enquanto o computador economiza energia.
energy-saving-game = Parar durante jogos
energy-saving-game-description = Enquanto você joga, as sugestões param para liberar memória para o jogo. Elas voltam quando você digitar em um campo de texto.
energy-saving-full-screen = Parar em tela cheia
energy-saving-full-screen-description = Enquanto um app ocupa a tela inteira, como um vídeo ou um jogo, as sugestões param para liberar memória. Elas voltam quando você digitar em um campo de texto.
energy-saving-preparation = Preparar sugestões com antecedência
energy-saving-preparation-description = Quando você entra em um campo de texto, o modelo lê o texto dele na hora. A primeira sugestão aparece mais rápido, mas isso usa mais energia.

## The Permissions pane of the settings window. It shows only on an OS that asks the user for permissions (macOS). The labels are the names of the permissions in System Settings.

permissions-title = Permissões
permissions-summary = O TypeAlike usa estas permissões para mostrar sugestões em outros apps. Você as concede nos Ajustes do Sistema.
permissions-accessibility = Acessibilidade
permissions-accessibility-description = O TypeAlike precisa desta permissão para ler o texto onde você digita e para inserir uma sugestão quando você pressiona Tab. Sem ela, nenhuma sugestão aparece.
permissions-screen-recording = Gravação do Áudio do Sistema e da Tela
permissions-screen-recording-description = Alguns apps não dão o texto deles ao TypeAlike. Com esta permissão, o TypeAlike lê esse texto em uma imagem da janela. As sugestões também funcionam sem ela.
permissions-granted = Permitido
permissions-give-access = Dar acesso
permissions-footer = Quando os Ajustes do Sistema pedirem para você sair do TypeAlike e abri-lo de novo, faça isso para que o TypeAlike possa usar a nova permissão.

## The About pane of the settings window: the version of TypeAlike, the check for a newer version and the licenses of the third-party software. $version is a version number, such as 0.4.0.

about-title = Sobre
about-summary = A versão do TypeAlike e as licenças do software que ele contém.
about-version = TypeAlike { $version }
about-check-for-updates = Verificar atualizações
about-checking = Verificando atualizações…
about-up-to-date = Esta é a versão mais recente.
about-update-available = A versão { $version } está disponível.
about-download = Baixar
about-check-failed = O TypeAlike não consegue verificar atualizações. Verifique sua conexão com a internet.
about-try-again = Tentar de novo
about-source-code = Ver o código-fonte
about-footer = O TypeAlike procura uma nova versão só quando você pede. O TypeAlike é software livre sob a Licença Pública Geral GNU, versão 3 ou posterior.
about-libraries-heading = Software de terceiros
about-libraries-footer = O TypeAlike contém este software de código aberto. Cada biblioteca tem a própria licença.
about-show-license = Ver licença
about-back-to-libraries = Voltar à lista

## The examples of the notes about the writer, in the menu “Start from an example” of the Personalization pane and of the setup. The text of each example goes into the notes, in the first person, and the model reads it.

notes-example-own = Suas próprias notas
notes-example-general = Escrita do dia a dia
notes-example-general-text = Escrevo mensagens claras e cordiais. Mantenho o tom da conversa.
notes-example-work = E-mails de trabalho
notes-example-work-text = Escrevo e-mails e mensagens no trabalho. Uso um tom educado e profissional, e minhas mensagens são curtas e claras.
notes-example-support = Atendimento ao cliente
notes-example-support-text = Respondo perguntas de clientes. Sou gentil e paciente, e explico com clareza os passos para resolver o problema.
notes-example-chats = Conversas com amigos e família
notes-example-chats-text = Converso com amigos e família. Escrevo mensagens curtas e informais.
notes-example-study = Escola e estudos
notes-example-study-text = Sou estudante. Escrevo redações, anotações e e-mails para meus professores com um estilo claro e correto.
notes-example-software = Desenvolvimento de software
notes-example-software-text = Trabalho com desenvolvimento de software. Escrevo revisões de código, relatórios de bugs e documentos técnicos. Uso os nomes exatos do código e das ferramentas.

## The setup, which shows when TypeAlike starts for the first time and after a reset of the settings. $number is the number of the step, and $count is the number of steps. The demo shows the typed text, then the suggestion after it in gray, then the key that types its next word.

setup-window-title = Configuração do TypeAlike
setup-progress = Etapa { $number } de { $count }
setup-back = Voltar
setup-continue = Continuar
setup-start = Começar
setup-finish = Concluir
setup-welcome-title = Boas-vindas ao TypeAlike
setup-welcome-body = O TypeAlike sugere as próximas palavras enquanto você digita, em qualquer app. Pressione Tab para inserir a próxima palavra ou continue digitando para ignorar a sugestão.
setup-demo-typed = Agradeço a resposta rápida. Vou enviar o
setup-demo-suggestion = relatório até sexta.
setup-demo-key = Tab
setup-model-title = Baixe o modelo
setup-model-select-title = Selecione o modelo
setup-model-footer = O TypeAlike precisa deste modelo para escrever sugestões. Você baixa o modelo só uma vez. A configuração continua quando o download terminar.
setup-model-checking = Verificando a placa de vídeo…
setup-other-models = Escolher outro modelo
setup-permission-title = Dê ao TypeAlike acesso aos campos de texto
setup-permission-body = Clique em “Dar acesso”. Depois, ative o TypeAlike nos Ajustes do Sistema e volte a esta janela.
setup-about-title = Conte ao TypeAlike sobre você
setup-sources-title = O que as sugestões podem usar
setup-history-description = O TypeAlike salva o que você escreve em um arquivo criptografado neste computador. Assim, as sugestões usam suas palavras e seu estilo. Você pode excluir esse texto na página Personalização.
setup-screen-text-description = O TypeAlike lê o texto perto do campo onde você digita, como as últimas mensagens de um chat. Junto com seus textos anteriores, ele também salva algumas palavras das mensagens que você responde.
setup-sources-footer = O TypeAlike usa a internet só para encontrar e baixar modelos, e para verificar atualizações quando você pede. Ele não salva o texto de campos de senha.
setup-ready-title = O TypeAlike está pronto
setup-ready-body = Experimente agora em qualquer app, como seu e-mail ou um chat.
setup-tray-notification-area = O TypeAlike fica na área de notificação da barra de tarefas. Use o ícone dele para pausar as sugestões ou abrir os ajustes.
setup-tray-menu-bar = O TypeAlike fica na barra de menus. Use o ícone dele para pausar as sugestões ou abrir os ajustes.
