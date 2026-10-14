---
paths:
  - "crates/ghosttype_windows/**"
  - "crates/ghosttype/**"
---
# Windows

## unsafe & FFI
- `unsafe` only in `ghosttype_windows`. Minimal blocks, `// SAFETY:` per block stating upheld invariants.
- Safe wrapper per OS resource: RAII type, `Drop` releases (hook, window, handle, COM init, GDI object). No raw handles in pub API.
- `unsafe fn` only if misuse = UB. Never fake `Send`/`Sync`.
- Wide (`W`) APIs only; UTF-16 conversion at the boundary, Rust strings inside.
- Check every return; map to typed errors right after the call (last-error read immediately).
- Pure logic (geometry, key mapping, DPI math) out of FFI code, unit-tested.

## Threads & runtime invariants
- Hooks, timers, window msgs on main thread. Never block the message loop.
- Hook callbacks fast (Windows drops slow LL hooks): no UIA, no `SendInput`, no alloc-heavy work inside; post msg, return.
- UIA on worker thread (COM MTA, init/uninit via guard). Requests carry ID; stale results dropped.
- Win32 callback thread-local state: one place only.
- `HWND` not `Send`: cross threads as `isize`.
- Tag own `SendInput` via `dwExtraInfo`; ignore only those (not all injected).
- Skip password fields, own process, elevated windows (UIPI), secure desktop.
- Per-monitor-v2 DPI aware → physical px everywhere; handle `WM_DPICHANGED`.

## Privacy
- Never log, persist, or send typed text unless the feature requires it and user enabled it.
