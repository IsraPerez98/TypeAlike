//! Windows platform layer of GhostType.
//!
//! Wraps Win32 and UI Automation behind safe types: keyboard and mouse hooks, reading text at the
//! caret, the ghost-text overlay and text injection. All `unsafe` code in the project belongs here.
