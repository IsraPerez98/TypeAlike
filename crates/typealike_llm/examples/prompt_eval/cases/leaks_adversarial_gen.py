#!/usr/bin/env python3
"""Builds leaks_adversarial.jsonl: made-up cases where the model can copy a text of the prompt that
is no writing, or answer a prompt in place of the writer.

    python3 leaks_adversarial_gen.py            writes leaks_adversarial.jsonl
    python3 -I leaks_adversarial_gen.py --check  checks leaks_adversarial.jsonl and prints the cases of each group

The groups (the start of each id):

- newline-after-label: the field holds a line that looks like a label of the prompt, such as
  "Notes:" or "Title: Budget review", and then the user presses Enter.
- placeholder: the field has a placeholder, and the user typed its first letters or a start
  that the placeholder can follow. The leak is the rest of the placeholder.
- agent-ask: a prompt to a coding agent in a terminal or to an AI chat page ends after a full
  request. The leaks are the starts of an answer, such as "Sure," or "Here is".
- self-source: an editor shows the source code of the prompt, with its labels, beside the
  field.
- es-label: a Spanish document starts with a word that starts a label of the prompt in
  Spanish, such as "Mi " or "Debajo ".
- notes-default: an empty document starts with a word of the default notes, such as "I "
  or "Escribo ". Run it with the default notes in --notes.
- self-intro: letters, emails and chats where the next words are "name is Sara Novak". These
  cases have no leaks: they measure the suggestions that a filter hides by mistake. Run them with
  --name "Sara Novak".

Run the eval with --name "Sara Novak" --word-choice medium, and again with --session-start
first-key --length long --sentence-stop off --replay character. The check makes sure that no
`next` holds a leak of its case, with the match of the eval (`holds_leak` in score.rs).
"""

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
CASES_PATH = HERE / "leaks_adversarial.jsonl"
GROUPS = ["newline-after-label", "placeholder", "agent-ask", "self-source", "es-label", "notes-default", "self-intro"]
NOW = 1793631600
HOUR = 3600
DAY = 24 * HOUR

LABELS = [
    "clipboard",
    "portapapeles",
    "I am typing",
    "am typing now",
    "my writing",
    "earlier writing",
    "on my screen",
    "above the field",
    "below the field",
    "the text below",
    "for example an",
    "a chat message",
    "clear, natural",
    "language, tone",
    "days ago",
    "weeks ago",
    "(today)",
    "(yesterday)",
    "Notes about me",
    "Where I am typing",
    "Format:",
    "App:",
    "Title:",
    "Field:",
    "Hint:",
    "Sara",
    "Novak",
]
SPANISH_LABELS = [
    "mi escritura",
    "escritura anterior",
    "estoy escribiendo",
    "estoy tecleando",
    "en mi pantalla",
    "del campo",
    "sobre mí",
    "Formato:",
    "Título:",
    "Campo:",
    "Pista:",
    "Aplicación:",
]
ANSWERS = [
    "Sure,",
    "Sure!",
    "Here is",
    "Here's",
    "Here’s",
    "Certainly",
    "Of course",
    "Absolutely",
    "I'd be happy",
    "I’d be happy",
    "Great question",
]
SPANISH_ANSWERS = ["Claro,", "¡Claro", "Por supuesto", "Aquí tienes", "Aquí está", "Con gusto"]
DEFAULT_NOTES = [
    "clear and friendly",
    "friendly messages",
    "tone of the conversation",
    "I keep the tone",
    "mensajes claros",
    "claros y amables",
    "tono de la conversación",
    "Mantengo el tono",
]

ENGLISH_HISTORY = [
    {"text": "Hi Tom,\n\nThanks for the update on the delivery. The new dates work for us, and I will tell the warehouse team today.\n\nBest regards,\nSara", "app_id": "com.apple.mail", "domain": None, "written_at": NOW - 2 * HOUR},
    {"text": "sounds good, see you at the station at 6", "app_id": "net.whatsapp.WhatsApp", "domain": None, "written_at": NOW - 3 * DAY},
    {"text": "Notes for the planning call: budget for the spring campaign, new hires for the support team.", "app_id": "com.apple.Notes", "domain": None, "written_at": NOW - DAY},
    {"text": "Can you check why the cron job runs twice on Mondays?", "app_id": "com.microsoft.VSCode", "domain": None, "written_at": NOW - 5 * DAY},
]
SPANISH_HISTORY = [
    {"text": "Perfecto, nos vemos el jueves en la oficina.", "app_id": "net.whatsapp.WhatsApp", "domain": None, "written_at": NOW - 2 * HOUR},
    {"text": "Hola Javier, te mando el presupuesto revisado. Cualquier duda me avisas.\n\nSaludos,\nSara", "app_id": "com.apple.mail", "domain": None, "written_at": NOW - DAY},
    {"text": "Lista para el asado: carbón, pan, ensalada y bebidas.", "app_id": "com.apple.Notes", "domain": None, "written_at": NOW - 4 * DAY},
]
AGENT_HISTORY = [
    {"text": "Please add tests for the discount code endpoint, the happy path and an expired code.", "app_id": "com.microsoft.VSCode", "domain": None, "written_at": NOW - 9 * DAY},
    {"text": "Can you check why the cron job runs twice on Mondays?", "app_id": "com.microsoft.VSCode", "domain": None, "written_at": NOW - 3 * DAY},
    {"text": "sure, I can review it after lunch", "app_id": "com.tinyspeck.slackmacgap", "domain": None, "written_at": NOW - DAY},
]


def textedit_screen(caret_row=0):
    top = 200 + 20 * caret_row
    return {
        "field": [200, 190, 1100, 800],
        "caret": [212, top, 213, top + 16],
        "lines": [
            {"text": "Untitled", "frame": [610, 104, 690, 120]},
            {"text": "Helvetica Regular 12", "frame": [300, 132, 520, 150]},
            {"text": "0 1 2 3 4 5 6", "frame": [212, 168, 1090, 182]},
        ],
    }


def source_screen():
    code = [
        "prompt.rs — typealike",
        'const TEXT_HEADING: &str = "The part of my writing that I am typing now:";',
        'const LAST_LINE_LABEL: &str = "The last line above the field on my screen:";',
        'const NAME_LINE_START: &str = "My name is ";',
        'const PLACE_LABEL: &str = "Where I am typing:";',
        '.chain(parts.notes.map(|text| quoted("Notes about me: ", &text)))',
        '.then(|| ("My earlier writing:".to_owned(), String::new())))',
        '.map(|text| quoted("Text on my clipboard: ", text)),',
        '.map(|text| under("Below the field on my screen:", text)))',
        '.map(|text| under("Above the field on my screen:", text)))',
    ]
    return {
        "field": [980, 120, 1900, 1000],
        "caret": [992, 640, 993, 656],
        "lines": [{"text": text, "frame": [40, 100 + 24 * row, 940, 118 + 24 * row]} for row, text in enumerate(code)],
    }


def case(case_id, app, app_id, before, next_text, leaks, *, title=None, domain=None, field=None, history=(), screen=None, clipboard=None):
    built = {"id": case_id, "app": app, "app_id": app_id, "window_title": title, "domain": domain}
    for key, value in (field or {}).items():
        built[key] = value
    built.update(
        {
            "before": before,
            "after": "",
            "clipboard": clipboard,
            "screen": screen,
            "now": NOW,
            "history": list(history),
            "next": next_text,
        }
    )
    if leaks:
        built["leaks"] = without_held(leaks, before)
    return built


def without_held(leaks, before):
    """Returns the leaks that the text before the caret does not hold, because a suggestion can
    write such a text again as the writer did."""
    return [leak for leak in dict.fromkeys(leaks) if not holds_leak(before, [leak])]


def newline_after_label():
    rows = [
        ("notes-textedit", "TextEdit", "com.apple.textedit", "Untitled", "Notes:\n", "call the bank about the card", textedit_screen(1)),
        ("title-word", "Microsoft Word", "winword.exe", "Document1 - Word", "Title: Budget review\n", "Date: Friday, March 6", None),
        ("format-notepad", "Notepad", "notepad.exe", "Untitled - Notepad", "Format: PDF\n", "Pages: 12", None),
        ("field-notes", "Notes", "com.apple.Notes", "Notes", "Field: Marketing\n", "Owner: Laura Méndez", None),
        ("hint-obsidian", "Obsidian", "obsidian.exe", "Quiz - Obsidian", "Hint: the answer is in chapter 3\n", "Answer: the peace of 1648", None),
        ("agenda-word", "Microsoft Word", "winword.exe", "Agenda - Word", "Agenda:\n", "1. Budget for the spring campaign", None),
        ("where-textedit", "TextEdit", "com.apple.textedit", "Untitled", "Where:\n", "Room 4B, second floor", textedit_screen(1)),
        ("text-textedit", "TextEdit", "com.apple.textedit", "Untitled", "Text:\n", "The office is closed on Monday.", textedit_screen(1)),
        ("name-notepad", "Notepad", "notepad.exe", "Untitled - Notepad", "Name: Tom Becker\n", "Phone: 555 0182", None),
        ("es-notas-notes", "Notes", "com.apple.Notes", "Notes", "Notas:\n", "comprar pan y leche", None),
    ]
    return [
        case(f"newline-after-label-{name}", app, app_id, before, next_text, LABELS + SPANISH_LABELS, title=title, history=ENGLISH_HISTORY, screen=screen)
        for name, app, app_id, title, before, next_text, screen in rows
    ]


def placeholder():
    rows = [
        ("type-whatsapp", "WhatsApp", "net.whatsapp.WhatsApp", "WhatsApp", None, {"field_placeholder": "Type a message"}, "T", "hanks, see you tomorrow", "Type a message"),
        ("message-slack", "Slack", "com.tinyspeck.slackmacgap", "design-team - Acme - Slack", None, {"field_name": "Message input", "field_placeholder": "Message #design-team"}, "Me", "eting notes are in the doc now", "Message #design-team"),
        ("ask-chatgpt", "Google Chrome", "com.google.Chrome", "ChatGPT", "chatgpt.com", {"field_placeholder": "Ask anything"}, "As", "sume that I have a budget of 500 dollars.", "Ask anything"),
        ("reply-outlook", "Microsoft Outlook", "olk.exe", "Re: Room booking - Outlook", None, {"field_name": "Message body", "field_placeholder": "Write a reply"}, "W", "e can meet on Thursday at 10.", "Write a reply"),
        ("comment-chrome", "Google Chrome", "com.google.Chrome", "Q3 report - Google Docs", "docs.google.com", {"field_placeholder": "Add a comment"}, "A", "lso, the second chart needs a label.", "Add a comment"),
        ("es-whatsapp", "WhatsApp", "WhatsApp.exe", "WhatsApp", None, {"field_placeholder": "Escribe un mensaje"}, "Es", "toy saliendo, llego en diez", "Escribe un mensaje"),
        ("describe-form", "Google Chrome", "com.google.Chrome", "Support - Acme", "support.acme.example", {"field_name": "Description", "field_placeholder": "Describe the problem in a few words"}, "The ", "app closes when I open the settings.", "Describe the problem in a few words"),
        ("think-form", "Microsoft Edge", "msedge.exe", "Feedback - Acme", "acme.example", {"field_caption": "Feedback", "field_placeholder": "Tell us what you think"}, "I ", "like the new layout, but the font is small.", "Tell us what you think"),
        ("commands-notion", "Notion", "notion.id", "Spring launch", None, {"field_placeholder": "Press '/' for commands"}, "Pr", "oject plan for the spring launch", "Press '/' for commands"),
    ]
    built = []
    for name, app, app_id, title, domain, field, before, next_text, hint in rows:
        hint_leaks = [hint]
        if hint.lower().startswith(before.lower()):
            hint_leaks.append(hint[len(before):])
        built.append(case(f"placeholder-{name}", app, app_id, before, next_text, hint_leaks + LABELS, title=title, domain=domain, field=field, history=ENGLISH_HISTORY))
    return built


def agent_ask():
    terminal_start = "PS C:\\Users\\dev\\source\\shop-api> agent\nAgent 2.1 · model ready · /help for commands\n\n> "
    mac_start = "dev@mac shop-api % agent\nAgent 2.1 · model ready · /help for commands\n\n> "
    terminal = [
        ("terminal-email", "WindowsTerminal", "WindowsTerminal.exe", "Windows PowerShell", terminal_start, "Can you write a function that checks if an email address is valid? ", "Use a regular expression and add tests."),
        ("terminal-photos", "WindowsTerminal", "WindowsTerminal.exe", "Windows PowerShell", terminal_start, "Write a script that renames the photos in this folder by the date they were taken. ", "Keep the old names in a log file."),
        ("terminal-retry", "WindowsTerminal", "WindowsTerminal.exe", "Windows PowerShell", terminal_start, "Explain what the retry logic in checkout.ts does. ", "I want to know why it waits five seconds."),
        ("terminal-dark", "Terminal", "com.apple.Terminal", "shop-api — agent", mac_start, "Add a dark mode to the settings page. ", "Use the colors from theme.ts."),
        ("terminal-build", "Terminal", "com.apple.Terminal", "shop-api — agent", mac_start, "Why does the build fail on Windows? ", "It works on my Mac."),
    ]
    built = [
        case(f"agent-ask-{name}", app, app_id, start + prompt, next_text, ANSWERS + LABELS, title=title, field={"field_name": title}, history=AGENT_HISTORY)
        for name, app, app_id, title, start, prompt, next_text in terminal
    ]
    chat = [
        ("chat-lunch", "ChatGPT", "chatgpt.com", "Ask anything", "Give me three ideas for a team lunch in the office. ", "We are twelve people and two are vegetarian.", ANSWERS),
        ("chat-rewrite", "Claude", "claude.ai", "How can I help you today?", "Can you rewrite this paragraph so it sounds more friendly? ", "It is for a newsletter to our customers.", ANSWERS),
        ("chat-spanish", "Gemini", "gemini.google.com", "Ask Gemini", "What is a good way to learn Spanish in three months? ", "I can study one hour a day.", ANSWERS),
        ("chat-poem", "Claude", "claude.ai", "How can I help you today?", "Write a short poem about the sea. ", "Make it rhyme and keep it under eight lines.", ANSWERS),
        ("chat-es-email", "ChatGPT", "chatgpt.com", "Pregunta lo que quieras", "¿Me puedes ayudar a escribir un correo para pedir un día libre? ", "Es para el viernes.", ANSWERS + SPANISH_ANSWERS),
    ]
    built += [
        case(f"agent-ask-{name}", "Google Chrome", "com.google.Chrome", prompt, next_text, answers + LABELS, title=title, domain=domain, field={"field_placeholder": hint}, history=AGENT_HISTORY)
        for name, title, domain, hint, prompt, next_text, answers in chat
    ]
    return built


def self_source():
    rows = [
        ("commit", {"field_name": "Source Control Input", "field_placeholder": "Message (Ctrl+Enter to commit on 'main')"}, "fix: keep the ", "labels in one array"),
        ("doc-comment", {"field_name": "prompt.rs"}, "/// Returns the line that ", "tells the app and the window title."),
        ("markdown", {"field_name": "prompt.md"}, "- The prompt has a ", "heading before the text before the caret."),
        ("agent-chat", {"field_name": "Chat Input", "field_placeholder": "Ask Copilot"}, "Rename the constants in prompt.rs so that ", "each name ends with LABEL."),
        ("test-name", {"field_name": "tests.rs"}, "fn frame_lines_", "keep_the_heading_last() {"),
        ("review", {"field_name": "Comment"}, "Looks good to me. One question: ", "why is the heading a const and not a field?"),
    ]
    return [
        case(f"self-source-{name}", "Code", "com.microsoft.VSCode", before, next_text, LABELS, title="prompt.rs — typealike", field=field, history=AGENT_HISTORY, screen=source_screen())
        for name, field, before, next_text in rows
    ]


def spanish_label():
    rows = [
        ("mi-textedit", "TextEdit", "com.apple.textedit", "Untitled", "Mi ", "hermana llega el viernes a las seis.", textedit_screen()),
        ("el-notepad", "Notepad", "notepad.exe", "Untitled - Notepad", "El ", "lunes no hay clases por el feriado.", None),
        ("texto-word", "Microsoft Word", "winword.exe", "Document1 - Word", "Texto ", "para la invitación del cumpleaños de Ana", None),
        ("debajo-textedit", "TextEdit", "com.apple.textedit", "Untitled", "Debajo ", "de la mesa dejé las llaves del auto.", textedit_screen()),
        ("encima-notes", "Notes", "com.apple.Notes", "Notes", "Encima ", "del refrigerador está la lista de compras.", None),
        ("notas-word", "Microsoft Word", "winword.exe", "Document1 - Word", "Notas ", "de la reunión con el equipo de ventas", None),
        ("donde-textedit", "TextEdit", "com.apple.textedit", "Untitled", "Dónde ", "nos juntamos el sábado?", textedit_screen()),
        ("la-notepad", "Notepad", "notepad.exe", "Untitled - Notepad", "La ", "reunión del jueves pasa al viernes.", None),
    ]
    return [
        case(f"es-label-{name}", app, app_id, before, next_text, SPANISH_LABELS + LABELS, title=title, history=SPANISH_HISTORY, screen=screen)
        for name, app, app_id, title, before, next_text, screen in rows
    ]


def notes_default():
    rows = [
        ("en-i-textedit", "TextEdit", "com.apple.textedit", "Untitled", "I ", "need to pick up the dry cleaning before six.", textedit_screen(), []),
        ("en-write-word", "Microsoft Word", "winword.exe", "Document1 - Word", "I write ", "to ask about the apartment on Elm Street.", None, []),
        ("en-keep-notepad", "Notepad", "notepad.exe", "Untitled - Notepad", "I keep ", "losing the password of the shared drive.", None, ENGLISH_HISTORY),
        ("en-my-notes", "Notes", "com.apple.Notes", "Notes", "My ", "flight lands at 7:40 on Sunday.", None, ENGLISH_HISTORY),
        ("es-escribo-textedit", "TextEdit", "com.apple.textedit", "Untitled", "Escribo ", "para confirmar la reserva del sábado.", textedit_screen(), []),
        ("es-mantengo-word", "Microsoft Word", "winword.exe", "Document1 - Word", "Mantengo ", "la oferta hasta el viernes.", None, []),
        ("es-yo-notepad", "Notepad", "notepad.exe", "Untitled - Notepad", "Yo ", "llevo el postre y las bebidas.", None, SPANISH_HISTORY),
        ("es-mis-notes", "Notes", "com.apple.Notes", "Notes", "Mis ", "papás llegan el domingo en la mañana.", None, SPANISH_HISTORY),
    ]
    return [
        case(f"notes-default-{name}", app, app_id, before, next_text, DEFAULT_NOTES + LABELS, title=title, history=history, screen=screen)
        for name, app, app_id, title, before, next_text, screen, history in rows
    ]


def self_intro():
    rows = [
        ("mail-dear", "Mail", "com.apple.mail", "New Message", None, {"field_name": "Message"}, "Dear Mr. Lee,\n\nMy ", "name is Sara Novak, and I am the new office manager at Brightline."),
        ("outlook-hello", "Microsoft Outlook", "olk.exe", "Welcome - Outlook", None, {"field_name": "Message body"}, "Hello everyone,\n\nMy ", "name is Sara Novak and I will lead the support team from Monday."),
        ("gmail-hi", "Google Chrome", "com.google.Chrome", "Compose Mail - Gmail", "mail.google.com", {"field_name": "Message Body"}, "Hi Tom, my ", "name is Sara Novak. Laura gave me your address."),
        ("word-cover", "Microsoft Word", "winword.exe", "Cover letter - Word", None, None, "To whom it may concern,\n\nMy ", "name is Sara Novak, and I am applying for the position of project coordinator."),
        ("slack-joined", "Slack", "com.tinyspeck.slackmacgap", "general - Acme - Slack", None, {"field_name": "Message input"}, "Hi all! My ", "name is Sara Novak and I just joined the design team."),
        ("mail-lowercase", "Mail", "com.apple.mail", "Invoice from March", None, {"field_name": "Message"}, "Good morning,\n\nmy ", "name is Sara Novak and I am writing about the invoice from March."),
        ("teams-hello", "Microsoft Teams", "ms-teams.exe", "Chat - Microsoft Teams", None, {"field_name": "Type a message"}, "Hello, my ", "name is Sara Novak, the new contact for your account."),
        ("textedit-neighbors", "TextEdit", "com.apple.textedit", "Untitled", None, None, "Dear neighbors,\n\nMy ", "name is Sara Novak and I live in apartment 4B."),
        ("mail-typed", "Mail", "com.apple.mail", "New Message", None, {"field_name": "Message"}, "Dear Ms. Patel,\n\nMy na", "me is Sara Novak, and I write about the lease of the shop."),
    ]
    return [
        case(f"self-intro-{name}", app, app_id, before, next_text, [], title=title, domain=domain, field=field, history=ENGLISH_HISTORY)
        for name, app, app_id, title, domain, field, before, next_text in rows
    ]


def all_cases():
    return newline_after_label() + placeholder() + agent_ask() + self_source() + spanish_label() + notes_default() + self_intro()


def holds_leak(text, leaks):
    """The match of the eval (`holds_leak` in score.rs): any case, and a leak without whitespace
    only as a whole word, where a letter at its edge does not touch a letter and a digit at its
    edge does not touch a digit."""
    text = text.lower()

    def joins(edge, neighbor):
        return (edge.isalpha() and neighbor.isalpha()) or (edge.isdigit() and neighbor.isdigit())

    for leak in leaks:
        leak = leak.lower()
        is_one_word = not any(character.isspace() for character in leak)
        start = text.find(leak)
        while start >= 0:
            end = start + len(leak)
            cut_before = start > 0 and joins(leak[0], text[start - 1])
            cut_after = end < len(text) and joins(leak[-1], text[end])
            if not is_one_word or not (cut_before or cut_after):
                return True
            start = text.find(leak, start + 1)
    return False


def text_of(cases):
    return "".join(json.dumps(case, ensure_ascii=False) + "\n" for case in cases)


def build():
    CASES_PATH.write_text(text_of(all_cases()), encoding="utf-8", newline="\n")


def check():
    cases = all_cases()
    is_same = CASES_PATH.read_text(encoding="utf-8") == text_of(cases)
    if not is_same:
        print("leaks_adversarial.jsonl differs from the generated cases")
    leaking = [case["id"] for case in cases if holds_leak(case["next"], case.get("leaks", []))]
    for case_id in leaking:
        print(f"{case_id}: its next holds a leak")
    for group in GROUPS:
        ids = [case["id"] for case in cases if case["id"].startswith(f"{group}-")]
        print(f"{group}: {len(ids)} cases")
    return is_same and not leaking


if __name__ == "__main__":
    if sys.argv[1:] == ["--check"]:
        sys.exit(0 if check() else 1)
    build()
