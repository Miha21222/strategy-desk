//! The chat's data: what is stored per chat, what the UI draws, and the pure
//! fold of driver events into a transcript (the twin of the `chat:event`
//! fold in `src/renderer/app.tsx` and the tallies in `src/main/agent.ts`).

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};
use desk_core::windows::WindowId;
use serde::{Deserialize, Serialize};

use crate::claude::{keys as driver_keys, DriverEvent, StopReason};

/// File-name-safe id (`[a-z0-9-]`), see [`super::store::is_valid_id`].
pub type ChatId = String;

/// The MVP's two chats: the day's review chat, and «+ Новый чат».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatKind {
    Macro,
    Ask,
}

/// A chat command from the UI (CONTRACTS «Чат (этап 5)»).
#[derive(Debug, Clone, PartialEq)]
pub enum ChatCommand {
    /// A new chat; `Ask` is «+ Новый чат». Opens it.
    New {
        kind: ChatKind,
    },
    /// The day's review chat (one per date), created on first use. Opens it.
    OpenMacro {
        date: NaiveDate,
    },
    /// Open a chat from the list (loads its transcript).
    Open {
        chat_id: ChatId,
    },
    Send {
        chat_id: ChatId,
        text: String,
    },
    /// OpenMacro + Send(text): «Не согласен?» (`discuss_prompt`) and the like.
    SendMacro {
        date: NaiveDate,
        text: String,
    },
    /// «Запустить / Пересобрать обзор»: SendMacro(`run_command(window, date)`).
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
    Delete {
        chat_id: ChatId,
    },
}

/// One chat in the list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMeta {
    pub id: ChatId,
    pub kind: ChatKind,
    /// Empty until the first message names it (UI shows its own placeholder).
    pub title: String,
    /// The day of a macro chat.
    pub date: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Claude Code's conversation id; `--resume` with it next time.
    pub session_id: Option<String>,
    /// A turn is running. Never stored: after a restart nothing runs.
    #[serde(skip)]
    pub busy: bool,
    pub cost_usd: f64,
}

/// What a chat shows, in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatItem {
    User {
        text: String,
        at: DateTime<Utc>,
    },
    /// `live`: still being written; the UI re-parses only this one.
    Assistant {
        text: String,
        at: DateTime<Utc>,
        live: bool,
    },
    /// One folded tool row. `ok`: `None` while running.
    Tool {
        id: String,
        name: String,
        summary: String,
        ok: Option<bool>,
    },
    /// Something the user should see, as an i18n key (+ vars).
    Notice {
        key: String,
        vars: Vec<(String, String)>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    pub items: Vec<ChatItem>,
}

/// The chat part of the snapshot.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatsView {
    /// Newest (last updated) first.
    pub list: Vec<ChatMeta>,
    pub open: Option<ChatId>,
    /// Only the open chat and chats with a running turn.
    pub transcripts: HashMap<ChatId, Arc<Transcript>>,
}

/// i18n keys of the chat (the driver's own come through `Error`).
pub mod keys {
    pub const NO_CLAUDE: &str = "chat.error.no_claude";
    pub const NO_VAULT: &str = "chat.error.no_vault";
    pub const SPAWN: &str = "chat.error.spawn";
    pub const BUSY: &str = "chat.error.busy";
    pub const UNKNOWN_CHAT: &str = "chat.error.unknown_chat";
    pub const STORE: &str = "chat.error.store";
    pub const STOPPED: &str = "chat.notice.stopped";
}

/// Longest title kept (`chats:rename` sliced to 120).
pub const TITLE_MAX: usize = 120;
/// A title taken from the first message (`agent.ts`: 60).
pub const AUTO_TITLE_MAX: usize = 60;

impl Transcript {
    pub fn push_user(&mut self, text: &str, at: DateTime<Utc>) {
        self.items.push(ChatItem::User {
            text: text.to_string(),
            at,
        });
    }

    pub fn notice(&mut self, key: &str, vars: Vec<(String, String)>) {
        self.items.push(ChatItem::Notice {
            key: key.to_string(),
            vars,
        });
    }

    /// End every live answer (a turn is over, or the process is gone).
    pub fn close_live(&mut self) {
        for it in &mut self.items {
            if let ChatItem::Assistant { live, .. } = it {
                *live = false;
            }
        }
    }
}

/// What the caller must do after an event.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Applied {
    /// Store the chat now (session id learned, turn over).
    pub persist: bool,
    /// The turn ended.
    pub done: Option<StopReason>,
    /// `--resume` named a conversation the CLI does not have: forget it.
    pub resume_missing: bool,
}

/// Fold one driver event into a chat.
pub fn apply_event(
    meta: &mut ChatMeta,
    t: &mut Transcript,
    ev: DriverEvent,
    now: DateTime<Utc>,
) -> Applied {
    let mut out = Applied::default();
    match ev {
        DriverEvent::SessionStarted { session_id } => {
            if meta.session_id.as_deref() != Some(session_id.as_str()) {
                meta.session_id = Some(session_id);
                out.persist = true;
            }
        }
        DriverEvent::TextDelta { text } => match t.items.last_mut() {
            Some(ChatItem::Assistant {
                text: acc,
                live: true,
                ..
            }) => acc.push_str(&text),
            _ => t.items.push(ChatItem::Assistant {
                text,
                at: now,
                live: true,
            }),
        },
        DriverEvent::ToolCall { id, name, summary } => {
            t.items.push(ChatItem::Tool {
                id,
                name,
                summary,
                ok: None,
            });
        }
        DriverEvent::ToolResult { id, ok: result } => {
            let hit = t.items.iter_mut().rev().find_map(|it| match it {
                ChatItem::Tool { id: tid, ok, .. } if *tid == id => Some(ok),
                _ => None,
            });
            if let Some(ok) = hit {
                *ok = Some(result);
            }
        }
        DriverEvent::Usage { cost_usd, .. } => {
            // `agent.ts`: the CLI's total is cumulative (a resumed session
            // carries what its transcript saved); a smaller number is a fresh
            // process's own spend, added on top.
            if let Some(c) = cost_usd.filter(|c| c.is_finite() && *c >= 0.0) {
                meta.cost_usd = if c >= meta.cost_usd {
                    c
                } else {
                    meta.cost_usd + c
                };
            }
        }
        DriverEvent::Done { stop } => {
            t.close_live();
            if stop == StopReason::Interrupted {
                t.notice(keys::STOPPED, Vec::new());
            }
            meta.busy = false;
            meta.updated_at = now;
            out.persist = true;
            out.done = Some(stop);
        }
        DriverEvent::Error { key, detail } => {
            out.resume_missing = key == driver_keys::RESUME_MISSING;
            let vars = if detail.is_empty() {
                Vec::new()
            } else {
                vec![("detail".to_string(), detail)]
            };
            t.notice(&key, vars);
        }
    }
    out
}

/// A chat's title from its first message: one line, `AUTO_TITLE_MAX` chars.
pub fn auto_title(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(AUTO_TITLE_MAX)
        .collect()
}

/// A title as the owner typed it: trimmed, one line, `TITLE_MAX` chars.
pub fn clean_title(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(TITLE_MAX)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 26, 7, 0, 0).unwrap()
    }

    fn meta() -> ChatMeta {
        ChatMeta {
            id: "c1".into(),
            kind: ChatKind::Ask,
            title: String::new(),
            date: None,
            created_at: now(),
            updated_at: now(),
            session_id: None,
            busy: true,
            cost_usd: 0.0,
        }
    }

    fn fold(evs: Vec<DriverEvent>) -> (ChatMeta, Transcript, Vec<Applied>) {
        let (mut m, mut t) = (meta(), Transcript::default());
        t.push_user("вопрос", now());
        let a = evs
            .into_iter()
            .map(|e| apply_event(&mut m, &mut t, e, now()))
            .collect();
        (m, t, a)
    }

    fn text(s: &str) -> DriverEvent {
        DriverEvent::TextDelta { text: s.into() }
    }

    #[test]
    fn a_turn_with_a_tool_folds_into_items() {
        let (m, t, a) = fold(vec![
            DriverEvent::SessionStarted {
                session_id: "s1".into(),
            },
            text("Смотрю "),
            text("шкалу."),
            DriverEvent::ToolCall {
                id: "t1".into(),
                name: "Bash".into(),
                summary: "python scale.py".into(),
            },
            DriverEvent::ToolResult {
                id: "t1".into(),
                ok: false,
            },
            text("Готово."),
            DriverEvent::Usage {
                input: 10,
                output: 5,
                cost_usd: Some(0.02),
            },
            DriverEvent::Done {
                stop: StopReason::EndTurn,
            },
        ]);
        assert_eq!(
            t.items[1..],
            [
                ChatItem::Assistant {
                    text: "Смотрю шкалу.".into(),
                    at: now(),
                    live: false
                },
                ChatItem::Tool {
                    id: "t1".into(),
                    name: "Bash".into(),
                    summary: "python scale.py".into(),
                    ok: Some(false)
                },
                ChatItem::Assistant {
                    text: "Готово.".into(),
                    at: now(),
                    live: false
                },
            ]
        );
        assert_eq!(m.session_id.as_deref(), Some("s1"));
        assert!(!m.busy);
        assert_eq!(m.cost_usd, 0.02);
        assert!(a[0].persist, "a new session id is stored at once");
        assert_eq!(a.last().unwrap().done, Some(StopReason::EndTurn));
    }

    #[test]
    fn text_is_live_until_done() {
        let (mut m, mut t) = (meta(), Transcript::default());
        apply_event(&mut m, &mut t, text("a"), now());
        assert!(matches!(t.items[0], ChatItem::Assistant { live: true, .. }));
        apply_event(
            &mut m,
            &mut t,
            DriverEvent::Done {
                stop: StopReason::ProcessDied,
            },
            now(),
        );
        assert!(matches!(
            t.items[0],
            ChatItem::Assistant { live: false, .. }
        ));
    }

    #[test]
    fn stop_and_death_leave_a_notice_and_free_the_chat() {
        let (m, t, _) = fold(vec![
            text("нач"),
            DriverEvent::Done {
                stop: StopReason::Interrupted,
            },
        ]);
        assert_eq!(
            t.items.last(),
            Some(&ChatItem::Notice {
                key: keys::STOPPED.into(),
                vars: vec![]
            })
        );
        assert!(!m.busy);
        let (m, t, _) = fold(vec![
            text("нач"),
            DriverEvent::Error {
                key: driver_keys::PROCESS_DIED.into(),
                detail: "exit status: 3".into(),
            },
            DriverEvent::Done {
                stop: StopReason::ProcessDied,
            },
        ]);
        assert_eq!(
            t.items.last(),
            Some(&ChatItem::Notice {
                key: driver_keys::PROCESS_DIED.into(),
                vars: vec![("detail".into(), "exit status: 3".into())]
            })
        );
        assert!(!m.busy);
    }

    #[test]
    fn cost_follows_the_old_rule() {
        let (mut m, mut t) = (meta(), Transcript::default());
        let usage = |c| DriverEvent::Usage {
            input: 0,
            output: 0,
            cost_usd: Some(c),
        };
        apply_event(&mut m, &mut t, usage(0.10), now());
        apply_event(&mut m, &mut t, usage(0.25), now()); // cumulative total
        assert_eq!(m.cost_usd, 0.25);
        apply_event(&mut m, &mut t, usage(0.05), now()); // a fresh process: added
        assert!((m.cost_usd - 0.30).abs() < 1e-12);
        apply_event(
            &mut m,
            &mut t,
            DriverEvent::Usage {
                input: 0,
                output: 0,
                cost_usd: None,
            },
            now(),
        );
        assert!((m.cost_usd - 0.30).abs() < 1e-12);
    }

    #[test]
    fn resume_missing_is_flagged() {
        let (_, _, a) = fold(vec![DriverEvent::Error {
            key: driver_keys::RESUME_MISSING.into(),
            detail: String::new(),
        }]);
        assert!(a[0].resume_missing);
    }

    #[test]
    fn titles() {
        assert_eq!(auto_title("  что\nбыло   вчера? "), "что было вчера?");
        assert_eq!(auto_title(&"я".repeat(100)).chars().count(), AUTO_TITLE_MAX);
        assert_eq!(clean_title(&"я".repeat(200)).chars().count(), TITLE_MAX);
    }

    #[test]
    fn items_serialize_tagged() {
        let v = serde_json::to_value(ChatItem::Tool {
            id: "t".into(),
            name: "Read".into(),
            summary: "a.md".into(),
            ok: None,
        })
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({ "type": "tool", "id": "t", "name": "Read", "summary": "a.md", "ok": null })
        );
    }
}
