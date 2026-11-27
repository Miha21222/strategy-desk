//! The chat (stage 5): the list in the sidebar, the chat page, and the
//! state the UI keeps between frames. The UI speaks desk-app's
//! [`ChatCommand`] (CONTRACTS «Чат (этап 5)») and draws its `ChatsView`.

pub mod markdown;
pub mod model;
pub mod view;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use desk_app::Command;

pub use self::model::{
    chats_of, last_word, title_of, ChatCommand, ChatId, ChatItem, ChatKind, ChatMeta, ChatsView,
    Transcript, ViewExt,
};

/// What the chat UI asks desk-app for.
pub type ChatAction = ChatCommand;

/// Into the command desk-app takes.
pub fn to_command(a: ChatAction) -> Command {
    Command::Chat(a)
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
}

/// How long autochat waits for desk-app to open the new chat.
const AUTOCHAT_TIMEOUT: Duration = Duration::from_secs(15);

impl AutoChat {
    pub fn new(text: String) -> Self {
        Self {
            text,
            started: None,
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
    /// The page follows the next change of `open` (we asked for New).
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

    /// Open a chat from the list (desk-app loads its transcript).
    pub fn open(&mut self, id: &str, out: &mut Vec<ChatAction>) {
        self.page = Page::Chat(Some(id.to_owned()));
        out.push(ChatAction::Open {
            chat_id: id.to_owned(),
        });
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
    /// Returns `true` while it still waits for desk-app; once the message is
    /// out (or it gave up) autochat is dropped, so it stops asking for frames.
    pub fn autochat_step(&mut self, out: &mut Vec<ChatAction>) -> bool {
        let Some(a) = &mut self.autochat else {
            return false;
        };
        let Some(t0) = a.started else {
            a.started = Some(Instant::now());
            let text = a.text.clone();
            crate::perf::trace::mark("autochat_new");
            self.start_new(out);
            self.pending_send = Some(text);
            return true;
        };
        if self.pending_send.is_none() {
            crate::perf::trace::mark("autochat_sent");
            eprintln!("strategy-desk: DESK_BENCH_AUTOCHAT: message sent");
        } else if t0.elapsed() > AUTOCHAT_TIMEOUT {
            eprintln!(
                "strategy-desk: DESK_BENCH_AUTOCHAT: no chat opened in {} s",
                AUTOCHAT_TIMEOUT.as_secs()
            );
        } else {
            return true;
        }
        self.autochat = None;
        false
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
