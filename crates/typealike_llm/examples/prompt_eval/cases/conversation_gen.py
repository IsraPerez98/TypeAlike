#!/usr/bin/env python3
"""Builds conversation.jsonl: prompt_eval cases with real-size chat and email conversations.

    python3 conversation_gen.py            writes conversation.jsonl and writers/sara_conversation.jsonl
    python3 -I conversation_gen.py --check  checks conversation.jsonl and prints the stats of each group

The writer is Sara Novak in each case, so run the eval with --name "Sara Novak". The groups:
far (the fact is more than 300 estimated tokens above the caret), near (control, the fact is in
the last 150 tokens), turns (the next word depends on who wrote which bubble), answer (the fact is
in the first OCR line of a long last message), live (older texts of the same app pull another
word), header (a habit with the contact that only the chat header names) and email (the fact is
more than 100 tokens into the quoted original after the caret).
"""

import json
import re
import statistics
import sys
import textwrap
from pathlib import Path

HERE = Path(__file__).resolve().parent
CASES_PATH = HERE / "conversation.jsonl"
STORE_NAME = "sara_conversation"
STORE_PATH = HERE / "writers" / f"{STORE_NAME}.jsonl"

NOW = 1793890800  # 2026-11-05 15:00 UTC
MINUTE = 60
DAY = 86400

COLUMN_LEFT = 420
COLUMN_RIGHT = 1400
CHAT_TOP = 120
MIN_FIELD_TOP = 1290
MAX_FIELD_TOP = 1900
FIELD_HEIGHT = 50
ROW_PITCH = 22
ROW_HEIGHT = 20

BUBBLE_WRAP = 56
BUBBLE_CHAR_PX = 10.8
INCOMING_LEFT = 440
OUTGOING_RIGHT = 1380
TIME_CHARS = 10

HEADER_WRAP = 112
HEADER_CHAR_PX = 7.8
HEADER_TEXT_LEFT = 492

SCREEN_TOKENS = 300
WIDER_SCREEN_TOKENS = 800
AFTER_CARET_TOKENS = 100

APPS = {
    "whatsapp": {"app": "WhatsApp", "app_id": "net.whatsapp.WhatsApp", "placeholder": "Type a message"},
    "telegram": {"app": "Telegram", "app_id": "ru.keepcoder.Telegram", "placeholder": "Write a message..."},
    "messages": {"app": "Messages", "app_id": "com.apple.MobileSMS", "placeholder": "iMessage"},
    "slack": {"app": "Slack", "app_id": "com.tinyspeck.slackmacgap"},
    "teams": {"app": "Microsoft Teams", "app_id": "com.microsoft.teams2", "placeholder": "Type a message"},
    "discord": {"app": "Discord", "app_id": "com.hnc.Discord"},
    "mail": {"app": "Mail", "app_id": "com.apple.mail"},
}
BUBBLE_APPS = {"whatsapp", "telegram", "messages"}

SIDEBARS = {
    "whatsapp": [("Mamá", "Ok, call me later"), ("Run Club", "Owen: track at 7 tomorrow"), ("Laura Méndez", "jajaja sí"), ("Kofi Mensah", "see you there"), ("Diego Fuentes", "Photo"), ("Rocío Fuentes", "Gracias!!")],
    "telegram": [("Saved Messages", "Pasaporte, renovar antes de abril"), ("Ignacia Pardo", "¡Dale!"), ("Felipe Araya", "Ok"), ("Teo Varga", "the bags are in my car"), ("Tess Holloway", "Sticker")],
    "messages": [("Hana Okafor", "Thanks!!"), ("Owen Marsh", "sounds good"), ("Jonas Weber", "haha yes"), ("Elena Brandt", "Liked a message"), ("Mila Hart", "see you Saturday")],
    "slack": [("Brightloom", ""), ("# general", ""), ("# offsite-2026", ""), ("# random", ""), ("# release-train", ""), ("Direct messages", ""), ("Mila Hart", ""), ("Owen Marsh", "")],
    "teams": [("Chat", ""), ("Ops Weekly", "Elias: noted"), ("Ruth Calder", "thanks!"), ("Owen Marsh", "see you at 3")],
    "discord": [("PIXEL GARDEN", ""), ("# announcements", ""), ("# general", ""), ("# raid-night", ""), ("# screenshots", ""), ("Raid", "")],
}


def wrap(text, width):
    return textwrap.wrap(text, width, break_long_words=False, break_on_hyphens=False)


def screen_line(text, left, top, char_px, height=ROW_HEIGHT):
    return {"text": text, "frame": [round(left), round(top), round(left + max(len(text), 1) * char_px), round(top + height)]}


def time_minutes(time_text, day_offset):
    hours, minutes = time_text.split(":")
    return day_offset * 24 * 60 + int(hours) * 60 + int(minutes)


def day_offset_of(label):
    return -1 if label.startswith("Yesterday") else 0


def bubble_lines(messages, top, has_times):
    """Lays out bubble messages from `top`: ("day", label) or (side, "HH:MM", text[, extras])."""
    lines = []
    y = top
    previous_side = None
    last_outgoing = max((index for index, message in enumerate(messages) if message[0] == "out"), default=None)
    for index, message in enumerate(messages):
        if message[0] == "day":
            y += 10 if previous_side else 0
            label = message[1]
            lines.append(screen_line(label, 910 - len(label) * 4, y, 8, 18))
            y += 18 + 22
            previous_side = None
            continue
        side, time_text, text = message[:3]
        extras = message[3] if len(message) > 3 else {}
        if previous_side:
            y += 30 if previous_side == side else 40
        rows = list(extras.get("preview", [])) + wrap(text, BUBBLE_WRAP)
        is_time_inline = has_times and len(rows[-1]) + TIME_CHARS <= BUBBLE_WRAP
        widest = max(len(row) for row in rows)
        if is_time_inline:
            widest = max(widest, len(rows[-1]) + TIME_CHARS)
        width = widest * BUBBLE_CHAR_PX + 24
        bubble_left = INCOMING_LEFT if side == "in" else OUTGOING_RIGHT - width
        y += 8
        for row in rows:
            lines.append(screen_line(row, bubble_left + 12, y, BUBBLE_CHAR_PX))
            y += ROW_PITCH
        if has_times:
            time_label = f"{time_text} ✓✓" if side == "out" else time_text
            time_top = y - ROW_PITCH + 4 if is_time_inline else y
            time_width = len(time_label) * 7
            lines.append({"text": time_label, "frame": [round(bubble_left + width - 12 - time_width), round(time_top), round(bubble_left + width - 12), round(time_top + 16)]})
            if not is_time_inline:
                y += 18
        y += 6
        if "reaction" in extras:
            reaction_left = bubble_left + 10 if side == "in" else bubble_left + width - 50
            lines.append(screen_line(extras["reaction"], reaction_left, y - 4, 12, 18))
            y += 14
        if side == "out" and index == last_outgoing and not has_times:
            lines.append(screen_line("Delivered", OUTGOING_RIGHT - 72, y + 2, 8, 16))
            y += 20
        previous_side = side
    return lines, y


def header_lines(messages, top, is_discord):
    """Lays out header-chat messages from `top`: (speaker, time or None for the same run, text)."""
    lines = []
    y = top
    for index, (speaker, time_text, text) in enumerate(messages):
        if time_text:
            y += 16 if index else 0
            header = f"{speaker} — Today at {time_text}" if is_discord else f"{speaker}  {time_text}"
            lines.append(screen_line(header, HEADER_TEXT_LEFT, y, HEADER_CHAR_PX))
            y += 24
        else:
            y += 6
        for row in wrap(text, HEADER_WRAP):
            lines.append(screen_line(row, HEADER_TEXT_LEFT, y, HEADER_CHAR_PX))
            y += ROW_PITCH
    return lines, y


def layout(app_key, messages, top):
    if app_key in BUBBLE_APPS:
        return bubble_lines(messages, top, has_times=app_key != "messages")
    return header_lines(messages, top, is_discord=app_key == "discord")


def sidebar_lines(app_key, field_top):
    lines = []
    y = CHAT_TOP
    for name, preview in SIDEBARS[app_key]:
        if y + 44 > field_top + FIELD_HEIGHT:
            break
        lines.append(screen_line(name, 96, y, 8))
        if preview:
            lines.append(screen_line(preview if len(preview) <= 36 else preview[:35] + "…", 96, y + 24, 7))
        y += 64 if preview else 32
    return lines


def scrolled_to_fit(app_key, messages):
    """Returns the messages that fit above the field: the oldest ones scroll off the top."""
    visible = list(messages)
    while CHAT_TOP + layout(app_key, visible, 0)[1] + 24 > MAX_FIELD_TOP:
        visible.pop(next(index for index, message in enumerate(visible) if message[0] != "day"))
    return visible


def chat_screen(app_key, messages, top_lines, before):
    messages = scrolled_to_fit(app_key, messages)
    _, height = layout(app_key, messages, 0)
    field_top = max(MIN_FIELD_TOP, CHAT_TOP + height + 24)
    body, _ = layout(app_key, messages, field_top - 24 - height)
    chrome = [screen_line(text, HEADER_TEXT_LEFT if app_key in BUBBLE_APPS else 440, 36 + 26 * index, 8) for index, text in enumerate(top_lines)]
    typed_line = before.split("\n")[-1]
    typed_before_key = typed_line[:-1]
    inside_text = typed_before_key[-90:] if typed_before_key else APPS[app_key].get("placeholder", "Message")
    inside = screen_line(inside_text, 440, field_top + 15, 8)
    caret_left = 440 + min(len(typed_line), 110) * 8
    lines = sidebar_lines(app_key, field_top) + chrome + body + [inside]
    lines.sort(key=lambda line: (line["frame"][1], line["frame"][0]))
    return {
        "field": [COLUMN_LEFT, field_top, COLUMN_RIGHT, field_top + FIELD_HEIGHT],
        "caret": [caret_left, field_top + 14, caret_left + 1, field_top + 36],
        "lines": lines,
    }


def sent_history(app_key, title, messages, now):
    """Returns the outgoing bubbles as earlier texts of the same app, timed before `now`."""
    timed = []
    day_offset = 0
    for message in messages:
        if message[0] == "day":
            day_offset = day_offset_of(message[1])
            continue
        timed.append((message[0], time_minutes(message[1], day_offset), message[2]))
    case_minutes = max(minutes for _, minutes, _ in timed) + 2
    app = APPS[app_key]
    return [
        history_entry(text, app_key, title, now - (case_minutes - minutes) * MINUTE)
        for side, minutes, text in timed
        if side == "out"
    ]


def history_entry(text, app_key, title, written_at):
    app = APPS[app_key]
    return {"text": text, "app": app["app"], "app_id": app["app_id"], "domain": None, "window_title": title, "written_at": written_at}


def case_record(case_id, app_key, title, before, next_text, screen, history, field_name=None, writer=None, after=""):
    app = APPS[app_key]
    record = {"id": case_id, "app": app["app"], "app_id": app["app_id"], "window_title": title, "domain": None}
    if field_name:
        record["field_name"] = field_name
    if app.get("placeholder") and app_key != "mail":
        record["field_placeholder"] = field_name or app["placeholder"]
    record.update({"before": before, "after": after, "clipboard": None, "screen": screen})
    if writer:
        record["writer"] = writer
    record.update({"now": NOW, "history": history, "next": next_text})
    return record


def chat_case(case_id, app_key, title, top_lines, messages, before, next_text, with_sent=False, history=(), field_name=None, writer=None):
    screen = chat_screen(app_key, messages, top_lines, before)
    entries = list(history)
    if with_sent:
        entries += sent_history(app_key, title, messages, NOW)
    return case_record(case_id, app_key, title, before, next_text, screen, entries, field_name=field_name, writer=writer)


def quoted_original(sent, sender, address, paragraphs):
    quoted = []
    for paragraph in paragraphs:
        for part in paragraph.split("\n"):
            quoted += ["> " + row for row in wrap(part, 72)]
        quoted.append(">")
    return f"\n\nOn {sent}, {sender} <{address}> wrote:\n\n" + "\n".join(quoted[:-1])


def email_case(case_id, subject, sender, address, sent, paragraphs, before, next_text, has_screen):
    title = f"Re: {subject}"
    after = quoted_original(sent, sender, address, paragraphs)
    screen = None
    if has_screen:
        rows = before.split("\n")
        caret_top = 300 + (len(rows) - 1) * 22
        caret_left = 440 + len(rows[-1]) * 8
        screen = {
            "field": [COLUMN_LEFT, 280, COLUMN_RIGHT, 1300],
            "caret": [caret_left, caret_top, caret_left + 1, caret_top + 22],
            "lines": [
                screen_line(f"To: {sender}", 440, 150, 8),
                screen_line("Cc:", 440, 182, 8),
                screen_line(f"Subject: {title}", 440, 214, 8),
            ],
        }
    return case_record(case_id, "mail", title, before, next_text, screen, [], after=after)


# Long bodies of the far and near groups.

OFFSITE = [
    ("Owen Marsh", "9:02 AM", "Morning all! Quick recap of the offsite plan so far, so we stop digging through threads. Dates are Nov 18 to 20, we leave Wednesday after lunch and come back Friday evening."),
    ("Owen Marsh", None, "Budget is approved for 14 people, Ruth is handling the venue and Teo the transport."),
    ("Ruth Calder", "9:10 AM", "Venue is done. I booked the Larchmont Lodge, three cabins plus the main hall. The booking reference is LL-58213, it is under the Brightloom account."),
    ("Ruth Calder", None, "Check-in is from 3 PM, so we should not arrive before that or we will stand around in the parking lot."),
    ("Teo Varga", "9:14 AM", "Transport: a shuttle leaves from Halden Station at 1:40 PM on the 18th, the ride takes about 75 minutes. If you drive yourself, tell me so I can give the driver the right head count."),
    ("Mila Hart", "9:21 AM", "Nice work both. For the agenda I am thinking a long session on the roadmap Thursday morning, then the customer panel after lunch, and Friday is for team retros and the hike."),
    ("Mila Hart", None, "If anyone wants to run a workshop, add it to the agenda doc by Monday. We have room for two more slots of 45 minutes."),
    ("Sara Novak", "9:25 AM", "I can run the one about the support handoff, the one we talked about last sprint. 45 minutes is enough if people read the notes before."),
    ("Mila Hart", "9:26 AM", "Perfect, I will put you down for Thursday at 4."),
    ("Owen Marsh", "9:31 AM", "Food: the lodge does breakfast and dinner, lunch is on us. Please put allergies and diet stuff in the form, not here, so it does not get lost."),
    ("Ruth Calder", "9:33 AM", "Also rooms are shared, two per room. If you have a preference for a roommate, DM me by Friday, otherwise I will match people by team."),
    ("Mila Hart", "9:44 AM", "Can someone own the evening games? Last time the quiz was the best part."),
    ("Sara Novak", "9:46 AM", "I can do the quiz again if someone helps me with the questions. Teo, you in?"),
    ("Teo Varga", "9:47 AM", "Sure, I will do the music round, you do the rest."),
    ("Owen Marsh", "9:52 AM", "Love it. The weather looks cold and wet for that week, so bring boots and a rain jacket for the hike. The trail is about 9 km with one steep part."),
    ("Mila Hart", "10:03 AM", "Reminder that the customer panel has three guests, all from the pilot program. Please do not pitch them anything, it is a listening session."),
    ("Teo Varga", "10:18 AM", "And if you want a seat on the shuttle, fill in the transport form by Friday, or I will assume you drive."),
    ("Owen Marsh", "10:22 AM", "One more thing, Kofi joins remotely on Thursday for the roadmap part, so we need a good mic in the hall."),
    ("Ruth Calder", "10:24 AM", "The lodge has a conference speaker, I asked them to set it up."),
]

RELEASE = [
    ("Ines Duarte", "9:05 AM", "Morning! Release train status for 7.4: the release branch is cut, and QA started on the first candidate yesterday."),
    ("Ines Duarte", None, "Build 4498 failed the payment smoke tests, so please do not use it for anything."),
    ("Kofi Mensah", "9:12 AM", "Build 4503 had the broken migration for the audit table, I reverted it this morning."),
    ("Nadia Petrov", "9:20 AM", "Ok, after the revert we made build 4512 and it passed the full regression run overnight. That is our candidate unless something new comes up."),
    ("Nadia Petrov", None, "I am also the rollback owner this week, so ping me first if anything looks bad after the deploy."),
    ("Sara Novak", "9:24 AM", "Thanks Nadia. I updated the release notes draft, the two big items are the new export screen and the faster search."),
    ("Sara Novak", None, "Kofi, can you check the wording for the export part? I am not sure we can call it beta."),
    ("Kofi Mensah", "9:30 AM", "Yes, we call it beta, product agreed last week. I will add a line about the file size limit too."),
    ("Ines Duarte", "9:41 AM", "Code freeze is Thursday at 18:00. After that only fixes for blockers, and each one needs two reviews."),
    ("Ines Duarte", None, "The deploy window is Friday morning, 7 to 9, before the traffic peak."),
    ("Kofi Mensah", "9:47 AM", "Do we want the feature flag for smart routing on at launch or off?"),
    ("Nadia Petrov", "9:50 AM", "Off at launch. We turn it on for 5 percent of the accounts on Monday and watch the error rate for a day."),
    ("Sara Novak", "9:58 AM", "I will write the status page note for Friday. Something short like a planned maintenance with no downtime expected."),
    ("Ines Duarte", "10:04 AM", "Perfect. Also the mobile team asked if the API changes are backward compatible. I said yes, but someone please confirm."),
    ("Kofi Mensah", "10:09 AM", "Confirmed, all new fields are optional and nothing was removed. The old export endpoint stays until March."),
    ("Ines Duarte", "10:11 AM", "Great. Last thing, the support team wants a short demo of the export screen on Thursday afternoon."),
    ("Sara Novak", "10:12 AM", "I can do the demo, 20 minutes at 3 PM works for me."),
    ("Ines Duarte", "10:15 AM", "Booked it. Thanks everyone, see you at the go or no-go meeting on Thursday."),
    ("Kofi Mensah", "10:20 AM", "One open question: do we bump the minimum app version in this release or in the next one?"),
    ("Ines Duarte", "10:23 AM", "Next one. We do not want two risky changes in the same week."),
]

OPS = [
    ("Owen Marsh", "8:55 AM", "Morning team. Agenda for today: the packaging order, the new shift plan, and the forklift training dates."),
    ("Ruth Calder", "9:01 AM", "Packaging first. We placed the big order with Fernhill Packaging, 1,200 boxes and 80 rolls of tape. The PO number is PO-77310."),
    ("Ruth Calder", None, "Their truck arrives on Wednesday at 7 AM at Dock 14, the one next to the cold room."),
    ("Elias Brandt", "9:06 AM", "Who signs for the delivery? I am off on Wednesday until noon."),
    ("Owen Marsh", "9:07 AM", "Sara or me. Sara, can you take it?"),
    ("Sara Novak", "9:08 AM", "Yes, I can take it. I will be there from 6:30 anyway for the early shift."),
    ("Owen Marsh", "9:15 AM", "Thanks. Now the shift plan. From December we move to three shifts on weekdays, the late one ends at 23:00."),
    ("Elias Brandt", "9:18 AM", "Do we have enough people for the late shift? Two of the night crew asked to switch to mornings."),
    ("Owen Marsh", "9:20 AM", "Not yet. HR posts two jobs this week, and we can borrow one person from the Riverside site for a month."),
    ("Ruth Calder", "9:24 AM", "I can do the late shift on Mondays and Tuesdays if that helps, but not on Fridays."),
    ("Owen Marsh", "9:27 AM", "That helps a lot, thanks Ruth. I will put it in the draft and share it on Thursday."),
    ("Elias Brandt", "9:33 AM", "Forklift training: the trainer can come on the 18th or the 25th. It takes a full day and we can send six people."),
    ("Sara Novak", "9:35 AM", "The 25th is better, the 18th is inventory day."),
    ("Owen Marsh", "9:36 AM", "Agreed, the 25th. Elias, please send the list of names by Friday."),
    ("Elias Brandt", "9:38 AM", "Will do. Also the safety vests in the locker room are almost gone, should I order more?"),
    ("Ruth Calder", "9:40 AM", "Yes, order 30 in size L and 20 in size M. Use the normal supplier, not the cheap one from last time."),
    ("Owen Marsh", "9:44 AM", "Last point from me: the heating in the loading area is fixed, finally. Thanks for your patience with that."),
    ("Elias Brandt", "9:51 AM", "And the coffee machine on the second floor is broken again, just so everyone knows."),
    ("Ruth Calder", "9:58 AM", "I called the service company, they come tomorrow morning."),
]

RAID = [
    ("quillfox", "19:58", "Raid night this Saturday! We are doing the Sunken Archive again, the full run this time."),
    ("quillfox", None, "I set up a private server so randoms do not join. The name is Pixel Garden Raid, the password is TIDEPOOL, all caps."),
    ("nightjar", "20:03", "Nice. What time do we start?"),
    ("quillfox", "20:04", "We start at 21:30 server time, please be online 15 minutes before for the gear check."),
    ("mossbyte", "20:07", "I can only stay until midnight, is that ok?"),
    ("quillfox", "20:08", "Should be fine, the full run takes about two hours if nobody wipes on the second boss."),
    ("Sara Novak", "20:11", "I can bring the healer again. Do we have a second tank or only Bram?"),
    ("nightjar", "20:12", "Bram and me. I finally got the shield from the last run."),
    ("mossbyte", "20:15", "Can someone explain the second boss again? Last time I died in the first ten seconds."),
    ("quillfox", "20:18", "Stay out of the green circles, and when he jumps, everyone runs to the pillars. The healer stays in the middle."),
    ("Sara Novak", "20:20", "And please do not pull the adds before the tank is ready, that is what killed us last time."),
    ("nightjar", "20:22", "That was me, sorry. I will behave."),
    ("mossbyte", "20:23", "Should we use voice chat this time? Typing during the fight did not work at all."),
    ("quillfox", "20:25", "Yes, voice in the Raid channel. Push to talk please, nobody wants to hear your keyboard."),
    ("Bramwell", "20:28", "I am in. Also I can craft potions for everyone if you send me the herbs before Friday."),
    ("Sara Novak", "20:31", "Great, I will send you mine tomorrow."),
    ("quillfox", "20:40", "Loot rule as always: need before greed, and the healer gets the first pick on the staff if it drops."),
    ("mossbyte", "20:44", "fair"),
    ("nightjar", "20:52", "I will stream it on my channel if nobody minds."),
]

FLIGHT = [
    ("day", "Today"),
    ("in", "9:02", "Booked my flight for mom's birthday weekend!"),
    ("in", "9:02", "It is KL 1394 on Friday, it lands at 14:25. Can someone pick me up at the airport or should I take the train?"),
    ("out", "9:10", "Yay!! I can pick you up, I am off on Friday afternoon anyway. Leo has the car that day but I will take it back from him"),
    ("in", "9:12", "You are the best. Do I need to bring anything? I was thinking of buying the cake in the bakery near my place, the one with the hazelnut layers that mom loved last year"),
    ("out", "9:15", "She would love that, but it has to survive the flight. Last time the box got squashed in the overhead bin and you were sad for a whole day"),
    ("in", "9:16", "Haha true, it looked like a pancake. Ok, then you choose one there and I pay half"),
    ("out", "9:20", "Deal. I will ask Ruth if her sister still bakes, she made an amazing chocolate one for the wedding and it was not even expensive"),
    ("in", "9:22", "Good idea. What about the present? Did you and Leo decide something or are we still on the list of twenty ideas?"),
    ("out", "9:30", "We were thinking of a weekend in the mountains, the cabin with the hot tub she keeps talking about. It is not cheap, but if we split it in three it is fine, I will send you the link tonight"),
    ("in", "9:40", "Also, is the guest room free or should I book a hotel? I don't want to be in the way, I know Leo works from home now"),
    ("out", "9:41", "The guest room is yours, Leo moved his bike stuff to the garage last week, so you even have space for your bag now", {"reaction": "😂"}),
    ("in", "10:15", "Great. And dinner on Saturday is at the Italian place? Mom said something about a reservation but she was not sure"),
    ("out", "10:17", "Yes, she chose it herself. 7:30, I already made the reservation for six people, so do not plan anything else that evening"),
]

BIRTHDAY = [
    ("day", "Today"),
    ("in", "18:02", "Hola! Ya tengo todo listo para el cumple de la Pili el sábado, por fin"),
    ("in", "18:03", "Reservé en La Higuera, calle Olmedo 214, a las 21:30, a nombre de Solano. Somos diez, contando a los que vienen de Valparaíso"),
    ("out", "18:10", "Qué bueno!! Gracias por organizar todo, yo no habría tenido tiempo esta semana con el cierre de mes en la oficina"),
    ("in", "18:11", "De nada jaja, alguien tenía que hacerlo. ¿Tú te encargas de la torta? La Pili ama la de frambuesa de la pastelería de la esquina de su casa"),
    ("out", "18:15", "Sí, yo la encargo mañana temprano. ¿Le ponemos velas o se enoja? Mira que el año pasado casi no sopla"),
    ("in", "18:16", "Velas sí, pero no la edad, que está sensible con los 40 jajaja. Una vela grande y listo"),
    ("in", "18:25", "Otra cosa, ¿quién la lleva al restorán? Ella cree que vamos a ir a comer sushi con el Marco y nada más"),
    ("out", "18:27", "La puede llevar el Marco, él vive cerca y tiene auto. Le digo que la entretenga un rato antes para que no sospeche nada"),
    ("in", "18:28", "Perfecto, le escribo ahora. Ojalá no se le escape nada, es pésimo para los secretos"),
    ("in", "18:40", "Para el regalo pensé en juntar plata para el curso de cerámica que quería hace tiempo, el de los sábados en la mañana"),
    ("out", "18:42", "Me encanta esa idea, siempre habla de ese curso. ¿Cuánto ponemos cada uno más o menos?"),
    ("in", "18:43", "Como 12 lucas, y el resto lo pongo yo porque igual le quiero comprar unas herramientas para el torno"),
    ("out", "18:45", "Dale, te transfiero hoy en la noche cuando llegue a la casa, ahora estoy en el metro con mala señal"),
    ("in", "18:50", "Gracias! Y no le digas nada al Tito, que se le escapa todo y después la Pili me reta a mí"),
    ("out", "18:51", "Jajaja tranqui, no le digo nada a nadie, ni siquiera a mi mamá", {"reaction": "🙏"}),
]

MOVING = [
    ("day", "Today"),
    ("in", "10:02", "Ya confirmé el camión para la mudanza del sábado, me costó pero encontré uno con dos peonetas"),
    ("in", "10:03", "Llega a las 8:30 a tu depa. El chofer se llama Rubén y la patente es KTRV-52, por si el conserje pregunta"),
    ("out", "10:10", "Genial, gracias! Le aviso al conserje mañana, porque el de la noche no anota nada y después no deja entrar a nadie"),
    ("in", "10:11", "¿Ya tienes las cajas? Puedo pasar a dejarte unas del supermercado, las de plátanos son las mejores para los libros"),
    ("out", "10:14", "Tengo como veinte, pero me faltan para los libros y para la loza. La loza me da miedo, la de mi abuela es muy delicada"),
    ("in", "10:15", "Te llevo diez más el jueves en la tarde, y papel de diario para envolver los platos"),
    ("out", "10:16", "Eres un sol, te debo una grande"),
    ("in", "10:30", "¿Qué hacemos con el sillón grande? No sé si cabe en el ascensor del depa nuevo, se ve bien chico en las fotos"),
    ("out", "10:33", "Lo medí ayer, no cabe ni parado. Hay que subirlo por la escalera, son tres pisos y la escalera tiene una curva fea"),
    ("in", "10:34", "Uf, entonces necesitamos dos personas más por lo menos, los peonetas solos no van a poder"),
    ("out", "10:36", "Le pregunto al Tito y al primo de la Vale, los dos van al gimnasio así que no tienen excusa"),
    ("in", "10:37", "Jajaja ok, y yo les llevo almuerzo para que no reclamen, unas empanadas de pino o algo así"),
    ("in", "11:05", "Otra cosa, ¿tienes el contrato del depa nuevo a mano? La aseguradora me pide la dirección exacta para el seguro de la mudanza"),
    ("out", "11:07", "Sí, es Avenida Los Leones 1880, depto 34. Dime si te piden algo más, tengo todo escaneado en el correo"),
    ("in", "11:08", "Gracias, ya la mandé. Me dijeron que la póliza llega mañana"),
]

SWIM = [
    ("day", "Today 7:58 AM"),
    ("in", "7:58", "Morning! Reminder that the swim meet is this Saturday, the kids are so ready"),
    ("in", "7:59", "It is at the Corwin Aquatic Center this time, not the school pool. Warmup starts at 7:15 and the kids swim in lane 5"),
    ("out", "8:05", "Thanks! Leo is so excited, he has been practicing his flip turns all week in the bathtub, which was a mess"),
    ("in", "8:06", "Haha Mia too, she made me watch old Olympic finals with her every night this week"),
    ("in", "8:10", "Can you drive on the way there? I can bring them back in the afternoon, I have a work call in the morning that I cannot move"),
    ("out", "8:12", "Sure, I will pick up Mia at 6:30 then. Tell her to have breakfast before, the snack bar does not open until 9"),
    ("in", "8:13", "Perfect. She needs her goggles and the team cap, and I always forget the cap, so if you see it in her bag, great"),
    ("out", "8:15", "I will bring a spare one just in case, Leo has three of them somehow and I never bought a single one"),
    ("in", "8:30", "Also the coach asked for parents to help with the timing at the end of the lanes, it is easy but they need eight people"),
    ("out", "8:33", "I can do it in the morning, but I have to leave by 1 because of my sister's birthday lunch"),
    ("in", "8:34", "That works, the finals are after lunch anyway and the other parents can do that part"),
    ("in", "8:50", "Should we bring food? The snack bar there is expensive and it closes early, last year we ended up eating chips"),
    ("out", "8:52", "I will pack sandwiches and fruit for both, and some of those cereal bars they like"),
    ("in", "8:53", "Amazing, I owe you one. Next meet is on me, food and driving"),
    ("out", "8:55", "You can buy me a coffee at the meet and we are even"),
]


SLACK_OFFSITE = ("slack", "#offsite-2026 - Brightloom - Slack", ["# offsite-2026", "Planning for the November offsite"], "Message #offsite-2026")
SLACK_RELEASE = ("slack", "#release-train - Kestrel Labs - Slack", ["# release-train", "Release 7.4, deploy Friday"], "Message #release-train")
TEAMS_OPS = ("teams", "Chat | Ops Weekly | Microsoft Teams", ["Ops Weekly", "Chat   Files   Photos"], None)
DISCORD_RAID = ("discord", "#raid-night | Pixel Garden - Discord", ["# raid-night", "Saturday raids, be nice"], "Message #raid-night")


def header_chat(case_id, place, messages, before, next_text):
    app_key, title, top_lines, field_name = place
    return chat_case(case_id, app_key, title, top_lines, messages, before, next_text, field_name=field_name)


def far_and_near_cases():
    return [
        header_chat("conv-far-01", SLACK_OFFSITE, OFFSITE + [("Owen Marsh", "10:38 AM", "the lodge just called me to confirm the group, they want the booking reference. anyone have it handy?")], "Yes, it is ", "LL-58213"),
        header_chat("conv-far-02", SLACK_OFFSITE, OFFSITE + [("Mila Hart", "10:38 AM", "quick one, where does the shuttle leave from? my calendar invite has no location")], "It leaves from ", "Halden Station"),
        header_chat("conv-far-03", SLACK_RELEASE, RELEASE + [("Kofi Mensah", "10:41 AM", "quick check before I update the deploy ticket, which build did we pick as the candidate in the end? the dashboard still shows 4503")], "We picked build ", "4512"),
        header_chat("conv-far-04", SLACK_RELEASE, RELEASE + [("Ruth Calder", "10:41 AM", "hi all, who is the rollback owner this week? I need the name for the incident runbook")], "It is ", "Nadia"),
        header_chat("conv-far-05", TEAMS_OPS, OPS + [("Elias Brandt", "10:32 AM", "Sara, the gate guard asks which dock the Fernhill truck is coming to, he wants to keep it free")], "It comes to Dock ", "14"),
        header_chat("conv-far-06", TEAMS_OPS, OPS + [("Owen Marsh", "10:32 AM", "Accounting asks for the PO number of the packaging order, does anyone have it?")], "Yes, it is ", "PO-77310"),
        header_chat("conv-far-07", DISCORD_RAID, RAID + [("Bramwell", "21:10", "late to the party, what's the password for the private server?")], "It is ", "TIDEPOOL"),
        header_chat("conv-far-08", DISCORD_RAID, RAID + [("Bramwell", "21:10", "what time do we start on Saturday? I need to tell my family")], "We start at ", "21:30"),
        chat_case("conv-far-09", "whatsapp", "WhatsApp", ["Jonas Weber", "online"], FLIGHT + [("in", "10:40", "Ok one more thing, what time do I land again? I deleted the email by accident 🙈")], "You land at ", "14:25", with_sent=True),
        chat_case("conv-far-10", "whatsapp", "WhatsApp", ["Jonas Weber", "online"], FLIGHT + [("in", "10:40", "My airline app logged me out and I can't find the flight number, do you still have it?")], "Yes, it is ", "KL 1394"),
        chat_case("conv-far-11", "whatsapp", "WhatsApp", ["Beatriz Solano", "click here for contact info"], BIRTHDAY + [("in", "19:20", "Oye, ¿a qué hora era la reserva? Se me olvidó y el Marco me está preguntando")], "A las ", "21:30", with_sent=True),
        chat_case("conv-far-12", "whatsapp", "WhatsApp", ["Beatriz Solano", "click here for contact info"], BIRTHDAY + [("in", "19:20", "El Marco pregunta en qué número de Olmedo es, no lo encuentra en el mapa")], "Es en Olmedo ", "214"),
        chat_case("conv-far-13", "telegram", "Felipe Araya", ["Felipe Araya", "last seen recently"], MOVING + [("in", "11:30", "El conserje del edificio nuevo quiere el nombre del chofer, ¿cómo se llamaba?")], "Se llama ", "Rubén"),
        chat_case("conv-far-14", "telegram", "Felipe Araya", ["Felipe Araya", "last seen recently"], MOVING + [("in", "11:30", "Y la patente del camión, ¿te acuerdas? El conserje la necesita para abrir el portón")], "Es la ", "KTRV-52", with_sent=True),
        chat_case("conv-far-15", "messages", "Hana Okafor", ["To: Hana Okafor"], SWIM + [("in", "9:40", "The Petrov kids are joining us too, their dad asked me where the meet is. Do you remember the place?")], "Yes, it is at the ", "Corwin Aquatic Center", with_sent=True),
        chat_case("conv-far-16", "messages", "Hana Okafor", ["To: Hana Okafor"], SWIM + [("in", "9:40", "Quick question, what time does the warmup start? I want to tell the Petrov kids")], "Warmup starts at ", "7:15"),
        header_chat("conv-near-01", SLACK_OFFSITE, OFFSITE + [("Ruth Calder", "10:35 AM", "Got the confirmation email from Larchmont. Reference LL-58213 again, in case anyone needs it."), ("Owen Marsh", "10:38 AM", "the lodge just called me, they want the booking reference")], "It is ", "LL-58213"),
        header_chat("conv-near-02", SLACK_RELEASE, RELEASE + [("Nadia Petrov", "10:36 AM", "For the runbook: build 4512 is the candidate, and I am the rollback owner."), ("Kofi Mensah", "10:41 AM", "which build did we pick as the candidate in the end? the dashboard still shows 4503")], "We picked build ", "4512"),
        chat_case("conv-near-03", "whatsapp", "WhatsApp", ["Jonas Weber", "online"], FLIGHT + [("in", "10:38", "Checked in online, the flight still lands at 14:25 👍"), ("out", "10:39", "Great, I will be there"), ("in", "10:40", "Mom is asking when we get home, what should I tell her?")], "Tell her you land at ", "14:25"),
        chat_case("conv-near-04", "whatsapp", "WhatsApp", ["Beatriz Solano", "click here for contact info"], BIRTHDAY + [("in", "19:18", "Te confirmo que la reserva sigue a las 21:30 en La Higuera"), ("in", "19:20", "El Marco pregunta a qué hora tiene que llevar a la Pili")], "Tiene que llegar antes de las ", "21:30", with_sent=True),
        chat_case("conv-near-05", "telegram", "Felipe Araya", ["Felipe Araya", "last seen recently"], MOVING + [("in", "11:28", "Rubén me escribió, dice que llega puntual"), ("in", "11:30", "El conserje del edificio nuevo quiere el nombre del chofer")], "Se llama ", "Rubén", with_sent=True),
        chat_case("conv-near-06", "messages", "Hana Okafor", ["To: Hana Okafor"], SWIM + [("in", "9:38", "The coach says the parking at the Corwin Aquatic Center fills up fast, so come early"), ("in", "9:40", "The Petrov kids are joining us, their dad asked where the meet is")], "It is at the ", "Corwin Aquatic Center"),
    ]


def turns_cases():
    return [
        chat_case("conv-turns-01", "whatsapp", "WhatsApp", ["Mila Hart", "online"], [
            ("day", "Today"),
            ("in", "8:41", "morning! are we still on for the lake this weekend?"),
            ("out", "8:50", "Yes!! Leaving Saturday at 8 if that works for you"),
            ("in", "8:52", "8 is perfect, I will be ready"),
            ("in", "8:53", "I can bring the tent and the lantern, the big one with the dimmer"),
            ("out", "8:58", "Great, I will bring the stove and the coffee"),
            ("in", "9:01", "deal. do we need a permit for the fire pit?"),
            ("out", "9:04", "No, I checked, it is free in November"),
            ("in", "9:05", "amazing"),
            ("in", "9:31", "Ruth just asked me what you are bringing, she wants to make a list so we don't double up"),
        ], "Tell her I bring the ", "stove", with_sent=True),
        chat_case("conv-turns-02", "messages", "Owen Marsh", ["To: Owen Marsh"], [
            ("day", "Today 6:10 PM"),
            ("in", "18:10", "we're here!!"),
            ("out", "18:12", "same! I'm at the south gate, by the big fountain"),
            ("in", "18:13", "I'm at the north entrance, next to the food trucks"),
            ("in", "18:14", "the crowd here is crazy, can you come to me? I have the tickets and the blanket"),
        ], "Ok, walking to the ", "north entrance"),
        chat_case("conv-turns-03", "telegram", "Ignacia Pardo", ["Ignacia Pardo", "last seen recently"], [
            ("day", "Today"),
            ("in", "19:02", "¡Hola! ¿Al final vienes mañana a Santiago?"),
            ("out", "19:10", "¡Sí! Ya compré el pasaje"),
            ("in", "19:11", "¡Qué bueno! ¿A qué hora llegas?"),
            ("in", "19:11", "Mi tren llega a las 6:10, vengo de Rancagua"),
            ("out", "19:15", "El mío llega a las 6:40, salgo de Talca"),
            ("in", "19:16", "Ah, perfecto, entonces te espero en la estación"),
            ("in", "19:16", "Así nos vamos juntas en taxi a la casa"),
            ("out", "19:20", "¡Genial, gracias!"),
            ("in", "19:31", "¿Me confirmas tu hora para no quedarme media hora parada en el andén?"),
        ], "Sí, llego a las ", "6:40", with_sent=True),
        chat_case("conv-turns-04", "whatsapp", "WhatsApp", ["Kofi Mensah", "online"], [
            ("day", "Today"),
            ("out", "12:31", "made it! where are you sitting?"),
            ("in", "12:32", "inside, by the window. I'm at the counter now, they only take orders here"),
            ("in", "12:33", "I'm getting the lasagna"),
            ("out", "12:34", "ok! I'll have the risotto"),
            ("out", "12:34", "and a sparkling water please"),
            ("in", "12:36", "the guy is confused, what did you want again?"),
        ], "The ", "risotto"),
        chat_case("conv-turns-05", "messages", "Hana Okafor", ["To: Hana Okafor"], [
            ("day", "Today 4:02 PM"),
            ("in", "16:02", "made it to the hotel!"),
            ("in", "16:03", "the lobby is gorgeous, wait till you see it"),
            ("in", "16:05", "I'm in room 214, second floor, the one by the stairs"),
            ("out", "16:20", "just checked in too, I'm in 309, top floor with a view of the river"),
            ("in", "16:21", "so jealous"),
            ("in", "16:22", "come down to my room when you're ready, I have the snacks and the wine"),
            ("out", "16:24", "give me 10 min, I need a shower after that drive"),
            ("in", "16:25", "take your time"),
        ], "Ok, on my way to room ", "214", with_sent=True),
        chat_case("conv-turns-06", "whatsapp", "WhatsApp", ["Pilar Ortúzar", "online"], [
            ("day", "Today"),
            ("in", "20:02", "Holi, ¿qué llevamos el sábado al cumple de la Ana?"),
            ("out", "20:10", "Yo llevo el postre, todavía no sé cuál"),
            ("in", "20:11", "Rico"),
            ("in", "20:12", "Y yo las bebidas, ya compré dos cajas de jugo y unas cervezas"),
            ("in", "20:40", "La Ana me preguntó qué llevas tú, quiere armar la mesa"),
        ], "Dile que yo llevo ", "el postre"),
        chat_case("conv-turns-07", "telegram", "Teo Varga", ["Teo Varga", "last seen recently"], [
            ("day", "Today"),
            ("in", "17:41", "movie was so good"),
            ("out", "17:42", "right?? the ending!"),
            ("in", "17:44", "ok where did you park?"),
            ("in", "17:44", "I'm on level 3 near the pay machines"),
            ("out", "17:46", "I'm on level 5, right next to the elevator"),
            ("in", "17:47", "I have all the bags from the shop, they're heavy"),
            ("in", "17:48", "let's meet at your car, it's closer to the exit anyway"),
        ], "Ok, I am on level ", "5", with_sent=True),
        chat_case("conv-turns-08", "whatsapp", "WhatsApp", ["Rocío Fuentes", "online"], [
            ("day", "Today"),
            ("out", "15:02", "Hoy salgo del trabajo a las 6"),
            ("out", "15:02", "¿Nos vamos juntas?"),
            ("in", "15:10", "Yo salgo a las 7, tengo reunión hasta tarde"),
            ("in", "15:13", "¿Me esperas? Te invito un café mientras"),
        ], "Sí, te espero hasta las ", "7"),
        chat_case("conv-turns-09", "messages", "Jonas Weber", ["To: Jonas Weber"], [
            ("day", "Today 11:15 AM"),
            ("in", "11:15", "booked my flight for Lisbon!"),
            ("in", "11:16", "AF 1288, lands at 18:05"),
            ("out", "11:30", "yay!! mine is AF 1240, it lands at 17:40"),
            ("out", "11:30", "so I'll be there first"),
            ("in", "11:31", "nice, then I'll meet you at arrivals"),
            ("in", "11:45", "send me your flight number again so I can track it"),
        ], "It is AF ", "1240", with_sent=True),
        chat_case("conv-turns-10", "whatsapp", "WhatsApp", ["Andrea Vial", "online"], [
            ("day", "Today"),
            ("in", "13:02", "¡Ya firmamos el contrato!!"),
            ("out", "13:05", "¡Por fin! Yo me quedo con la pieza del balcón, me gusta la luz de la mañana"),
            ("in", "13:06", "Perfecto"),
            ("in", "13:06", "Entonces yo tomo la grande, la que da a la calle"),
            ("in", "13:30", "Mi mamá pregunta cuál es la tuya, quiere mandarte cortinas"),
        ], "Dile que la mía es la ", "del balcón"),
        chat_case("conv-turns-11", "messages", "Mila Hart", ["To: Mila Hart"], [
            ("day", "Today 9:02 AM"),
            ("in", "9:02", "I booked the 9am yoga for Saturday"),
            ("out", "9:10", "I got the 6pm one, the morning was full"),
            ("in", "9:11", "nice, that teacher is great"),
            ("in", "9:12", "wanna grab a smoothie right after your class?"),
        ], "Sure, my class is at ", "6pm", with_sent=True),
        chat_case("conv-turns-12", "telegram", "Matías Lagos", ["Matías Lagos", "last seen recently"], [
            ("day", "Today"),
            ("out", "10:02", "Llegué al registro civil, me tocó el número 57"),
            ("in", "10:03", "Yo estoy en la otra sala"),
            ("in", "10:03", "A mí me tocó el 42"),
            ("in", "10:06", "Uf, van en el 30"),
            ("in", "10:20", "¿Cuál era tu número? Te aviso si lo llaman acá también"),
        ], "El mío es el ", "57"),
        chat_case("conv-turns-13", "whatsapp", "WhatsApp", ["Ruth Calder", "online"], [
            ("day", "Yesterday"),
            ("out", "17:30", "I'll send you the slides tonight"),
            ("in", "17:32", "great, and I'll send the budget sheet tomorrow morning"),
            ("in", "17:33", "then we can merge both into one deck for Friday"),
            ("out", "17:35", "sounds good"),
            ("day", "Today"),
            ("in", "10:12", "morning! did you send what you promised? I can't find it in my inbox"),
            ("out", "10:20", "oh no"),
        ], "Sorry, I forgot! I will send the ", "slides", with_sent=True),
        chat_case("conv-turns-14", "messages", "Elena Brandt", ["To: Elena Brandt"], [
            ("day", "Today 2:10 PM"),
            ("in", "14:10", "ordering the team jerseys now"),
            ("out", "14:15", "S for me please, the M was way too big last year"),
            ("in", "14:16", "noted"),
            ("in", "14:16", "I'm an M as always"),
            ("in", "14:30", "wait, the form got reset. what size did you say you are?"),
        ], "I am an ", "S"),
    ]


def answer_cases():
    return [
        chat_case("conv-answer-01", "whatsapp", "WhatsApp", ["Diego Fuentes", "online"], [
            ("day", "Today"),
            ("in", "8:31", "morning! standup is in room B12 as usual today"),
            ("out", "8:40", "morning! ok, see you there"),
            ("in", "8:41", "bring your laptop, we will look at the sprint board"),
            ("out", "8:45", "will do"),
            ("in", "9:02", "Change of plan, we are in room C4 now because the projector is broken and Kofi joins remotely, so we need the big screen. Can you tell the others? Some of them will still go to B12"),
        ], "Sure, see you in ", "C4", with_sent=True),
        chat_case("conv-answer-02", "messages", "Hana Okafor", ["To: Hana Okafor"], [
            ("day", "Today 7:40 AM"),
            ("in", "7:40", "morning! pickup is at 2:45 like every Wednesday, right?"),
            ("out", "7:52", "yes, I'll be there"),
            ("in", "7:53", "great, Mia has her piano class after"),
            ("in", "11:05", "Today pickup is at 3:15 because of the teacher meeting, please do not come early, the gate stays locked until then, so the usual 2:45 does not work today"),
        ], "Got it, I will be there at ", "3:15"),
        chat_case("conv-answer-03", "telegram", "Ignacia Pardo", ["Ignacia Pardo", "last seen recently"], [
            ("day", "Today"),
            ("in", "16:02", "Ya estoy en el aeropuerto"),
            ("in", "16:03", "El vuelo sale por la puerta 12, ¿dónde estás?"),
            ("out", "16:10", "Pasando el control de seguridad, ¡hay una fila enorme!"),
            ("in", "16:11", "Ok, te espero"),
            ("in", "16:40", "Cambiaron la puerta, ahora es la 27, lo anunciaron hace un rato pero no por altavoz, así que si vienes desde el control no vayas a la puerta 12"),
        ], "Ok, voy a la puerta ", "27", with_sent=True),
        chat_case("conv-answer-04", "whatsapp", "WhatsApp", ["Teo Varga", "online"], [
            ("day", "Today"),
            ("out", "14:02", "hey, I'm at the studio, what's the wifi password?"),
            ("in", "14:05", "the guest network is STUDIO-GUEST but it's super slow"),
            ("out", "14:06", "ok, is there another one?"),
            ("in", "14:08", "The main network password is MAPLE-ROOM-88, all caps with the dashes, and if it does not work restart the router behind the plants. Forget STUDIO-GUEST, it drops every five minutes"),
        ], "Thanks! I will write it on the whiteboard: ", "MAPLE-ROOM-88"),
        chat_case("conv-answer-05", "messages", "Owen Marsh", ["To: Owen Marsh"], [
            ("day", "Today 8:12 AM"),
            ("in", "8:12", "reminder: the vet appointment for Pepper is Monday at 10"),
            ("out", "8:20", "thanks, I'll take her"),
            ("in", "8:21", "she'll hate it haha"),
            ("in", "12:40", "The vet moved Pepper to Tuesday at 11:30, the doctor is sick, and they asked us to bring the vaccine card and her food diary, so it is not Monday at 10 anymore"),
        ], "Ok, so Tuesday at ", "11:30", with_sent=True),
        chat_case("conv-answer-06", "whatsapp", "WhatsApp", ["Rocío Fuentes", "online"], [
            ("day", "Today"),
            ("in", "9:10", "¿Vas a la junta de vecinos hoy?"),
            ("in", "9:10", "Es en la sala 3 a las 8"),
            ("out", "9:15", "Sí, voy"),
            ("in", "18:02", "Al final la junta es en el quincho del edificio B, porque están arreglando el techo, y van a hablar del ascensor y de las cuotas, así que no vayan a la sala 3"),
        ], "Ok, nos vemos en el ", "quincho"),
        chat_case("conv-answer-07", "telegram", "Nadia Petrov", ["Nadia Petrov", "last seen recently"], [
            ("day", "Today"),
            ("in", "9:02", "the call tomorrow is with Ellis from procurement"),
            ("out", "9:10", "ok, I'll prepare the numbers"),
            ("in", "9:11", "thanks, keep it short, he hates long decks"),
            ("in", "15:30", "Small change, the call is with Priscilla Novo now, she will want to see the pricing sheet and the timeline for phase two, and please do not send anything to Ellis"),
        ], "Got it, I will send the pricing sheet to ", "Priscilla", with_sent=True),
        chat_case("conv-answer-08", "messages", "Elena Brandt", ["To: Elena Brandt"], [
            ("day", "Today 10:02 AM"),
            ("in", "10:02", "dinner tonight is at 8 at Nonna's, I booked it"),
            ("out", "10:10", "yay, can't wait"),
            ("in", "16:45", "Moved our table to 7:45 at Bistro Varela instead, Nonna's had a kitchen problem and Varela gives the table away after ten minutes, so do not go to Nonna's at 8"),
        ], "Perfect, see you at Bistro Varela at ", "7:45"),
        chat_case("conv-answer-09", "whatsapp", "WhatsApp", ["Andrea Vial", "online"], [
            ("day", "Today"),
            ("in", "11:02", "Te dejé las llaves con el conserje"),
            ("in", "11:02", "El código del portón es 1188, por si llegas tarde"),
            ("out", "11:10", "¡Gracias!"),
            ("in", "19:30", "El código nuevo del portón es 4632, lo cambiaron hoy después del robo, y el conserje dice que no se lo demos a nadie. El 1188 ya no funciona"),
        ], "Gracias por avisar, entonces el código es ", "4632", with_sent=True),
        chat_case("conv-answer-10", "telegram", "Felix Arden", ["Felix Arden", "last seen recently"], [
            ("day", "Today"),
            ("in", "9:15", "can you send me the sample package this week?"),
            ("in", "9:15", "to the Calder Street office, as usual"),
            ("out", "9:20", "sure, I'll ship it tomorrow"),
            ("in", "13:02", "Please send it to Unit 9 at the Brookfield warehouse instead, and write my name on the box so the guys at the door know it is for me. Not to Calder Street"),
        ], "Ok, I will send it to the ", "Brookfield warehouse"),
    ]


def older_texts(app_key, texts):
    """Returns earlier texts of `app_key`, each (days ago, window title, text)."""
    return [history_entry(text, app_key, title, NOW - days * DAY - 3 * 3600) for days, title, text in texts]


def live_cases():
    return [
        chat_case("conv-live-01", "whatsapp", "WhatsApp", ["Kofi Mensah", "online"], [
            ("day", "Today"),
            ("in", "11:02", "lunch today?"),
            ("out", "11:05", "yes please, I'm starving"),
            ("in", "11:06", "let's try the new place, Café Brisa on Dalton Street"),
            ("in", "11:06", "Owen says they have the best sandwiches in town"),
            ("out", "11:08", "sounds great, 1pm?"),
            ("in", "11:09", "perfect"),
        ], "See you at Café ", "Brisa", with_sent=True, history=older_texts("whatsapp", [
            (5, "Mila Hart", "See you at Café Lumo at 4!"),
            (4, "Ruth Calder", "Café Lumo again tomorrow? Same table by the window"),
            (3, "Mila Hart", "Running 5 min late, grab us a table at Lumo"),
            (2, "Jonas Weber", "Café Lumo has the best cinnamon buns, trust me"),
        ])),
        header_chat_with_history("conv-live-02", ("slack", "#search-team - Kestrel Labs - Slack", ["# search-team", "Search and indexing"], "Message #search-team"), [
            ("Kofi Mensah", "10:12 AM", "morning! the indexer rebuild finished overnight, no errors this time"),
            ("Ines Duarte", "10:20 AM", "nice. we still need a review on the search indexer PR before the release, it blocks the rest of the train"),
            ("Kofi Mensah", "10:22 AM", "I can't review it, I wrote half of it"),
            ("Ines Duarte", "10:25 AM", "Sara, can you take it?"),
        ], "Sure, I will review the ", "search indexer PR", older_texts("slack", [
            (5, "#billing - Kestrel Labs - Slack", "I will review the billing service PR after lunch"),
            (4, "#billing - Kestrel Labs - Slack", "billing service tests are green now, merging"),
            (3, "#billing - Kestrel Labs - Slack", "can someone review my billing PR? it is small"),
            (2, "#billing - Kestrel Labs - Slack", "I reviewed the billing service changes, two small comments"),
        ])),
        chat_case("conv-live-03", "whatsapp", "WhatsApp", ["Pilar Ortúzar", "online"], [
            ("day", "Today"),
            ("in", "17:02", "¿Nos juntamos hoy después de la pega?"),
            ("out", "17:05", "¡Sí! ¿Dónde?"),
            ("in", "17:06", "En la plaza Brasil, queda cerca de mi oficina nueva"),
            ("in", "17:06", "A las 7 y media, ¿te sirve?"),
            ("out", "17:10", "Perfecto"),
            ("in", "17:11", "Genial, llevo la guitarra"),
        ], "Ya, nos vemos en la plaza ", "Brasil", history=older_texts("whatsapp", [
            (6, "Rocío Fuentes", "Nos vemos en la plaza Ñuñoa a las 7"),
            (5, "Andrea Vial", "Llego a la plaza Ñuñoa en 10 minutos"),
            (3, "Rocío Fuentes", "¿En la plaza Ñuñoa como siempre?"),
            (2, "Andrea Vial", "Estoy en la plaza Ñuñoa, al lado de la pileta"),
        ])),
        chat_case("conv-live-04", "telegram", "Tess Holloway", ["Tess Holloway", "last seen recently"], [
            ("day", "Today"),
            ("in", "14:02", "I got the tickets for the jazz concert!"),
            ("in", "14:03", "It's on Saturday at 8, at Verna Hall"),
            ("out", "14:10", "Amazing, thank you!! How much do I owe you?"),
            ("in", "14:11", "Nothing, early birthday present 🎉"),
            ("in", "14:12", "Let's meet at the bar next door at 7"),
        ], "Best present ever. See you on ", "Saturday", with_sent=True, history=older_texts("telegram", [
            (6, "Marta Ruiz", "Pilates on Tuesday again?"),
            (5, "Marta Ruiz", "See you Tuesday at the studio!"),
            (3, "Ignacia Pardo", "Tuesday works for me"),
            (2, "Marta Ruiz", "Can't wait for Tuesday, see you there"),
        ])),
        chat_case("conv-live-05", "messages", "Jonas Weber", ["To: Jonas Weber"], [
            ("day", "Today 4:01 PM"),
            ("in", "16:01", "Did you see the email from the airline?"),
            ("in", "16:02", "They moved your flight tomorrow to 9:50, mine too"),
            ("out", "16:05", "Ugh, no, I didn't see it"),
            ("in", "16:06", "At least you can sleep in"),
            ("in", "16:06", "I'll pick you up at 8 then"),
        ], "True. So my flight is at ", "9:50", history=older_texts("messages", [
            (4, "Ruth Calder", "My flight is at 7:05 on Friday, so I'll leave super early"),
            (3, "Mila Hart", "the 7:05 flight, yes"),
            (2, "Ruth Calder", "boarding at 6:35 for the 7:05, wish me luck"),
        ])),
        chat_case("conv-live-06", "telegram", "Felipe Araya", ["Felipe Araya", "last seen recently"], [
            ("day", "Today"),
            ("in", "12:01", "Ojo que cambiaron el código del portón"),
            ("in", "12:01", "El nuevo es 2290, desde hoy"),
            ("out", "12:05", "¡Gracias por avisar!"),
            ("in", "12:06", "¿Te lo anoto o te acuerdas?"),
        ], "Me acuerdo, es el ", "2290", with_sent=True, history=older_texts("telegram", [
            (6, "Andrea Vial", "El código del edificio es 4471"),
            (5, "Pilar Ortúzar", "Marca 4471 y después la campana"),
            (3, "Andrea Vial", "Te dejé las llaves con el conserje, el código es 4471"),
        ])),
        header_chat_with_history("conv-live-07", ("slack", "#platform - Kestrel Labs - Slack", ["# platform", "Platform team"], "Message #platform"), [
            ("Ines Duarte", "9:02 AM", "heads up: tomorrow standup is at 10:15 because of the all-hands"),
            ("Nadia Petrov", "9:05 AM", "noted, thanks"),
            ("Kofi Mensah", "9:10 AM", "Sara, can you tell the two contractors? they are not in this channel"),
        ], "Sure, I will tell them it is at ", "10:15", older_texts("slack", [
            (5, "#platform - Kestrel Labs - Slack", "standup moved to 9:30 starting Monday"),
            (4, "#platform - Kestrel Labs - Slack", "see you all at the 9:30 standup"),
            (2, "#platform - Kestrel Labs - Slack", "9:30 standup again tomorrow, same room"),
        ])),
        chat_case("conv-live-08", "messages", "Elena Brandt", ["To: Elena Brandt"], [
            ("day", "Today 6:01 PM"),
            ("in", "18:01", "Family lunch on Sunday at my place!"),
            ("in", "18:02", "Mom is doing the roast and I am on desserts"),
            ("in", "18:02", "Can you bring a salad? The big one with pears and walnuts"),
            ("out", "18:10", "Of course!"),
            ("in", "18:11", "Great, and come at 1"),
        ], "Perfect, so I bring the ", "salad", history=older_texts("messages", [
            (6, "Ruth Calder", "I'll bring the lasagna on Sunday!"),
            (5, "Ruth Calder", "lasagna is in the oven, see you at 1"),
            (3, "Mila Hart", "I always end up bringing the lasagna haha"),
        ])),
        chat_case("conv-live-09", "whatsapp", "WhatsApp", ["Andrea Vial", "online"], [
            ("day", "Today"),
            ("in", "20:01", "Ya saqué la cuenta de la cabaña del fin de semana"),
            ("in", "20:02", "Son 23.500 cada uno, con la leña incluida"),
            ("out", "20:05", "Ok, te transfiero hoy"),
            ("in", "20:06", "Dale, gracias!"),
        ], "Listo, te transferí los ", "23.500", with_sent=True, history=older_texts("whatsapp", [
            (5, "Rocío Fuentes", "Te transfiero los 15.000 del asado"),
            (4, "Pilar Ortúzar", "Ya te pasé los 15.000"),
            (3, "Rocío Fuentes", "Son 15.000 cada uno, ¿cierto?"),
        ])),
        header_chat_with_history("conv-live-10", ("discord", "#raid-night | Pixel Garden - Discord", ["# raid-night", "Saturday raids, be nice"], "Message #raid-night"), [
            ("quillfox", "19:02", "Heads up: the raid moves to Sunday this week, the server maintenance is on Thursday night"),
            ("nightjar", "19:05", "Sunday works for me"),
            ("mossbyte", "19:07", "same"),
        ], "Ok, see you all on ", "Sunday", older_texts("discord", [
            (6, "#raid-night | Pixel Garden - Discord", "gg all, see you Thursday"),
            (4, "#raid-night | Pixel Garden - Discord", "Thursday raid again?"),
            (2, "#raid-night | Pixel Garden - Discord", "see you all on Thursday"),
        ])),
    ]


def header_chat_with_history(case_id, place, messages, before, next_text, history):
    app_key, title, top_lines, field_name = place
    return chat_case(case_id, app_key, title, top_lines, messages, before, next_text, history=history, field_name=field_name)


STORE_TEXTS = [
    (10, "Laura Méndez", "Hola Lau! ¿Cómo te fue en la entrevista? Cuéntame todo. Besos!"),
    (10, "Jonas Weber", "Hey Jonny, did you call mom yet? She asked about you twice. Cheers"),
    (9, "Kofi Mensah", "Morning Kof! Coffee at 10 in the usual spot?"),
    (9, "Rosa Vidal", "Hola tía! Gracias por el queque, estaba riquísimo. Un abrazo grande"),
    (8, "Run Club", "Hi team! Track at 7 tomorrow, bring water"),
    (8, "Laura Méndez", "Hola Lau! Mañana paso por tu casa a dejarte el taladro, como a las 6. Besos!"),
    (7, "Owen Marsh", "Hi Owen, the report is in the shared folder. Best, Sara"),
    (7, "Jonas Weber", "Hey Jonny! I booked the train for Christmas, the 23rd at 10. Cheers"),
    (6, "Kofi Mensah", "Morning Kof, I left the charger on your desk"),
    (6, "Rosa Vidal", "Hola tía! El domingo no puedo ir, tengo turno. Un abrazo grande"),
    (5, "Laura Méndez", "Lau, te mando las fotos del viaje en la noche, están lindas. Besos!"),
    (5, "Run Club", "Hi team, rain tomorrow, we move the run to Thursday"),
    (4, "Jonas Weber", "Jonny, can you send me the photos from the wedding? Cheers"),
    (4, "Owen Marsh", "Hi Owen, I can join the call at 3. Best, Sara"),
    (3, "Rosa Vidal", "Hola tía! ¿Cómo sigue el tío de la rodilla? Un abrazo grande"),
    (3, "Kofi Mensah", "Kof, you saved me today, thanks!"),
    (2, "Laura Méndez", "Hola Lau! ¿Vamos al mercado el domingo temprano? Besos!"),
    (2, "Run Club", "Hi team! Great run today, see you next week"),
    (1, "Jonas Weber", "Hey Jonny, happy Friday! Any plans? Cheers"),
    (1, "Kofi Mensah", "Morning Kof! Did you see the new menu at the canteen?"),
]


def store_entries():
    return [history_entry(text, "whatsapp", title, NOW - days * DAY - (3 + 4 * (index % 2)) * 3600) for index, (days, title, text) in enumerate(STORE_TEXTS)]


def header_cases():
    def contact_case(case_id, contact, messages, before, next_text):
        return chat_case(case_id, "whatsapp", "WhatsApp", [contact], messages, before, next_text, writer=STORE_NAME)

    return [
        contact_case("conv-header-01", "Laura Méndez", [
            ("day", "Today"),
            ("in", "19:40", "¿Estás en tu casa? Te quería pedir algo"),
            ("in", "19:41", "Es por lo del viaje del próximo mes"),
        ], "Hola ", "Lau"),
        contact_case("conv-header-02", "Laura Méndez", [
            ("day", "Yesterday"),
            ("in", "21:10", "¿Te gustó el libro que te presté?"),
            ("out", "21:30", "¡Me encantó! Lo terminé en dos días"),
            ("day", "Today"),
            ("in", "9:12", "¿Me lo puedes devolver esta semana? Se lo prometí a mi hermana"),
        ], "Claro, mañana te lo dejo en la portería. ", "Besos!"),
        contact_case("conv-header-03", "Jonas Weber", [
            ("day", "Today"),
            ("in", "22:41", "you up? need your help with something"),
            ("in", "22:41", "it's about mom's present"),
        ], "Hey ", "Jonny"),
        contact_case("conv-header-04", "Rosa Vidal", [
            ("day", "Today"),
            ("in", "10:02", "Hola mijita, ¿vienes el domingo a almorzar?"),
            ("in", "10:03", "Voy a hacer pastel de choclo"),
        ], "Hola ", "tía"),
        contact_case("conv-header-05", "Kofi Mensah", [
            ("day", "Today"),
            ("in", "8:31", "coffee later? I need to vent about the release"),
        ], "Morning ", "Kof"),
        contact_case("conv-header-06", "Jonas Weber", [
            ("day", "Today"),
            ("in", "18:02", "did you talk to mom about Christmas?"),
            ("in", "18:03", "she keeps asking me if we come on the 23rd"),
        ], "Not yet, I will call her tonight and tell her about the train. ", "Cheers"),
    ]


def email_cases():
    return [
        email_case("conv-email-01", "Workshop logistics for March", "Ruth Calder", "ruth.calder@example.com", "Wed, Nov 4, 2026 at 9:12 AM", [
            "Hi Sara,",
            "I finally heard back from the facilities team about the workshop, sorry it took so long. They had a few questions about the number of people and the catering, so I answered those first.",
            "Short version: the large training room on the ground floor is booked for the whole first half of March, so we cannot use it. The Juniper Room on the 3rd floor is free, it holds 24 people, and it has the big screen and the whiteboards we asked for.",
            "The only date when the Juniper Room is free for a full day is March 12. The other days have a standing meeting from 10 to 11, which would cut the morning session in half.",
            "For catering, they can do coffee and pastries at 9, lunch at 12:30, and a snack in the afternoon. They need the final head count and any allergies two weeks before.",
            "Can you confirm the date so I can send the invite to the team leads? I would also like to share the agenda by the end of next week.",
            "Thanks,\nRuth",
        ], "Hi Ruth,\n\nThanks for sorting this out. Let us go with March ", "12", has_screen=True),
        email_case("conv-email-02", "Reserva del viaje a Valdivia", "Laura Méndez", "laura.mendez@example.com", "Tue, Nov 3, 2026 at 6:40 PM", [
            "Hola Sara:",
            "Te escribo con el resumen del viaje, para que lo tengas todo en un solo correo y no tengas que buscar en el chat del grupo.",
            "Los pasajes de bus son para el viernes 20 a las 22:15, salimos del terminal Alameda y llegamos a Valdivia como a las 8 de la mañana. Cada uno lleva su propio pasaje en el teléfono.",
            "Para la cabaña hice la reserva en Cabañas Río Cruces, para cuatro personas, del 21 al 23. El código de reserva es HX-40917, y lo piden cuando llegas, junto con el carnet.",
            "El pago lo hice yo con la tarjeta, así que después me transfieren lo que corresponde. Son 48.000 por persona con la leña incluida.",
            "Si alguien quiere el kayak el sábado, hay que avisar antes del miércoles, porque tienen pocos.",
            "Un abrazo,\nLaura",
        ], "Hola Laura:\n\nMuchas gracias por organizar todo. Ya confirmé la reserva con el código ", "HX-40917", has_screen=False),
        email_case("conv-email-03", "Invoice for the October work", "Elias Brandt", "elias.brandt@example.com", "Mon, Nov 2, 2026 at 4:05 PM", [
            "Hi Sara,",
            "I hope you had a good weekend. Attached is the invoice for the work we did in October on the warehouse labels and the new picking screens.",
            "As agreed in the contract, the hours for the extra report are on a separate line, and the travel costs for the two site visits are included at cost, with the receipts in the second attachment.",
            "The invoice number is INV-20931 and the total is 4,860 EUR. The payment term is 30 days, so the due date is December 2. Please use the invoice number as the reference for the bank transfer, otherwise our accounting team cannot match it.",
            "If your finance team needs a different format or a purchase order number on the invoice, let me know and I will send a new version.",
            "Best regards,\nElias",
        ], "Hi Elias,\n\nThank you. I forwarded invoice ", "INV-20931", has_screen=True),
        email_case("conv-email-04", "Field trip on Friday", "Hana Okafor", "hana.okafor@example.com", "Tue, Nov 3, 2026 at 8:20 PM", [
            "Hi Sara,",
            "Just a few details for the field trip to the science museum on Friday, since the school letter was a bit confusing.",
            "The kids should wear the school t-shirt and bring a packed lunch, a water bottle and a jacket, because part of the visit is in the garden. Phones stay at home.",
            "The bus leaves from the Pine Street entrance at 7:20, not from the main gate, because the main gate is closed for the construction work. The teachers take attendance at 7:10, so please be there a few minutes before that.",
            "We come back at around 3:30 to the same entrance. If you cannot pick up Leo at that time, he can stay in the after school club until 5.",
            "Thanks for helping with the permission slips!\nHana",
        ], "Hi Hana,\n\nThanks for the details. I will make sure that Leo is at the ", "Pine Street entrance", has_screen=False),
        email_case("conv-email-05", "Entrega de los muebles", "Gonzalo Riquelme", "gonzalo.riquelme@example.com", "Wed, Nov 4, 2026 at 11:30 AM", [
            "Hola Sara:",
            "Te escribo para confirmar la entrega de los muebles que compraste en la tienda el sábado pasado: el sofá de tres cuerpos, la mesa de centro y las dos sillas de la terraza.",
            "Como el sofá viene de la bodega de Quilicura, la entrega no puede ser esta semana. La primera fecha que tenemos es el jueves 12, entre las 9 y las 11 de la mañana.",
            "El transporte lo hace Transportes Cordillera. El chofer te va a llamar media hora antes de llegar, y necesita que alguien firme la guía de despacho. El número de seguimiento es TC-88214.",
            "Si el jueves no te sirve, me puedes responder este correo y buscamos otra fecha la semana siguiente.",
            "Saludos cordiales,\nGonzalo Riquelme\nServicio al Cliente",
        ], "Hola Gonzalo:\n\nGracias por la información. El jueves ", "12", has_screen=True),
        email_case("conv-email-06", "New contact for the Harlow account", "Nadia Petrov", "nadia.petrov@example.com", "Wed, Nov 4, 2026 at 2:48 PM", [
            "Hi Sara,",
            "Quick update on the Harlow account before the quarterly review next week. As you may have heard, there were some changes in their team after the merger in September.",
            "Marcus, who was our main contact for the last two years, moved to their London office and now works on a different product line. He asked me to thank you for all the help with the rollout.",
            "From now on, please contact Wendell Osei for everything about the account. He is the new account lead, and he already has the notes from the last three calls and the open support tickets.",
            "He prefers email for anything with numbers and a short call for the rest. His assistant books the calls, so please copy her on the invites.",
            "Thanks,\nNadia",
        ], "Hi Nadia,\n\nThanks for the update. I will reach out to ", "Wendell", has_screen=False),
        email_case("conv-email-07", "Your talk at the Northgate conference", "Owen Marsh", "owen.marsh@example.com", "Tue, Nov 3, 2026 at 10:05 AM", [
            "Hi Sara,",
            "Great news: the program committee accepted your talk about the support handoff, and the feedback from the reviewers was very positive. Congratulations!",
            "Here are the practical details. The conference is at the Northgate Convention Center on November 26 and 27. Your talk is on the first day, in Hall C at 14:20, and it has a slot of 30 minutes, with 5 minutes for questions.",
            "Please send your slides by November 20, so the technical team can test them on the room computer. If you want to use your own laptop, bring an HDMI adapter.",
            "Speakers get a free pass for both days and lunch in the speaker lounge on the second floor. The speaker dinner is on the evening of the 26th.",
            "Best,\nOwen",
        ], "Hi Owen,\n\nThank you, that is great news! I will send the slides before November ", "20", has_screen=False),
        email_case("conv-email-08", "Proveedor para la remodelación", "Beatriz Solano", "beatriz.solano@example.com", "Mon, Nov 2, 2026 at 5:15 PM", [
            "Hola Sara:",
            "Te cuento cómo quedó lo de la remodelación de la oficina, porque ayer se cerró todo con la administración del edificio.",
            "El presupuesto final quedó un poco más alto de lo que pensábamos, por el cambio del piso de la sala de reuniones, pero la gerencia lo aprobó igual.",
            "El proveedor nuevo para los materiales es Ferretería Los Alerces, en avenida Grecia. Ellos tienen todo en stock y nos hacen despacho gratis si el pedido es antes del 15. El contacto allá es la señora Marta.",
            "Las obras parten el lunes 16 y duran tres semanas. Durante ese tiempo vamos a trabajar desde la sala del segundo piso.",
            "Cualquier duda me avisas.\nSaludos,\nBeatriz",
        ], "Hola Beatriz:\n\nPerfecto, gracias por el resumen. Mañana llamo a Ferretería ", "Los Alerces", has_screen=True),
    ]


def build():
    cases = far_and_near_cases() + turns_cases() + answer_cases() + live_cases() + header_cases() + email_cases()
    with CASES_PATH.open("w", encoding="utf-8") as file:
        for case in cases:
            file.write(json.dumps(case, ensure_ascii=False) + "\n")
    with STORE_PATH.open("w", encoding="utf-8") as file:
        for entry in store_entries():
            file.write(json.dumps(entry, ensure_ascii=False) + "\n")
    print(f"wrote {len(cases)} cases to {CASES_PATH.name} and {len(store_entries())} texts to writers/{STORE_PATH.name}")


# The check: a port of the line pick of typealike_core/src/screen.rs and the token estimate of
# typealike_core/src/excerpt.rs, to measure where each fact sits in the screen budget.

CASE_KEYS = {"id", "app", "app_id", "window_title", "domain", "field_caption", "field_name", "field_placeholder", "field_help", "before", "before_enter", "after", "clipboard", "screen", "has_unreadable_text", "writer", "left_out", "now", "history", "words", "next", "typo", "fixed", "leaks"}
ENTRY_KEYS = {"text", "app_id", "domain", "written_at", "reply_to", "app", "window_title"}
PLAIN_PUNCTUATION = set(".,;:!?'’\"“”()-…")
HALVES_OF_DAY = {"am", "pm"}
DAY_WORDS = {"today", "yesterday", "hoy", "ayer"}
DAY_TIME_JOINS = {"at", "a", "las", "la"}
SPANISH_WORDS = {"el", "la", "las", "los", "es", "que", "se", "sí", "dile", "nos", "te", "hola", "gracias", "voy", "llamo", "claro", "listo", "jueves", "tía"}


def estimated_tokens(text):
    thirds = sum({1: 1, 4: 6}.get(len(character.encode()), 3) for character in text)
    return -(-thirds // 3)


def one_line(text):
    return " ".join(text.split())


def is_time(word):
    hours, separator, minutes = word.partition(":")
    digits = hours + minutes
    return bool(separator) and 1 <= len(hours) <= 2 and len(minutes) == 2 and digits.isascii() and digits.isdigit()


def is_separator(word):
    return not any(character.isalnum() for character in word)


def header_name(text):
    words = text.split(" ")
    if words and words[-1].lower() in HALVES_OF_DAY:
        words.pop()
    if not words or not is_time(words[-1]):
        return None
    words.pop()
    days = [index for index, word in enumerate(words) if word.lower() in DAY_WORDS]
    if days:
        day = days[-1]
        if all(word.lower() in DAY_TIME_JOINS for word in words[day + 1:]) and (day == 0 or is_separator(words[day - 1])):
            words = words[:day]
    if words and is_separator(words[-1]):
        words.pop()
    return " ".join(words) if len(words) <= 2 else None


def is_noise(text):
    signs = sum(1 for character in text if not character.isalnum() and not character.isspace() and character not in PLAIN_PUNCTUATION)
    has_digit_in_word = any(first.islower() and middle.isnumeric() and last.islower() for first, middle, last in zip(text, text[1:], text[2:]))
    return signs * 100 > len(text) * 20 or has_digit_in_word


def is_inside(frame, field):
    middle_x = (frame[0] + frame[2]) // 2
    middle_y = (frame[1] + frame[3]) // 2
    return field[0] <= middle_x < field[2] and field[1] <= middle_y < field[3]


def ranked_candidates(case):
    """Returns the screen lines that the prompt can keep, nearest to the caret first."""
    screen = case["screen"]
    field = screen["field"]
    caret = screen["caret"] or field
    left, right = field[0], field[2]
    for line in screen["lines"]:
        frame = line["frame"]
        if frame[0] < field[2] and field[0] < frame[2]:
            left, right = min(left, frame[0]), max(right, frame[2])
    field_text = case["before"] + case["after"]
    field_lines = {one_line(line) for line in field_text.split("\n")}
    field_joined = one_line(field_text)
    seen = set()
    candidates = []
    for line in screen["lines"]:
        frame = line["frame"]
        key = (line["text"], tuple(frame))
        if key in seen or not (frame[0] < right and left < frame[2]) or is_inside(frame, field):
            continue
        seen.add(key)
        text = one_line(line["text"])
        name = header_name(text)
        text = name if name is not None else text
        repeats_field = text in field_lines or (len(text) >= 20 and text in field_joined)
        if not text or is_noise(text) or repeats_field:
            continue
        gap = caret[1] - frame[3] if frame[3] <= caret[1] else frame[1] - caret[3] if caret[3] <= frame[1] else 0
        candidates.append((gap, frame[1], frame[0], text))
    candidates.sort()
    return [text for _, _, _, text in candidates]


def kept_texts(ranked, budget):
    left = budget
    kept = []
    for text in ranked:
        tokens = estimated_tokens(text)
        if tokens <= left:
            left -= tokens
            kept.append(text)
    return kept


def fact_word(case):
    return case["next"].split()[0].rstrip(".,!?:;")


def holds_fact(case, text):
    return re.search(rf"(?<![\w:]){re.escape(fact_word(case))}(?![\w:])", text) is not None


def fact_distance(case, ranked):
    """Returns the tokens of the lines nearer than the nearest line with the first word of next."""
    distance = 0
    for text in ranked:
        if holds_fact(case, text):
            return distance
        distance += estimated_tokens(text)
    return None


def last_bubble_rows(screen):
    """Returns the text rows of the last bubble above the field, without its time."""
    field_top = screen["field"][1]
    texts = [line for line in screen["lines"] if line["frame"][3] <= field_top and header_name(one_line(line["text"])) != "" and "✓" not in line["text"]]
    rows = [texts[-1]]
    for line in reversed(texts[:-1]):
        if line["frame"][0] != rows[0]["frame"][0] or rows[0]["frame"][1] - line["frame"][1] != ROW_PITCH:
            break
        rows.insert(0, line)
    return [row["text"] for row in rows]


def check():
    groups = {}
    ids = set()
    problems = []
    for number, raw in enumerate(CASES_PATH.read_text(encoding="utf-8").splitlines(), 1):
        case = json.loads(raw)
        unknown = set(case) - CASE_KEYS
        unknown |= {key for entry in case["history"] for key in entry} - ENTRY_KEYS
        if unknown:
            problems.append(f"line {number}: unknown keys {sorted(unknown)}")
        if case["id"] in ids:
            problems.append(f"line {number}: duplicate id {case['id']}")
        ids.add(case["id"])
        if not case["before"] or not case["next"].strip():
            problems.append(f"{case['id']}: empty before or next")
        groups.setdefault(case["id"].split("-")[1], []).append(case)
    store_lines = STORE_PATH.read_text(encoding="utf-8").splitlines()
    for raw in store_lines:
        unknown = set(json.loads(raw)) - ENTRY_KEYS
        if unknown:
            problems.append(f"store: unknown keys {sorted(unknown)}")
    print(f"{len(ids)} cases, {len(store_lines)} store texts")
    for group, cases in groups.items():
        print_group(group, cases, problems)
    for problem in problems:
        print("PROBLEM:", problem)
    return not problems


def print_group(group, cases, problems):
    screen_tokens, quarter_tokens, distances = [], [], []
    with_history = sum(1 for case in cases if case["history"] or case.get("writer"))
    spanish = sum(1 for case in cases if SPANISH_WORDS & set(re.findall(r"\w+", (case["before"] + " " + case["next"]).lower())))
    for case in cases:
        if not case["screen"]:
            continue
        ranked = ranked_candidates(case)
        screen_tokens.append(sum(estimated_tokens(text) for text in ranked))
        quarter_tokens.append(sum(len(text) for text in ranked) // 4)
        if group not in ("far", "near"):
            continue
        distance = fact_distance(case, ranked)
        distances.append(distance)
        kept_now = any(holds_fact(case, text) for text in kept_texts(ranked, SCREEN_TOKENS))
        kept_wider = any(holds_fact(case, text) for text in kept_texts(ranked, WIDER_SCREEN_TOKENS))
        print(f"        {case['id']}: screen {screen_tokens[-1]} tokens, fact {distance} tokens from the caret")
        if group == "far" and (kept_now or not kept_wider or distance is None or distance < 320):
            problems.append(f"{case['id']}: fact kept at {SCREEN_TOKENS}: {kept_now}, at {WIDER_SCREEN_TOKENS}: {kept_wider}")
        if group == "near" and (distance is None or distance > 150):
            problems.append(f"{case['id']}: fact distance {distance}")
    line = f"{group:7} {len(cases):3} cases, {with_history:2} with history, {spanish:2} in Spanish"
    if screen_tokens:
        line += f", screen tokens median {statistics.median(screen_tokens):.0f} max {max(screen_tokens)} (chars/4: median {statistics.median(quarter_tokens):.0f} max {max(quarter_tokens)})"
    print(line)
    if distances:
        print(f"        fact distance from the caret (estimated tokens): {sorted(distances, key=lambda distance: -1 if distance is None else distance)}")
    if group == "answer":
        for case in cases:
            rows = last_bubble_rows(case["screen"])
            if not 3 <= len(rows) <= 4 or not holds_fact(case, rows[0]) or holds_fact(case, rows[-1]):
                problems.append(f"{case['id']}: last bubble rows {rows}")
    if group == "email":
        offsets = []
        for case in cases:
            start = re.search(rf"(?<![\w:]){re.escape(fact_word(case))}(?![\w:])", case["after"]).start()
            offsets.append((estimated_tokens(case["after"][:start]), estimated_tokens(case["after"])))
            if offsets[-1][0] <= AFTER_CARET_TOKENS or start // 4 <= AFTER_CARET_TOKENS:
                problems.append(f"{case['id']}: fact at {offsets[-1][0]} tokens into after")
        print(f"        fact offset and length of after (estimated tokens): {offsets}")


if __name__ == "__main__":
    if sys.argv[1:] == ["--check"]:
        sys.exit(0 if check() else 1)
    build()
