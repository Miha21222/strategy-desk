//! The chat (stage 5): the list in the sidebar, the chat page, and the one
//! place where the UI's chat actions become desk-app commands.
//!
//! The UI speaks [`ChatAction`], the contract's `ChatCommand` spelled out
//! (CONTRACTS «Чат (этап 5)»). desk-app's enum still has only `Send`, `Stop`
//! and `New`, so [`to_commands`] maps what it can and drops the rest with a
//! trace line; when the backend lands it is a one-to-one mapping.

pub mod markdown;
pub mod model;
pub mod view;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use desk_app::{ChatCommand, Command};
use desk_core::prompts::{macro_chat_title, run_command};
use desk_core::WindowId;

pub use self::model::{chats_of, ChatId, ChatItem, ChatKind, ChatMeta, ChatsView, Transcript};

/// What the chat UI asks desk-app for (the contract's `ChatCommand`).
#[derive(Debug, Clone, PartialEq)]
pub enum ChatAction {
    /// «+ Новый чат» (`Ask`).
    New {
        kind: ChatKind,
    },
    /// One macro chat per day; opening it from the list loads its transcript.
    OpenMacro {
        date: NaiveDate,
    },
    Send {
        chat_id: ChatId,
        text: String,
    },
    /// «Запустить / Пересобрать обзор»: OpenMacro + Send(run_command).
    Run {
        window: WindowId,
        date: NaiveDate,
    },
    Stop {
        chat_id: ChatId,
    },
    Rename {
        chat_id: ChatId,
        title: String,
    },
    /// Only from the list, after «Точно?».
    Delete {
        chat_id: ChatId,
    },
}

/// The bridge to today's desk-app. `Run` is sent the way the Electron desk
/// did it (a `Send` of the skill command into the day's macro chat, by
/// title); `OpenMacro`, `Rename` and `Delete` have no command yet.
pub fn to_commands(a: ChatAction) -> Vec<Command> {
    let c = match a {
        ChatAction::New { kind } => ChatCommand::New { kind },
        ChatAction::Send { chat_id, text } => ChatCommand::Send { chat_id, text },
        ChatAction::Stop { chat_id } => ChatCommand::Stop { chat_id },
        ChatAction::Run { window, date } => ChatCommand::Send {
            chat_id: macro_chat_title(date),
            text: run_command(window, date),
        },
        other => {
            let what = format!("{other:?}");
            crate::perf::trace::event("chat_action_unwired", &format!(", \"action\": {what:?}"));
            return Vec::new();
        }
    };
    vec![Command::Chat(c)]
}

/// Which page the main area shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Page {
    /// «Обзор дня».
    #[default]
    Day,
    /// A chat; `None` while desk-app is creating the one «+ Новый чат» asked for.
    Chat(Option<ChatId>),
}

/// `DESK_BENCH_AUTOCHAT=1`: one message once the first frame is out, so QA
/// can measure frames while a (fake) `claude` streams.
#[derive(Debug)]
pub struct AutoChat {
    pub text: String,
    started: Option<Instant>,
    done: bool,
}

/// How long autochat waits for desk-app to open the new chat.
const AUTOCHAT_TIMEOUT: Duration = Duration::from_secs(15);

impl AutoChat {
    pub fn new(text: String) -> Self {
        Self {
            text,
            started: None,
            done: false,
        }
    }
}

/// Everything the chat UI remembers between frames.
#[derive(Debug, Default)]
pub struct ChatUi {
    pub page: Page,
    /// The composer's text per chat; `""` = the chat being created.
    pub drafts: HashMap<ChatId, String>,
    /// Sent while the new chat did not exist yet: goes out when it does.
    pub pending_send: Option<String>,
    /// `open` as last seen in the view, so only a change moves the page.
    seen_open: Option<ChatId>,
    /// The page follows the next change of `open` (we asked for New/OpenMacro).
    follow_open: bool,
    pub md: markdown::Cache,
    /// Inline rename in the list: (chat, text being typed).
    pub rename: Option<(ChatId, String)>,
    /// «Удалить» pressed once: (chat, when), the second press deletes.
    pub confirm_delete: Option<(ChatId, f64)>,
    pub autochat: Option<AutoChat>,
}

impl ChatUi {
    /// The chat on screen, if a chat page is open and its chat exists.
    pub fn open_chat(&self) -> Option<&str> {
        match &self.page {
            Page::Chat(Some(id)) => Some(id),
            _ => None,
        }
    }

    /// Ask for a new «Ask» chat and show it as soon as desk-app opens it.
    pub fn start_new(&mut self, out: &mut Vec<ChatAction>) {
        self.page = Page::Chat(None);
        self.follow_open = true;
        out.push(ChatAction::New {
            kind: ChatKind::Ask,
        });
    }

    /// Open a chat from the list.
    pub fn open(&mut self, meta: &ChatMeta, out: &mut Vec<ChatAction>) {
        self.page = Page::Chat(Some(meta.id.clone()));
        if let (ChatKind::Macro, Some(date)) = (meta.kind, meta.date) {
            out.push(ChatAction::OpenMacro { date });
        }
    }

    /// Once per frame, before drawing: follow `open`, deliver a message typed
    /// while the chat was being created, drive autochat.
    pub fn sync(&mut self, view: &ChatsView, out: &mut Vec<ChatAction>) {
        if view.open != self.seen_open {
            self.seen_open = view.open.clone();
            if let (true, Some(id)) = (self.follow_open, &view.open) {
                self.follow_open = false;
                self.page = Page::Chat(Some(id.clone()));
                if let Some(d) = self.drafts.remove("") {
                    self.drafts.insert(id.clone(), d);
                }
            }
        }
        // A chat deleted under us: back to the day.
        if let Page::Chat(Some(id)) = &self.page {
            if !view.list.is_empty() && view.meta(id).is_none() {
                self.page = Page::Day;
            }
        }
        if let Some(id) = self.open_chat().map(str::to_owned) {
            let idle = view.meta(&id).is_some_and(|m| !m.busy);
            if idle {
                if let Some(text) = self.pending_send.take() {
                    out.push(ChatAction::Send { chat_id: id, text });
                }
            }
        }
    }

    /// Drive `DESK_BENCH_AUTOCHAT` (called from the second frame on).
    pub fn autochat_step(&mut self, out: &mut Vec<ChatAction>) {
        let Some(a) = &mut self.autochat else { return };
        if a.done {
            return;
        }
        match a.started {
            None => {
                a.started = Some(Instant::now());
                let text = a.text.clone();
                crate::perf::trace::mark("autochat_new");
                self.start_new(out);
                self.pending_send = Some(text);
            }
            Some(t0) => {
                if self.pending_send.is_none() {
                    a.done = true;
                    crate::perf::trace::mark("autochat_sent");
                    eprintln!("strategy-desk: DESK_BENCH_AUTOCHAT: message sent");
                } else if t0.elapsed() > AUTOCHAT_TIMEOUT {
                    a.done = true;
                    eprintln!(
                        "strategy-desk: DESK_BENCH_AUTOCHAT: no chat opened in {} s (is the chat backend in this build?)",
                        AUTOCHAT_TIMEOUT.as_secs()
                    );
                }
            }
        }
    }

    /// The composer sends: now, or once the new chat exists.
    pub fn send(&mut self, view: &ChatsView, text: String, out: &mut Vec<ChatAction>) {
        let text = text.trim().to_owned();
        if text.is_empty() {
            return;
        }
        match self.open_chat() {
            Some(id) if view.meta(id).is_some_and(|m| !m.busy) => out.push(ChatAction::Send {
                chat_id: id.to_owned(),
                text,
            }),
            Some(_) => {}
            None => self.pending_send = Some(text),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    pub(crate) fn meta(id: &str, kind: ChatKind, busy: bool) -> ChatMeta {
        let at = Utc.with_ymd_and_hms(2026, 10, 26, 7, 0, 0).unwrap();
        ChatMeta {
            id: id.into(),
            kind,
            title: id.into(),
            date: None,
            created_at: at,
            updated_at: at,
            session_id: None,
            busy,
            cost_usd: 0.0,
        }
    }

    #[test]
    fn run_goes_into_the_days_macro_chat() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        let cmds = to_commands(ChatAction::Run {
            window: WindowId::Eu,
            date: d,
        });
        assert_eq!(
            cmds,
            vec![Command::Chat(ChatCommand::Send {
                chat_id: "Обзор дня 26.10".into(),
                text: "/macro-review eu 2026-10-26".into(),
            })]
        );
        assert!(to_commands(ChatAction::Delete {
            chat_id: "x".into()
        })
        .is_empty());
    }

    #[test]
    fn a_new_chat_is_followed_and_gets_the_early_message() {
        let mut ui = ChatUi::default();
        let mut out = Vec::new();
        ui.start_new(&mut out);
        ui.send(&ChatsView::default(), "  привет ".into(), &mut out);
        assert_eq!(
            out,
            vec![ChatAction::New {
                kind: ChatKind::Ask
            }]
        );
        assert_eq!(ui.page, Page::Chat(None));
        // desk-app opened it, busy for a moment (it is starting): wait.
        let mut view = ChatsView {
            list: vec![meta("c1", ChatKind::Ask, true)],
            open: Some("c1".into()),
            ..ChatsView::default()
        };
        out.clear();
        ui.sync(&view, &mut out);
        assert_eq!(ui.page, Page::Chat(Some("c1".into())));
        assert!(out.is_empty());
        view.list[0].busy = false;
        ui.sync(&view, &mut out);
        assert_eq!(
            out,
            vec![ChatAction::Send {
                chat_id: "c1".into(),
                text: "привет".into()
            }]
        );
    }

    #[test]
    fn a_busy_chat_takes_no_send_and_run_does_not_move_the_page() {
        let mut ui = ChatUi {
            page: Page::Chat(Some("c1".into())),
            ..ChatUi::default()
        };
        let view = ChatsView {
            list: vec![
                meta("c1", ChatKind::Ask, true),
                meta("m", ChatKind::Macro, true),
            ],
            open: Some("m".into()),
            ..ChatsView::default()
        };
        let mut out = Vec::new();
        ui.send(&view, "ещё".into(), &mut out);
        assert!(out.is_empty());
        // `open` moved because «Запустить» opened the macro chat: we did not
        // ask to follow, so the page stays.
        ui.sync(&view, &mut out);
        assert_eq!(ui.page, Page::Chat(Some("c1".into())));
    }
}
