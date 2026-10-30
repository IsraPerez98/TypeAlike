//! Tells whether the app in front is a game, and whether it shows in full screen
//! ([`full_screen`](crate::full_screen)). A game is a program in the game list of Windows: the
//! programs that Game Bar and Game Mode know as games (`HKCU\System\GameConfigStore\Children`).
//! Windows adds a game to the list when the game starts, and the user can add a program in Game
//! Bar ("Remember this is a game"). The list is not a documented API. When it is missing, no app
//! counts as a game.
//!
//! The watch runs on a thread of its own with a message loop: a `WinEvent` hook tells it each change
//! of the foreground window, and a registry notification tells it each change of the game list.
//! Windows sends no event when a window goes into or out of full screen, so the watch also reads
//! the window in front every second.

use std::collections::HashSet;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use typealike_core::desktop::{FullScreenActivity, GameActivity};
use windows::Win32::Foundation::{
    E_UNEXPECTED, HANDLE, HWND, LPARAM, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT, WPARAM,
};
use windows::Win32::System::Threading::{CreateEventW, GetCurrentThreadId};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EVENT_SYSTEM_FOREGROUND, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx,
    PM_NOREMOVE, PM_REMOVE, PeekMessageW, PostThreadMessageW, QS_ALLINPUT, WINEVENT_OUTOFCONTEXT, WM_QUIT,
    WM_USER,
};
use windows::core::{Error, Owned, PCWSTR, Result, w};

use crate::mailbox::Mailbox;
use crate::registry::OpenKey;
use crate::{focus, full_screen};

const THREAD_NAME: &str = "front app watch";
/// The time between two reads of the window in front while no event comes.
const FULL_SCREEN_CHECK_INTERVAL: Duration = Duration::from_secs(1);
const GAME_LIST_KEY: PCWSTR = w!("System\\GameConfigStore\\Children");
const KIND_VALUE: PCWSTR = w!("Type");
const PROGRAM_VALUE: PCWSTR = w!("MatchedExeFullPath");
/// The kind of an entry of the game list that is a game. Other kinds mark programs that are not
/// games.
const GAME_KIND: u32 = 1;

/// What the watch tells about the app in front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrontApp {
    pub(crate) game: GameActivity,
    pub(crate) full_screen: FullScreenActivity,
}

/// The thread that watches the app in front. Drop stops the thread and waits for it.
#[derive(Debug)]
pub(crate) struct FrontAppWatch {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl FrontAppWatch {
    /// Starts the thread, which sends to `changes` whether the app in front is a game and whether
    /// it shows in full screen: one time at the start, and again after each change.
    pub(crate) fn start(changes: Mailbox<FrontApp>) -> Result<Self> {
        let (started, start) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || watch_front_app(&changes, &started))?;
        let thread_id = start
            .recv()
            .map_err(|_disconnected| Error::from(E_UNEXPECTED))??;
        Ok(Self {
            thread_id,
            thread: Some(thread),
        })
    }
}

impl Drop for FrontAppWatch {
    fn drop(&mut self) {
        // SAFETY: plain call. The thread made its message queue before it sent its id.
        if let Err(error) = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) } {
            tracing::warn!(%error, "failed to stop the front app watch");
            return;
        }
        if self.thread.take().is_some_and(|thread| thread.join().is_err()) {
            tracing::warn!("the front app watch panicked");
        }
    }
}

/// Sends the id of this thread to `started`, then watches the app in front until a `WM_QUIT`.
fn watch_front_app(changes: &Mailbox<FrontApp>, started: &mpsc::SyncSender<Result<u32>>) {
    make_message_queue();
    let hook = match watch_foreground() {
        Ok(hook) => hook,
        Err(error) => {
            // The receiver waits in `GameWatch::start`, which gets the error.
            let _ = started.send(Err(error));
            return;
        }
    };
    // SAFETY: the call takes no argument and has no precondition.
    if started.send(Ok(unsafe { GetCurrentThreadId() })).is_err() {
        return;
    }
    FrontAppWatcher::new(changes).run();
    drop(hook);
}

/// Makes the message queue of this thread, so that `PostThreadMessageW` can reach it at once.
fn make_message_queue() {
    let mut message = MSG::default();
    // SAFETY: `message` is a local the call fills in.
    // Returns whether a message waits, not an error.
    let _ = unsafe { PeekMessageW(&raw mut message, None, WM_USER, WM_USER, PM_NOREMOVE) };
}

/// Watches the changes of the foreground window: each change reaches `foreground_event_procedure`
/// through the message loop of this thread.
fn watch_foreground() -> Result<Owned<HWINEVENTHOOK>> {
    // SAFETY: plain call. The procedure lives for the whole program, and the events come through
    // the message loop of this thread.
    let hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(foreground_event_procedure),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    if hook.is_invalid() {
        return Err(Error::from_thread());
    }
    // SAFETY: SetWinEventHook returned a valid hook that nothing else owns.
    Ok(unsafe { Owned::new(hook) })
}

/// The loop reads the app in front after it runs the messages of each event, so the event needs no
/// work of its own.
extern "system" fn foreground_event_procedure(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _window: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time_ms: u32,
) {
}

/// What woke the loop of the watch.
enum Wake {
    /// The game list changed.
    ListChange,
    /// Messages wait, with the events of the foreground window.
    Messages,
    /// The time to read the window in front again came.
    FullScreenCheck,
    /// The wait failed.
    Failure,
}

struct FrontAppWatcher<'changes> {
    changes: &'changes Mailbox<FrontApp>,
    list_watch: Option<GameListWatch>,
    list: GameList,
    sent: Option<FrontApp>,
}

impl<'changes> FrontAppWatcher<'changes> {
    fn new(changes: &'changes Mailbox<FrontApp>) -> Self {
        let list_watch = GameListWatch::open()
            .inspect_err(|error| tracing::info!(%error, "found no game list"))
            .ok();
        let list = list_watch.as_ref().map(GameListWatch::read).unwrap_or_default();
        Self {
            changes,
            list_watch,
            list,
            sent: None,
        }
    }

    fn run(&mut self) {
        loop {
            self.send_front_app();
            match self.wait() {
                Wake::ListChange => self.read_list(),
                Wake::Messages if dispatch_messages() => {}
                Wake::Messages => return,
                Wake::FullScreenCheck => {}
                Wake::Failure => {
                    tracing::warn!(error = %Error::from_thread(), "the front app watch stopped");
                    return;
                }
            }
        }
    }

    /// Sends whether the app in front is a game and whether it shows in full screen, when this
    /// changed since the last send.
    fn send_front_app(&mut self) {
        let program = focus::foreground_program();
        let front_app = FrontApp {
            game: self.list.activity_of(program.as_deref()),
            full_screen: full_screen::read(),
        };
        if self.sent == Some(front_app) {
            return;
        }
        tracing::info!(game = ?front_app.game, full_screen = ?front_app.full_screen, program, "the app in front changed");
        self.sent = Some(front_app);
        self.changes.send(front_app);
    }

    fn wait(&self) -> Wake {
        let events: Vec<HANDLE> = self.list_watch.iter().map(GameListWatch::event).collect();
        let events = (!events.is_empty()).then_some(events.as_slice());
        let timeout_ms = u32::try_from(FULL_SCREEN_CHECK_INTERVAL.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: the events stay open for the call.
        let wake =
            unsafe { MsgWaitForMultipleObjectsEx(events, timeout_ms, QS_ALLINPUT, MWMO_INPUTAVAILABLE) };
        match wake {
            WAIT_FAILED => Wake::Failure,
            WAIT_TIMEOUT => Wake::FullScreenCheck,
            WAIT_OBJECT_0 if events.is_some() => Wake::ListChange,
            _ => Wake::Messages,
        }
    }

    fn read_list(&mut self) {
        let Some(list_watch) = &self.list_watch else {
            return;
        };
        if let Err(error) = list_watch.watch_next_change() {
            tracing::warn!(%error, "failed to watch the game list");
            self.list_watch = None;
            return;
        }
        self.list = list_watch.read();
    }
}

/// Runs the messages that wait, and with them the events of the foreground window. Returns `false`
/// after a `WM_QUIT`.
fn dispatch_messages() -> bool {
    let mut message = MSG::default();
    // SAFETY: `message` is a local the call fills in.
    while unsafe { PeekMessageW(&raw mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
        if message.message == WM_QUIT {
            return false;
        }
        // SAFETY: `message` was just filled in by PeekMessageW.
        unsafe { DispatchMessageW(&raw const message) };
    }
    true
}

/// The game list of Windows, with an event that it signals after a change.
struct GameListWatch {
    key: OpenKey,
    changed: Owned<HANDLE>,
}

impl GameListWatch {
    fn open() -> Result<Self> {
        let key = OpenKey::open(GAME_LIST_KEY)?;
        // SAFETY: plain call. An auto-reset event without a name.
        let changed = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }?;
        // SAFETY: CreateEventW returned a valid handle that nothing else owns.
        let changed = unsafe { Owned::new(changed) };
        let watch = Self { key, changed };
        watch.watch_next_change()?;
        Ok(watch)
    }

    fn event(&self) -> HANDLE {
        *self.changed
    }

    fn watch_next_change(&self) -> Result<()> {
        self.key.notify_change(*self.changed)
    }

    fn read(&self) -> GameList {
        GameList::new(self.key.subkey_names().iter().map(|name| {
            let subkey = PCWSTR(name.as_ptr());
            GameEntry {
                kind: self.key.subkey_number(subkey, KIND_VALUE),
                program: self.key.subkey_text(subkey, PROGRAM_VALUE),
            }
        }))
    }
}

/// An entry of the game list of Windows.
#[derive(Debug)]
struct GameEntry {
    kind: Option<u32>,
    /// The full path of the program, when Windows matched the entry to a program.
    program: Option<String>,
}

/// The programs of the game list that are games, in lowercase, because Windows compares paths
/// without case.
#[derive(Debug, Default)]
struct GameList {
    programs: HashSet<String>,
}

impl GameList {
    fn new(entries: impl IntoIterator<Item = GameEntry>) -> Self {
        let programs = entries
            .into_iter()
            .filter(|entry| entry.kind == Some(GAME_KIND))
            .filter_map(|entry| entry.program)
            .map(|program| program.to_lowercase())
            .collect();
        Self { programs }
    }

    /// Whether `program`, the program of the app in front, is a game.
    fn activity_of(&self, program: Option<&str>) -> GameActivity {
        let is_game = program.is_some_and(|program| self.programs.contains(&program.to_lowercase()));
        if is_game {
            GameActivity::InFront
        } else {
            GameActivity::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAME: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Balatro\Balatro.exe";

    fn entry(kind: u32, program: Option<&str>) -> GameEntry {
        GameEntry {
            kind: Some(kind),
            program: program.map(str::to_owned),
        }
    }

    #[test]
    fn program_of_a_game_entry_is_a_game() {
        let list = GameList::new([entry(GAME_KIND, Some(GAME))]);

        assert_eq!(list.activity_of(Some(GAME)), GameActivity::InFront);
    }

    #[test]
    fn program_matches_its_entry_without_case() {
        let list = GameList::new([entry(GAME_KIND, Some(GAME))]);

        let activity = list.activity_of(Some(&GAME.to_uppercase()));

        assert_eq!(activity, GameActivity::InFront);
    }

    #[test]
    fn program_of_an_entry_of_another_kind_is_no_game() {
        let list = GameList::new([entry(2, Some(GAME))]);

        assert_eq!(list.activity_of(Some(GAME)), GameActivity::None);
    }

    #[test]
    fn entry_without_a_kind_is_no_game() {
        let list = GameList::new([GameEntry {
            kind: None,
            program: Some(GAME.to_owned()),
        }]);

        assert_eq!(list.activity_of(Some(GAME)), GameActivity::None);
    }

    #[test]
    fn program_outside_the_list_is_no_game() {
        let list = GameList::new([entry(GAME_KIND, Some(GAME)), entry(GAME_KIND, None)]);

        let activity = list.activity_of(Some(r"C:\Windows\notepad.exe"));

        assert_eq!(activity, GameActivity::None);
    }

    #[test]
    fn no_program_in_front_is_no_game() {
        let list = GameList::new([entry(GAME_KIND, Some(GAME))]);

        assert_eq!(list.activity_of(None), GameActivity::None);
    }
}
