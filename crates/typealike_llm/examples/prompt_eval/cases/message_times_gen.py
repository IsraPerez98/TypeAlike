#!/usr/bin/env python3
"""Builds message_times.jsonl: the bubble chats of conversation.jsonl and scrolled.jsonl, where
text recognition joins the time of a message to the end of its last line.

    python3 message_times_gen.py            writes message_times.jsonl
    python3 -I message_times_gen.py --check  checks message_times.jsonl and prints the stats of each group

Telegram and WhatsApp show the time of a message at the end of its bubble. When a small gap divides
the time from the message, the text recognition of macOS reads them as one line ("see you there
9:57"), and it does not read the check marks after the time. Each case of the source sets with a
time at the end of a bubble row gives four cases:

- joined: the same caret and next word, with the times joined to their lines. Use it to measure
  the harm of a change.
- end: the caret after the whole message, with the joined times as leaks. Use it to count the
  suggestions that copy a time of a message.
- joinedpm and endpm: the same cases on a 12-hour clock, with the half of the day joined to each
  time, as text recognition reads "9:41 PM" ("9:41PM"). The leaks are the times without the half of
  the day.

Run the eval with --name "Sara Novak" --word-choice medium.
"""

import importlib.util
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
CASES_PATH = HERE / "message_times.jsonl"
SOURCES = [HERE / "conversation.jsonl", HERE / "scrolled.jsonl"]
MESSAGE_TIME = re.compile(r"^(\d{1,2}:\d{2}(?:AM|PM)?)( ✓✓)?$")
HALF_OF_DAY = re.compile(r"(AM|PM)$")
END_SIGNS = " .!?"
MIN_ROW_OVERLAP = 8
MIN_COLUMN_LEFT = 400


def load_conversation_gen():
    spec = importlib.util.spec_from_file_location("conversation_gen", HERE / "conversation_gen.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


CASE_KEYS = load_conversation_gen().CASE_KEYS | {"earlier_screens", "screen_gap_seconds"}


def is_on_row(line, time_line):
    """Whether `line` is a message row left of `time_line` in the chat column, on the same row."""
    top = max(line["frame"][1], time_line["frame"][1])
    bottom = min(line["frame"][3], time_line["frame"][3])
    return (
        MIN_COLUMN_LEFT <= line["frame"][0]
        and line["frame"][2] <= time_line["frame"][0]
        and bottom - top >= MIN_ROW_OVERLAP
    )


def joined_screen(screen):
    """Returns `screen` with each message time joined to the row that it ends, and the joined times."""
    times = [line for line in screen["lines"] if MESSAGE_TIME.match(line["text"])]
    lines = [dict(line, frame=list(line["frame"])) for line in screen["lines"] if not MESSAGE_TIME.match(line["text"])]
    joined_times = []
    for time_line in times:
        row = [line for line in lines if is_on_row(line, time_line)]
        if not row:
            lines.append(time_line)
            continue
        time_text = MESSAGE_TIME.match(time_line["text"]).group(1)
        message = max(row, key=lambda line: line["frame"][2])
        message["text"] += " " + time_text
        message["frame"][2] = time_line["frame"][2]
        joined_times.append(time_text)
    return {**screen, "lines": lines}, joined_times


def twelve_hour_time(time_text):
    """Returns `time_text` on a 12-hour clock with the half of the day joined to it, as "9:41PM"."""
    hours, minutes = time_text.split(":")
    half = "AM" if int(hours) < 12 else "PM"
    return f"{(int(hours) - 1) % 12 + 1}:{minutes}{half}"


def twelve_hour_screen(screen):
    """Returns `screen` with each message time on a 12-hour clock."""
    lines = []
    for line in screen["lines"]:
        match = MESSAGE_TIME.match(line["text"])
        text = twelve_hour_time(match.group(1)) + (match.group(2) or "") if match else line["text"]
        lines.append(dict(line, text=text))
    return {**screen, "lines": lines}


def twelve_hour_case(case):
    """Returns `case` with each message time of its reads on a 12-hour clock."""
    twelve_hour = {**case, "screen": twelve_hour_screen(case["screen"])}
    if "earlier_screens" in case:
        twelve_hour["earlier_screens"] = [twelve_hour_screen(earlier) for earlier in case["earlier_screens"]]
    return twelve_hour


def joined_case(case):
    """Returns `case` with joined times in each of its reads, and the joined times of its last read."""
    screen, joined_times = joined_screen(case["screen"])
    joined = {**case, "screen": screen}
    if "earlier_screens" in case:
        joined["earlier_screens"] = [joined_screen(earlier)[0] for earlier in case["earlier_screens"]]
    return joined, joined_times


def content_text(case, joined_times):
    """Returns the text of `case` that the writer and the messages give, without the joined times."""
    texts = [case["before"], case["next"]] + [line["text"] for line in case["screen"]["lines"]]
    text = "\n".join(texts) + "\n"
    for time_text in joined_times:
        text = text.replace(" " + time_text + "\n", "\n")
    return text


def cases_of(case, suffix=""):
    if not case["screen"]:
        return []
    joined, joined_times = joined_case(case)
    if not joined_times:
        return []
    content = content_text(joined, joined_times)
    clock_times = {HALF_OF_DAY.sub("", time_text) for time_text in joined_times}
    leaks = sorted(time_text for time_text in clock_times if time_text not in content)
    end = {
        **joined,
        "id": f"mt-end{suffix}-{case['id']}",
        "before": (case["before"] + case["next"]).rstrip(END_SIGNS) + " ",
        "next": "",
        "leaks": leaks,
    }
    return [{**joined, "id": f"mt-joined{suffix}-{case['id']}"}, end]


def built_cases():
    sources = [json.loads(raw) for source in SOURCES for raw in source.read_text(encoding="utf-8").splitlines()]
    cases = [built for case in sources for built in cases_of(case)]
    with_screen = [case for case in sources if case["screen"]]
    return cases + [built for case in with_screen for built in cases_of(twelve_hour_case(case), "pm")]


def build():
    cases = built_cases()
    text = "".join(json.dumps(case, ensure_ascii=False) + "\n" for case in cases)
    CASES_PATH.write_text(text, encoding="utf-8")
    print(f"wrote {len(cases)} cases to {CASES_PATH.name}")


def lines_with_time_at_end(case):
    return sum(1 for line in case["screen"]["lines"] if re.search(r" \d{1,2}:\d{2}(AM|PM)?$", line["text"]))


def check():
    problems = []
    cases = [json.loads(raw) for raw in CASES_PATH.read_text(encoding="utf-8").splitlines()]
    if cases != built_cases():
        problems.append(f"{CASES_PATH.name} differs from the source sets, run message_times_gen.py")
    ids = [case["id"] for case in cases]
    if len(ids) != len(set(ids)):
        problems.append("duplicate ids")
    groups = {}
    for case in cases:
        unknown = set(case) - CASE_KEYS
        if unknown:
            problems.append(f"{case['id']}: unknown keys {sorted(unknown)}")
        groups.setdefault(case["id"].split("-")[1], []).append(case)
    for case in groups.get("end", []) + groups.get("endpm", []):
        if not case["leaks"]:
            problems.append(f"{case['id']}: no leaks")
        if any(leak in case["before"] for leak in case["leaks"]):
            problems.append(f"{case['id']}: a leak is in the text before the caret")
    print(f"{len(cases)} cases")
    for group, group_cases in groups.items():
        apps = sorted({case["app"] for case in group_cases})
        joined = sum(lines_with_time_at_end(case) for case in group_cases)
        leaks = sum(len(case.get("leaks", [])) for case in group_cases)
        print(f"{group:7} {len(group_cases):3} cases, apps {apps}, lines with a time at the end {joined}, leaks {leaks}")
    for problem in problems:
        print("PROBLEM:", problem)
    return not problems


if __name__ == "__main__":
    if sys.argv[1:] == ["--check"]:
        sys.exit(0 if check() else 1)
    build()
