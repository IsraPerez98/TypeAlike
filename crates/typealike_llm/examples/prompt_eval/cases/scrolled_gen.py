#!/usr/bin/env python3
"""Builds scrolled.jsonl: prompt_eval cases where the screen was read before the last read.

    python3 scrolled_gen.py            writes scrolled.jsonl
    python3 -I scrolled_gen.py --check  checks scrolled.jsonl and prints the stats of each group

Each case gives the earlier reads of the same window (earlier_screens) before the last read
(screen), in a laptop window with a short chat area. The writer is Sara Novak in each case, so run
the eval with --name "Sara Novak". The groups: scrolled (an earlier read showed the fact, and the
last read shows the bottom of the chat without it, with lines in common), switched (the earlier
read is another chat of the same app with a conflicting fact, and the fact of the current chat is
on the screen), stale (as scrolled, but the earlier read came 15 minutes before) and visible
(control, the fact is in the last read, after an earlier read of the same chat).

The layout and the port of the line pick of the prompt come from conversation_gen.py.
"""

import importlib.util
import json
import re
import statistics
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
CASES_PATH = HERE / "scrolled.jsonl"


def load_conversation_gen():
    spec = importlib.util.spec_from_file_location("conversation_gen", HERE / "conversation_gen.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


conversation = load_conversation_gen()

FIELD_TOP = 900
CHAT_BOTTOM = FIELD_TOP - 24
FACT_MARGIN = 60
OVERLAP = 120
VISIBLE_SCROLL = 300

SCREEN_GAP = 20
STALE_GAP = 900
MAX_LINE_AGE = 600
MIN_SHARED_LINE_CHARS = 20
MIN_SHARED_LINES = 2
SCREEN_TOKENS = 800


def whatsapp(contact, status="online"):
    return ("whatsapp", "WhatsApp", [contact, status], None)


def telegram(contact):
    return ("telegram", contact, [contact, "last seen recently"], None)


def messages_app(contact):
    return ("messages", contact, [f"To: {contact}"], None)


FLIGHT = whatsapp("Jonas Weber")
BIRTHDAY = whatsapp("Beatriz Solano", "click here for contact info")
MOVING = telegram("Felipe Araya")
SWIM = messages_app("Hana Okafor")
OFFSITE = conversation.SLACK_OFFSITE
TEAM_DINNER = ("slack", "#team-dinner - Brightloom - Slack", ["# team-dinner", "Friday dinner plans"], "Message #team-dinner")


def chat_window(place, messages, before, scroll):
    """Returns the screen of the chat of `place`, scrolled up by `scroll` pixels from its bottom."""
    app_key, _, top_lines, _ = place
    _, height = conversation.layout(app_key, messages, 0)
    body, _ = conversation.layout(app_key, messages, CHAT_BOTTOM - height + scroll)
    shown = [line for line in body if line["frame"][1] >= conversation.CHAT_TOP and line["frame"][3] <= CHAT_BOTTOM]
    chrome_left = conversation.HEADER_TEXT_LEFT if app_key in conversation.BUBBLE_APPS else 440
    chrome = [conversation.screen_line(text, chrome_left, 36 + 26 * index, 8) for index, text in enumerate(top_lines)]
    typed_line = before.split("\n")[-1]
    inside_text = typed_line[:-1][-90:] or conversation.APPS[app_key].get("placeholder", "Message")
    inside = conversation.screen_line(inside_text, 440, FIELD_TOP + 15, 8)
    caret_left = 440 + min(len(typed_line), 110) * 8
    lines = conversation.sidebar_lines(app_key, FIELD_TOP) + chrome + shown + [inside]
    lines.sort(key=lambda line: (line["frame"][1], line["frame"][0]))
    return {
        "field": [conversation.COLUMN_LEFT, FIELD_TOP, conversation.COLUMN_RIGHT, FIELD_TOP + conversation.FIELD_HEIGHT],
        "caret": [caret_left, FIELD_TOP + 14, caret_left + 1, FIELD_TOP + 36],
        "lines": lines,
    }


def first_word(text):
    return text.split()[0].rstrip(".,!?:;")


def holds_word(word, text):
    return re.search(rf"(?<![\w:]){re.escape(word)}(?![\w:])", text) is not None


def fact_scroll(place, messages, fact):
    """Returns the scroll that shows the first line with `fact` FACT_MARGIN below the top of the chat."""
    app_key = place[0]
    _, height = conversation.layout(app_key, messages, 0)
    body, _ = conversation.layout(app_key, messages, CHAT_BOTTOM - height)
    top = next(line["frame"][1] for line in body if holds_word(fact, line["text"]))
    most = max(0, height - (CHAT_BOTTOM - conversation.CHAT_TOP))
    return min(most, max(0, conversation.CHAT_TOP + FACT_MARGIN - top))


def scrolls_down_from(scroll):
    """Returns the scrolls of the reads from `scroll` down to the bottom, without the bottom, each
    read OVERLAP pixels inside the read before it."""
    step = CHAT_BOTTOM - conversation.CHAT_TOP - OVERLAP
    count = -(-scroll // step)
    return [round(scroll * (count - index) / count) for index in range(count)]


def chat_record(case_id, place, messages, before, next_text, earlier_screens, gap=None):
    app_key, title, _, field_name = place
    screen = chat_window(place, messages, before, 0)
    record = conversation.case_record(case_id, app_key, title, before, next_text, screen, [], field_name=field_name)
    record["earlier_screens"] = earlier_screens
    if gap:
        record["screen_gap_seconds"] = gap
    return record


def scrolled_case(case_id, place, messages, before, next_text, gap=None):
    scroll = fact_scroll(place, messages, first_word(next_text))
    earlier = [chat_window(place, messages, before, step) for step in scrolls_down_from(scroll)]
    return chat_record(case_id, place, messages, before, next_text, earlier, gap)


def switched_case(case_id, other, other_messages, other_fact, place, messages, before, next_text):
    earlier = [chat_window(other, other_messages, "", fact_scroll(other, other_messages, other_fact))]
    return chat_record(case_id, place, messages, before, next_text, earlier)


def visible_case(case_id, place, messages, before, next_text):
    earlier = [chat_window(place, messages, before, VISIBLE_SCROLL)]
    return chat_record(case_id, place, messages, before, next_text, earlier)


def scrolled_cases():
    flight, birthday, moving, swim, offsite = conversation.FLIGHT, conversation.BIRTHDAY, conversation.MOVING, conversation.SWIM, conversation.OFFSITE
    return [
        scrolled_case("scr-scrolled-01", FLIGHT, flight + [("in", "10:40", "Ok one more thing, what time do I land again? I deleted the email by accident 🙈")], "You land at ", "14:25"),
        scrolled_case("scr-scrolled-02", FLIGHT, flight + [("in", "10:40", "My airline app logged me out and I can't find the flight number, do you still have it?")], "Yes, it is ", "KL 1394"),
        scrolled_case("scr-scrolled-03", FLIGHT, flight + [("in", "10:41", "Did you ask anyone about the cake yet? I can call the bakery too")], "Not yet, I will ask ", "Ruth tonight"),
        scrolled_case("scr-scrolled-04", BIRTHDAY, birthday + [("in", "19:20", "Oye, ¿a qué hora era la reserva? Se me olvidó y el Marco me está preguntando")], "A las ", "21:30"),
        scrolled_case("scr-scrolled-05", BIRTHDAY, birthday + [("in", "19:20", "¿Cómo se llamaba el restorán? El Marco lo quiere buscar en el mapa")], "Se llama ", "La Higuera"),
        scrolled_case("scr-scrolled-06", BIRTHDAY, birthday + [("in", "19:21", "Ya, ¿y de qué era la torta que le gusta a la Pili? Para decirle a la pastelería")], "De ", "frambuesa"),
        scrolled_case("scr-scrolled-07", MOVING, moving + [("in", "11:30", "El conserje del edificio nuevo quiere el nombre del chofer, ¿cómo se llamaba?")], "Se llama ", "Rubén"),
        scrolled_case("scr-scrolled-08", MOVING, moving + [("in", "11:30", "Y la patente del camión, ¿te acuerdas? El conserje la necesita para abrir el portón")], "Es la ", "KTRV-52"),
        scrolled_case("scr-scrolled-09", SWIM, swim + [("in", "9:40", "The Petrov kids are joining us too, their dad asked me where the meet is. Do you remember the place?")], "Yes, it is at the ", "Corwin Aquatic Center"),
        scrolled_case("scr-scrolled-10", SWIM, swim + [("in", "9:40", "Quick question, what time does the warmup start? I want to tell the Petrov kids")], "Warmup starts at ", "7:15"),
        scrolled_case("scr-scrolled-11", OFFSITE, offsite + [("Owen Marsh", "10:38 AM", "the lodge just called me to confirm the group, they want the booking reference. anyone have it handy?")], "Yes, it is ", "LL-58213"),
        scrolled_case("scr-scrolled-12", OFFSITE, offsite + [("Mila Hart", "10:38 AM", "quick one, where does the shuttle leave from? my calendar invite has no location")], "It leaves from ", "Halden Station"),
    ]


MILA_TRIP = [
    ("day", "Today"),
    ("in", "11:02", "Hiii, I finally booked the trip home for the long weekend!"),
    ("in", "11:03", "I land on Friday at 16:50, terminal 2. Any chance you can pick me up?"),
    ("out", "11:10", "Of course! I finish work at 4, so that works perfectly"),
    ("in", "11:11", "You are a star. I will bring you the chocolate from the airport shop"),
    ("out", "11:12", "Deal, the dark one please"),
    ("in", "11:30", "Leo asked me what time I arrive, what should I tell him?"),
]

OWEN_FLIGHT = [
    ("day", "Today"),
    ("in", "8:40", "Morning! I am flying to the conference on Sunday"),
    ("in", "8:41", "It is LH 2271 from Halden, it lands at 19:05. Can you book the airport pickup for me in the company app?"),
    ("out", "8:50", "Sure, I will do it after my meeting"),
    ("in", "8:51", "Thanks! The app asks for the flight number, so you will need it"),
    ("in", "9:30", "Did the booking go through? What did you put as the flight?"),
]

ANDREA_DINNER = [
    ("day", "Today"),
    ("in", "17:02", "Hola! Reservé para la cena del viernes con las chiquillas"),
    ("in", "17:03", "Es en El Molino, en Providencia, a las 20:00. Somos seis"),
    ("out", "17:10", "Buenísimo, gracias por organizar!"),
    ("in", "17:11", "De nada. Avísale a la Cata, que no está en el grupo"),
    ("out", "17:12", "Le escribo ahora"),
    ("in", "17:40", "La Cata pregunta a qué hora es, ¿le puedes decir tú?"),
]

PILAR_LUNCH = [
    ("day", "Today"),
    ("in", "12:15", "¿Almorzamos el domingo por el cumple de mi papá?"),
    ("in", "12:16", "Reservé en Casa Ñandú, en Lastarria, a la una y media"),
    ("out", "12:20", "Sí! Ahí voy con el Tito"),
    ("in", "12:21", "Perfecto, somos ocho entonces"),
    ("in", "12:45", "Mi tía no se acuerda del nombre del lugar, ¿se lo mandas tú?"),
]

TEO_REPAIR = [
    ("day", "Today"),
    ("in", "9:05", "El técnico de la lavadora viene mañana entre 10 y 12"),
    ("in", "9:06", "Se llama Mauricio y te va a llamar antes de llegar"),
    ("out", "9:15", "Gracias, voy a estar en la casa toda la mañana"),
    ("in", "9:16", "Ok. Si no llega, me avisas y reclamo"),
    ("in", "9:40", "Mi mamá pregunta cómo se llamaba el técnico, por si llama a la casa"),
]

OWEN_TOURNAMENT = [
    ("day", "Today 8:02 AM"),
    ("in", "8:02", "The basketball tournament is on Sunday at Brookfield Arena"),
    ("in", "8:03", "Warmup at 8:45, the first game at 9:30"),
    ("out", "8:10", "Great, Leo is ready. We will bring the snacks"),
    ("in", "8:11", "Perfect. Noah needs a ride, can you take him?"),
    ("out", "8:12", "Sure, we pass by your house anyway"),
    ("in", "8:30", "Noah's grandma wants to watch, where is it again?"),
]

ROCIO_GRANDMA = [
    ("day", "Today"),
    ("in", "10:20", "El almuerzo del cumple de la abuela es el domingo en la casa de Peñaflor"),
    ("in", "10:21", "Mi mamá dice que lleguemos a las 13:00, ni antes ni después jaja"),
    ("out", "10:30", "Jajaja ok, llevo el postre"),
    ("in", "10:31", "Dale, el de lúcuma que le gusta"),
    ("in", "11:05", "El Diego pregunta a qué hora tiene que estar allá"),
]

DINNER = [
    ("Mila Hart", "4:02 PM", "Booked the table for Friday at Ostra, the seafood place on the river. 7 PM, 12 people."),
    ("Mila Hart", None, "The reservation reference is OS-4471, it is under my name."),
    ("Owen Marsh", "4:10 PM", "Nice! Can we bring partners or is it team only?"),
    ("Mila Hart", "4:12 PM", "Team only this time, sorry."),
    ("Ruth Calder", "4:30 PM", "the restaurant called to confirm, they asked for the reservation reference. what was it?"),
]


def switched_cases():
    flight, birthday, moving, swim, offsite = conversation.FLIGHT, conversation.BIRTHDAY, conversation.MOVING, conversation.SWIM, conversation.OFFSITE
    return [
        switched_case("scr-switched-01", FLIGHT, flight, "14:25", whatsapp("Mila Hart"), MILA_TRIP, "Tell him you land at ", "16:50"),
        switched_case("scr-switched-02", FLIGHT, flight, "KL", whatsapp("Owen Marsh"), OWEN_FLIGHT, "Yes, I put ", "LH 2271"),
        switched_case("scr-switched-03", BIRTHDAY, birthday, "21:30", whatsapp("Andrea Vial"), ANDREA_DINNER, "Ya le dije, a las ", "20:00"),
        switched_case("scr-switched-04", BIRTHDAY, birthday, "Higuera", whatsapp("Pilar Ortúzar"), PILAR_LUNCH, "Listo, le dije que es en ", "Casa Ñandú"),
        switched_case("scr-switched-05", MOVING, moving, "Rubén", telegram("Teo Varga"), TEO_REPAIR, "Se llama ", "Mauricio"),
        switched_case("scr-switched-06", SWIM, swim, "Corwin", messages_app("Owen Marsh"), OWEN_TOURNAMENT, "It is at ", "Brookfield Arena"),
        switched_case("scr-switched-07", BIRTHDAY, birthday, "21:30", whatsapp("Rocío Fuentes", "click here for contact info"), ROCIO_GRANDMA, "Dile que a las ", "13:00"),
        switched_case("scr-switched-08", OFFSITE, offsite, "LL-58213", TEAM_DINNER, DINNER, "It is ", "OS-4471"),
    ]


def stale_cases():
    flight, birthday, moving, swim, offsite = conversation.FLIGHT, conversation.BIRTHDAY, conversation.MOVING, conversation.SWIM, conversation.OFFSITE
    return [
        scrolled_case("scr-stale-01", FLIGHT, flight + [("in", "10:40", "Wait, which flight was it again? Mom wants to track it on her phone")], "It is ", "KL 1394", gap=STALE_GAP),
        scrolled_case("scr-stale-02", BIRTHDAY, birthday + [("in", "19:20", "El Marco pregunta en qué número de Olmedo es, no lo encuentra en el mapa")], "Es en Olmedo ", "214", gap=STALE_GAP),
        scrolled_case("scr-stale-03", MOVING, moving + [("in", "11:30", "¿A qué hora llegaba el camión el sábado? Para poner la alarma")], "Llega a las ", "8:30", gap=STALE_GAP),
        scrolled_case("scr-stale-04", SWIM, swim + [("in", "9:40", "Which lane do the kids swim in? Mia keeps asking")], "They swim in lane ", "5", gap=STALE_GAP),
        scrolled_case("scr-stale-05", OFFSITE, offsite + [("Mila Hart", "10:38 AM", "what time does the shuttle leave on the 18th? I want to book my train to the station")], "It leaves at ", "1:40 PM", gap=STALE_GAP),
    ]


def visible_cases():
    flight, birthday, moving, swim, offsite = conversation.FLIGHT, conversation.BIRTHDAY, conversation.MOVING, conversation.SWIM, conversation.OFFSITE
    return [
        visible_case("scr-visible-01", FLIGHT, flight + [("in", "10:38", "Checked in online, the flight still lands at 14:25 👍"), ("out", "10:39", "Great, I will be there"), ("in", "10:40", "Mom is asking when we get home, what should I tell her?")], "Tell her you land at ", "14:25"),
        visible_case("scr-visible-02", BIRTHDAY, birthday + [("in", "19:18", "Te confirmo que la reserva sigue a las 21:30 en La Higuera"), ("in", "19:20", "El Marco pregunta a qué hora tiene que llevar a la Pili")], "Tiene que llegar antes de las ", "21:30"),
        visible_case("scr-visible-03", MOVING, moving + [("in", "11:28", "Rubén me escribió, dice que llega puntual"), ("in", "11:30", "El conserje del edificio nuevo quiere el nombre del chofer")], "Se llama ", "Rubén"),
        visible_case("scr-visible-04", SWIM, swim + [("in", "9:38", "The coach says the parking at the Corwin Aquatic Center fills up fast, so come early"), ("in", "9:40", "The Petrov kids are joining us, their dad asked where the meet is")], "It is at the ", "Corwin Aquatic Center"),
        visible_case("scr-visible-05", OFFSITE, offsite + [("Ruth Calder", "10:35 AM", "Got the confirmation email from Larchmont. Reference LL-58213 again, in case anyone needs it."), ("Owen Marsh", "10:38 AM", "the lodge just called me, they want the booking reference")], "It is ", "LL-58213"),
    ]


def build():
    cases = scrolled_cases() + switched_cases() + stale_cases() + visible_cases()
    with CASES_PATH.open("w", encoding="utf-8") as file:
        for case in cases:
            file.write(json.dumps(case, ensure_ascii=False) + "\n")
    print(f"wrote {len(cases)} cases to {CASES_PATH.name}")


# The check: a port of ConversationLines of typealike_core/src/screen/conversation.rs on top of the
# port of the line pick in conversation_gen.py.

CASE_KEYS = conversation.CASE_KEYS | {"earlier_screens", "screen_gap_seconds"}


def lines_above_caret(screen):
    field = screen["field"]
    caret = screen["caret"] or field
    left, right = field[0], field[2]
    for line in screen["lines"]:
        frame = line["frame"]
        if frame[0] < field[2] and field[0] < frame[2]:
            left, right = min(left, frame[0]), max(right, frame[2])
    copies = set()
    lines = []
    for line in screen["lines"]:
        frame = line["frame"]
        key = (line["text"], tuple(frame))
        is_above = (frame[1] + frame[3]) // 2 < (caret[1] + caret[3]) // 2
        if frame[0] < right and left < frame[2] and is_above and not conversation.is_inside(frame, field) and key not in copies:
            copies.add(key)
            lines.append(line)
    lines.sort(key=lambda line: (line["frame"][1], line["frame"][0]))
    return lines


def follow_reads(case):
    """Returns the screen that the prompt reads after the reads of the case, and for each read after
    the first whether it continued the conversation of the reads before it."""
    reads = case.get("earlier_screens", []) + [case["screen"]]
    gap = case.get("screen_gap_seconds", SCREEN_GAP)
    seen = []
    continued = []
    for index, screen in enumerate(reads):
        now = (index + 1 - len(reads)) * gap
        shown = lines_above_caret(screen)
        texts = {line["text"] for line in shown}
        seen = [(line, seen_at) for line, seen_at in seen if now - seen_at <= MAX_LINE_AGE]
        shared = {line["text"] for line, _ in seen if len(line["text"]) >= MIN_SHARED_LINE_CHARS and line["text"] in texts}
        if index:
            continued.append(len(shared) >= MIN_SHARED_LINES)
        if len(shared) < MIN_SHARED_LINES:
            seen = []
        seen = [(line, seen_at) for line, seen_at in seen if line["text"] not in texts] + [(line, now) for line in shown]
    screen = case["screen"]
    texts = {line["text"] for line in lines_above_caret(screen)}
    scrolled_past = [line for line, seen_at in seen if -seen_at <= MAX_LINE_AGE and line["text"] not in texts]
    bottom = min([line["frame"][1] for line in screen["lines"]] + [screen["field"][1]])
    stacked = []
    for line in reversed(scrolled_past):
        frame = line["frame"]
        stacked.append({"text": line["text"], "frame": [frame[0], bottom - (frame[3] - frame[1]), frame[2], bottom]})
        bottom -= frame[3] - frame[1]
    return {**screen, "lines": stacked + screen["lines"]}, continued


def holds_fact(case, lines):
    return any(conversation.holds_fact(case, line["text"]) for line in lines)


def check_case(group, case, problems):
    """Checks the reads of `case` for its group, and returns the screen tokens and the fact distance."""
    earlier = case.get("earlier_screens", [])
    taller, continued = follow_reads(case)
    ranked = conversation.ranked_candidates({**case, "screen": taller})
    distance = conversation.fact_distance(case, ranked)
    kept = any(conversation.holds_fact(case, text) for text in conversation.kept_texts(ranked, SCREEN_TOKENS))
    is_in_last = holds_fact(case, case["screen"]["lines"])
    is_in_earlier = any(holds_fact(case, screen["lines"]) for screen in earlier)
    adds_lines = len(taller["lines"]) > len(case["screen"]["lines"])
    gap = case.get("screen_gap_seconds", SCREEN_GAP)
    expected = {
        "scrolled": not is_in_last and is_in_earlier and all(continued) and kept,
        "switched": is_in_last and not is_in_earlier and not any(continued) and not adds_lines,
        "stale": not is_in_last and is_in_earlier and gap > MAX_LINE_AGE and not adds_lines,
        "visible": is_in_last and all(continued) and adds_lines and kept,
    }[group]
    if not earlier or not expected:
        problems.append(f"{case['id']}: in last {is_in_last}, in earlier {is_in_earlier}, continued {continued}, adds lines {adds_lines}, kept at {SCREEN_TOKENS} {kept}")
    if any(screen["field"] != case["screen"]["field"] for screen in earlier):
        problems.append(f"{case['id']}: an earlier read has another field")
    return sum(conversation.estimated_tokens(text) for text in ranked), distance


def check():
    groups = {}
    ids = set()
    problems = []
    for number, raw in enumerate(CASES_PATH.read_text(encoding="utf-8").splitlines(), 1):
        case = json.loads(raw)
        unknown = set(case) - CASE_KEYS
        if unknown:
            problems.append(f"line {number}: unknown keys {sorted(unknown)}")
        if case["id"] in ids:
            problems.append(f"line {number}: duplicate id {case['id']}")
        ids.add(case["id"])
        groups.setdefault(case["id"].split("-")[1], []).append(case)
    print(f"{len(ids)} cases")
    for group, cases in groups.items():
        stats = [check_case(group, case, problems) for case in cases]
        spanish = sum(1 for case in cases if conversation.SPANISH_WORDS & set(re.findall(r"\w+", (case["before"] + " " + case["next"]).lower())))
        reads = [len(case["earlier_screens"]) for case in cases]
        print(f"{group:8} {len(cases):3} cases, {spanish:2} in Spanish, earlier reads {sorted(reads)}, prompt screen tokens median {statistics.median(tokens for tokens, _ in stats):.0f}")
        print(f"         fact distance from the caret (estimated tokens): {[distance for _, distance in stats]}")
    for problem in problems:
        print("PROBLEM:", problem)
    return not problems


if __name__ == "__main__":
    if sys.argv[1:] == ["--check"]:
        sys.exit(0 if check() else 1)
    build()
