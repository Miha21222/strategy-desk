//! Switches that become `settings.*` in stage 6 (the settings screen). Until
//! then they are constants, so turning one on is a one-line change.

/// `settings.discuss`: the «Не согласен?» card (an argument from the owner
/// → a chat turn → `scale.py discuss`). Off by the owner's word (25.11:
/// «функция сырая»); the code stays and is drawn when this is on.
pub const DISCUSS: bool = false;
