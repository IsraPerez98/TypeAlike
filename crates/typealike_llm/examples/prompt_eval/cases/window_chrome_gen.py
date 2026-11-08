#!/usr/bin/env python3
"""Builds window_chrome.jsonl: the cases of recall.jsonl and tuning.jsonl with screen text in a
browser or a document app, with the lines of the app around the page that an OCR of the whole
window reads.

    python3 window_chrome_gen.py                  writes window_chrome.jsonl
    python3 -I window_chrome_gen.py --clean FILE  writes the same cases without the app lines to FILE
    python3 -I window_chrome_gen.py --check       checks window_chrome.jsonl

The app lines are made up, as a live OCR read them:

- browser: the tabs, the address and the items of the bookmarks bar above the page.
- word: the title bar and the ribbon of Word above the document.
- notion: the sidebar left of the page and the bar above it.

To measure the cost of the app lines, run the clean cases with --save, then window_chrome.jsonl with
--baseline.
"""

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
CASES_PATH = HERE / "window_chrome.jsonl"
SOURCES = [HERE / "recall.jsonl", HERE / "tuning.jsonl"]
BROWSERS = {"chrome.exe", "com.google.chrome", "com.apple.safari", "msedge.exe"}
KINDS = {"winword.exe": "word", "notion.id": "notion"} | {app_id: "browser" for app_id in BROWSERS}
ROW = 22
BOOKMARKS = [
    "Recipes",
    "Bank login",
    "Photo album",
    "Football scores",
    "Trail maps",
    "Weather radar",
    "School portal",
    "Podcasts",
    "Rent payments",
    "All Bookmarks",
]
OTHER_TABS = ["Weekly meal plan", "Garden club calendar"]
RIBBON = [
    ("AutoSave Off", 20, 140),
    ("File Home Insert Draw Design Layout References Mailings Review View Help", 20, 900),
    ("Paste", 20, 70),
    ("Calibri (Body)", 90, 220),
    ("Styles", 700, 780),
    ("Dictate", 820, 900),
    ("Editor", 920, 990),
]
NOTION_SIDEBAR = [
    "Search",
    "Home",
    "Inbox",
    "Private",
    "Reading list",
    "Travel ideas",
    "Budget 2026",
    "Meeting notes",
    "Settings",
    "Trash",
]


def line(text, left, top, right):
    return {"text": text, "frame": [left, top, right, top + ROW]}


def page_name(case):
    """Returns the name of the site of `case` as a tab shows it, such as "Greenleaf"."""
    return case["domain"].removeprefix("www.").split(".")[0].capitalize()


def browser_lines(case, top):
    tabs = [page_name(case)] + OTHER_TABS
    lines = [line(tab, 80 + 250 * index, top, 300 + 250 * index) for index, tab in enumerate(tabs)]
    lines.append(line(case["domain"] + "/contact", 160, top + 38, 900))
    lines += [line(item, 20 + 140 * index, top + 76, 140 + 140 * index) for index, item in enumerate(BOOKMARKS)]
    return lines


def word_lines(case, top):
    title = line(case["window_title"], 400, top, 1040)
    ribbon = [line(text, left, top + 38 + 38 * (index > 1), right) for index, (text, left, right) in enumerate(RIBBON)]
    return [title] + ribbon


def notion_lines(case, top):
    field_top = case["screen"]["field"][1]
    sidebar = [line(item, 20, field_top - 400 + 40 * index, 220) for index, item in enumerate(NOTION_SIDEBAR)]
    return sidebar + [line("Share", 1000, top, 1060), line("Edited just now", 820, top, 980)]


def app_lines(case):
    """Returns the made-up lines of the app of `case` above its topmost screen line."""
    screen = case["screen"]
    first_top = min([screen["field"][1]] + [line["frame"][1] for line in screen["lines"]])
    top = first_top - 4 * 38
    kind = KINDS[case["app_id"].lower()]
    return {"browser": browser_lines, "word": word_lines, "notion": notion_lines}[kind](case, top)


def source_cases():
    cases = []
    for source in SOURCES:
        for raw in source.read_text(encoding="utf-8").splitlines():
            case = json.loads(raw)
            if case.get("screen") and case["app_id"].lower() in KINDS:
                cases.append({**case, "id": f"wc-{KINDS[case['app_id'].lower()]}-{case['id']}"})
    return cases


def built_cases():
    return [
        {**case, "screen": {**case["screen"], "lines": app_lines(case) + case["screen"]["lines"]}}
        for case in source_cases()
    ]


def write(path, cases):
    text = "".join(json.dumps(case, ensure_ascii=False) + "\n" for case in cases)
    path.write_text(text, encoding="utf-8")
    print(f"wrote {len(cases)} cases to {path}")


def check():
    problems = []
    cases = [json.loads(raw) for raw in CASES_PATH.read_text(encoding="utf-8").splitlines()]
    if cases != built_cases():
        problems.append(f"{CASES_PATH.name} differs from the source sets, run window_chrome_gen.py")
    ids = [case["id"] for case in cases]
    if len(ids) != len(set(ids)):
        problems.append("duplicate ids")
    groups = {}
    for case in cases:
        groups.setdefault(case["id"].split("-")[1], []).append(case)
    print(f"{len(cases)} cases")
    for group, group_cases in groups.items():
        added = sum(len(app_lines(case)) for case in group_cases)
        print(f"{group:8} {len(group_cases):3} cases, app lines {added}")
    for problem in problems:
        print("PROBLEM:", problem)
    return not problems


if __name__ == "__main__":
    if sys.argv[1:] == ["--check"]:
        sys.exit(0 if check() else 1)
    if sys.argv[1:2] == ["--clean"]:
        write(Path(sys.argv[2]), source_cases())
        sys.exit(0)
    write(CASES_PATH, built_cases())
