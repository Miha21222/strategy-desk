//! stream-json lines ⇄ [`DriverEvent`]s. Pure: no I/O, no clock.
//!
//! Shapes are the Agent SDK 0.3.278 `SDKMessage` union (`sdk.d.ts`) and the
//! mapping in `translate()` of `src/main/drivers/claude.ts`:
//!
//! | stdout frame | event |
//! | --- | --- |
//! | `system`/`init` | `SessionStarted` (once per id) |
//! | `stream_event` with `content_block_delta`/`text_delta` | `TextDelta` |
//! | `assistant` → `tool_use` block | `ToolCall` |
//! | `assistant` → `text` block whose text did NOT just stream as deltas | `TextDelta` |
//! | `assistant` with `error` | `Error { chat.error.api }` |
//! | `user` → `tool_result` block | `ToolResult` |
//! | `result` | `Usage`, maybe `Error`, `Done` |
//!
//! Frames with `parent_tool_use_id` set belong to a subagent and are dropped
//! (Task is disallowed anyway). Line content is never logged: a tool result
//! can hold anything, the sessionid of an `env` dump included.

use std::collections::HashSet;

use serde_json::{json, Value};

use super::{keys, DriverEvent, StopReason};

/// Frame types the CLI is known to send that carry nothing for the desk.
/// They are skipped at `trace`; anything else unknown is `warn`ed once.
const IGNORED_TYPES: &[&str] = &[
    "keep_alive",
    "control_response",
    "control_cancel_request",
    "rate_limit_event",
    "tool_progress",
    "tool_use_summary",
    "auth_status",
    "prompt_suggestion",
    // Seen in a real `claude` 2.1.332 run ahead of `system/init`.
    "active_goal",
    "autocompact_state",
];

/// Longest tool summary, in characters.
const SUMMARY_MAX: usize = 160;

/// What the process layer must do after a line.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Parsed {
    /// A `result` frame ended the turn.
    pub turn_done: bool,
    /// The CLI sent a `control_request` and waits for an answer (request id).
    pub control_request: Option<String>,
}

/// Stateful line translator for one process.
#[derive(Debug, Default)]
pub struct Translator {
    session_id: Option<String>,
    /// Text went out as deltas since the last `assistant` frame with a text
    /// block: that frame repeats it and must not print it again. Keyed on
    /// order, not on message ids: the CLI streams a block, then sends its
    /// whole-message twin, and a synthetic message (API error) that was never
    /// streamed comes with nothing pending, so its text is shown.
    pending_text: bool,
    interrupted: bool,
    warned: HashSet<String>,
}

impl Translator {
    pub fn new() -> Self {
        Self::default()
    }

    /// The user asked to stop this turn: its error result is not an error.
    pub fn mark_interrupted(&mut self) {
        self.interrupted = true;
    }

    /// The conversation id, once the CLI said it.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Translate one stdout line, pushing events onto `out`.
    pub fn feed(&mut self, line: &str, out: &mut Vec<DriverEvent>) -> Parsed {
        let line = line.trim();
        if line.is_empty() {
            return Parsed::default();
        }
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                tracing::warn!(
                    len = line.len(),
                    "claude: skipped a stdout line that is not JSON"
                );
                return Parsed::default();
            }
        };
        let Some(ty) = v.get("type").and_then(Value::as_str) else {
            tracing::warn!(
                len = line.len(),
                "claude: skipped a JSON line without a type"
            );
            return Parsed::default();
        };
        if v.get("parent_tool_use_id").is_some_and(|p| !p.is_null()) {
            return Parsed::default();
        }
        match ty {
            "system" => self.system(&v, out),
            "stream_event" => self.stream_event(&v, out),
            "assistant" => self.assistant(&v, out),
            "user" => user(&v, out),
            "result" => return self.result(&v, out),
            "control_request" => {
                let id = v
                    .get("request_id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                return Parsed {
                    turn_done: false,
                    control_request: id,
                };
            }
            t if IGNORED_TYPES.contains(&t) => {
                tracing::trace!(frame = t, "claude: ignored frame");
            }
            t => {
                if self.warned.insert(t.to_string()) {
                    tracing::warn!(frame = t, "claude: unknown stream-json frame type, skipped");
                }
            }
        }
        Parsed::default()
    }

    fn system(&mut self, v: &Value, out: &mut Vec<DriverEvent>) {
        if v.get("subtype").and_then(Value::as_str) != Some("init") {
            return;
        }
        if let Some(id) = v.get("session_id").and_then(Value::as_str) {
            self.session(id, out);
        }
    }

    fn session(&mut self, id: &str, out: &mut Vec<DriverEvent>) {
        if id.is_empty() || self.session_id.as_deref() == Some(id) {
            return;
        }
        self.session_id = Some(id.to_string());
        out.push(DriverEvent::SessionStarted {
            session_id: id.to_string(),
        });
    }

    fn stream_event(&mut self, v: &Value, out: &mut Vec<DriverEvent>) {
        let Some(ev) = v.get("event") else { return };
        if ev.get("type").and_then(Value::as_str) != Some("content_block_delta") {
            return;
        }
        let Some(delta) = ev.get("delta") else { return };
        if delta.get("type").and_then(Value::as_str) != Some("text_delta") {
            return; // thinking / input_json deltas: not shown
        }
        let Some(text) = delta.get("text").and_then(Value::as_str) else {
            return;
        };
        self.pending_text = true;
        if !text.is_empty() {
            out.push(DriverEvent::TextDelta {
                text: text.to_string(),
            });
        }
    }

    fn assistant(&mut self, v: &Value, out: &mut Vec<DriverEvent>) {
        let streamed = self.pending_text;
        if let Some(blocks) = v.pointer("/message/content").and_then(Value::as_array) {
            for b in blocks {
                match b.get("type").and_then(Value::as_str) {
                    Some("tool_use") => {
                        let id = str_of(b, "id");
                        let name = str_of(b, "name");
                        let summary = tool_summary(&name, b.get("input").unwrap_or(&Value::Null));
                        out.push(DriverEvent::ToolCall { id, name, summary });
                    }
                    // A message the CLI never streamed: a synthetic API-error
                    // message, or a CLI run without partial messages.
                    Some("text") if !streamed => {
                        let text = str_of(b, "text");
                        if !text.is_empty() {
                            out.push(DriverEvent::TextDelta { text });
                        }
                    }
                    Some("text") => self.pending_text = false,
                    _ => {}
                }
            }
        }
        if let Some(code) = v.get("error").and_then(Value::as_str) {
            out.push(DriverEvent::Error {
                key: keys::API.into(),
                detail: code.to_string(),
            });
        }
    }

    fn result(&mut self, v: &Value, out: &mut Vec<DriverEvent>) -> Parsed {
        if let Some(id) = v.get("session_id").and_then(Value::as_str) {
            self.session(id, out);
        }
        let usage = v.get("usage");
        let tokens = |k: &str| {
            usage
                .and_then(|u| u.get(k))
                .and_then(Value::as_u64)
                .unwrap_or(0)
        };
        out.push(DriverEvent::Usage {
            input: tokens("input_tokens"),
            output: tokens("output_tokens"),
            cost_usd: v.get("total_cost_usd").and_then(Value::as_f64),
        });
        let subtype = v.get("subtype").and_then(Value::as_str).unwrap_or("");
        // A real 2.1.332 auth failure reports `subtype: success` with
        // `is_error: true`, so both are checked.
        let is_error =
            subtype != "success" || v.get("is_error").and_then(Value::as_bool) == Some(true);
        let stop = if self.interrupted {
            StopReason::Interrupted
        } else if subtype == "error_max_turns" {
            out.push(DriverEvent::Error {
                key: keys::MAX_TURNS.into(),
                detail: subtype.into(),
            });
            StopReason::MaxTurns
        } else if is_error {
            let key = if resume_missing(v) {
                keys::RESUME_MISSING
            } else {
                keys::RESULT
            };
            out.push(DriverEvent::Error {
                key: key.into(),
                detail: result_detail(v, subtype),
            });
            StopReason::Error
        } else {
            StopReason::EndTurn
        };
        out.push(DriverEvent::Done { stop });
        // Per-turn state.
        self.interrupted = false;
        self.pending_text = false;
        Parsed {
            turn_done: true,
            control_request: None,
        }
    }
}

fn user(v: &Value, out: &mut Vec<DriverEvent>) {
    let Some(blocks) = v.pointer("/message/content").and_then(Value::as_array) else {
        return; // a string content is an echo of our own turn
    };
    for b in blocks {
        if b.get("type").and_then(Value::as_str) == Some("tool_result") {
            out.push(DriverEvent::ToolResult {
                id: str_of(b, "tool_use_id"),
                ok: b.get("is_error").and_then(Value::as_bool) != Some(true),
            });
        }
    }
}

/// The CLI's own wording; English regardless of locale. If it ever changes
/// the error still arrives, only under the generic key.
fn resume_missing(v: &Value) -> bool {
    v.get("errors").and_then(Value::as_array).is_some_and(|a| {
        a.iter()
            .filter_map(Value::as_str)
            .any(|e| e.starts_with("No conversation found"))
    })
}

/// `subtype: errors…` or, for an `is_error` success, the result text.
fn result_detail(v: &Value, subtype: &str) -> String {
    let errs: Vec<&str> = v
        .get("errors")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let mut d = subtype.to_string();
    if !errs.is_empty() {
        d.push_str(": ");
        d.push_str(&errs.join("\n"));
    } else if let Some(r) = v
        .get("result")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        d.push_str(": ");
        d.push_str(&truncate(r, 400));
    }
    d
}

fn str_of(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

/// One line for the folded tool row: the argument a person recognises
/// (the command, the file, the pattern), else compact JSON.
pub fn tool_summary(name: &str, input: &Value) -> String {
    let key = match name {
        "Bash" | "PowerShell" => Some("command"),
        "Read" | "Write" | "Edit" | "NotebookEdit" => Some("file_path"),
        "Glob" | "Grep" => Some("pattern"),
        "Skill" => Some("skill"),
        "WebFetch" => Some("url"),
        "WebSearch" => Some("query"),
        _ => None,
    };
    let raw = key
        .and_then(|k| input.get(k))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| match input {
            Value::Null => String::new(),
            Value::Object(m) if m.is_empty() => String::new(),
            other => other.to_string(),
        });
    let one_line = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate(&one_line, SUMMARY_MAX)
}

/// At most `max` chars, `…` when cut. Char-safe (Cyrillic paths).
pub fn truncate(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        None => s.to_string(),
        Some((i, _)) => format!("{}…", &s[..i]),
    }
}

/// A user turn on stdin (`userMessage` in `src/main/stream-input.ts`).
pub fn user_line(text: &str) -> String {
    let v = json!({
        "type": "user",
        "message": { "role": "user", "content": text },
        "parent_tool_use_id": null,
        "session_id": "",
    });
    format!("{v}\n")
}

/// The SDK's `Query.interrupt()`: a control request on stdin.
pub fn interrupt_line(request_id: &str) -> String {
    let v = json!({
        "type": "control_request",
        "request_id": request_id,
        "request": { "subtype": "interrupt" },
    });
    format!("{v}\n")
}

/// Refuse a control request the desk does not serve (with `dontAsk` and no
/// `--permission-prompt-tool` none is expected; an unanswered one would hang
/// the turn).
pub fn control_error_line(request_id: &str) -> String {
    let v = json!({
        "type": "control_response",
        "response": { "subtype": "error", "request_id": request_id, "error": "unsupported by Strategy Desk" },
    });
    format!("{v}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(fixture: &str) -> Vec<DriverEvent> {
        let mut t = Translator::new();
        let mut out = Vec::new();
        for l in fixture.lines() {
            t.feed(l, &mut out);
        }
        out
    }

    fn text_of(ev: &[DriverEvent]) -> String {
        ev.iter()
            .filter_map(|e| match e {
                DriverEvent::TextDelta { text } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn text_turn() {
        let ev = run(include_str!("testdata/text_turn.jsonl"));
        assert_eq!(
            ev.first(),
            Some(&DriverEvent::SessionStarted {
                session_id: "5f0c2a8e-1111-4c1e-9a55-000000000001".into()
            })
        );
        // Streamed once; the whole-message assistant frame is not printed again.
        assert_eq!(text_of(&ev), "Привет! Европа открывается спокойно.");
        let n = ev.len();
        assert_eq!(
            ev[n - 2],
            DriverEvent::Usage {
                input: 812,
                output: 14,
                cost_usd: Some(0.0213)
            }
        );
        assert_eq!(
            ev[n - 1],
            DriverEvent::Done {
                stop: StopReason::EndTurn
            }
        );
        assert!(!ev.iter().any(|e| matches!(e, DriverEvent::Error { .. })));
    }

    #[test]
    fn tool_turn() {
        let ev = run(include_str!("testdata/tool_turn.jsonl"));
        let tools: Vec<_> = ev
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    DriverEvent::ToolCall { .. } | DriverEvent::ToolResult { .. }
                )
            })
            .cloned()
            .collect();
        assert_eq!(
            tools,
            vec![
                DriverEvent::ToolCall {
                    id: "toolu_01".into(),
                    name: "Bash".into(),
                    summary: "python indices-kb/tools/scale.py show --date 2026-11-25 --region eu"
                        .into()
                },
                DriverEvent::ToolResult {
                    id: "toolu_01".into(),
                    ok: true
                },
                DriverEvent::ToolCall {
                    id: "toolu_02".into(),
                    name: "Read".into(),
                    summary: "indices-kb/state/scale_2026-11-25_eu.json".into()
                },
                DriverEvent::ToolResult {
                    id: "toolu_02".into(),
                    ok: false
                },
            ]
        );
        // The subagent frame's text is not in the answer.
        assert_eq!(text_of(&ev), "Смотрю шкалу. Шкала: слабый лонг.");
        assert_eq!(
            ev.last(),
            Some(&DriverEvent::Done {
                stop: StopReason::EndTurn
            })
        );
        // init repeats on the second model call: one SessionStarted only.
        assert_eq!(
            ev.iter()
                .filter(|e| matches!(e, DriverEvent::SessionStarted { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn auth_error_recorded_from_a_real_cli() {
        let ev = run(include_str!("testdata/auth_error.jsonl"));
        assert_eq!(
            ev,
            vec![
                DriverEvent::SessionStarted {
                    session_id: "54eb4b92-1c55-4682-9c37-40ce7e9dd2be".into()
                },
                DriverEvent::TextDelta {
                    text: "Authentication error · This may be a temporary network issue, please try again".into()
                },
                DriverEvent::Error {
                    key: keys::API.into(),
                    detail: "authentication_failed".into()
                },
                DriverEvent::Usage { input: 0, output: 0, cost_usd: Some(0.0) },
                DriverEvent::Error {
                    key: keys::RESULT.into(),
                    detail: "success: Authentication error · This may be a temporary network issue, please try again".into()
                },
                DriverEvent::Done { stop: StopReason::Error },
            ]
        );
    }

    #[test]
    fn resume_of_a_missing_conversation_recorded_from_a_real_cli() {
        let ev = run(include_str!("testdata/resume_missing.jsonl"));
        let n = ev.len();
        assert!(
            matches!(&ev[n - 2], DriverEvent::Error { key, detail } if key == keys::RESUME_MISSING && detail.contains("No conversation found")),
            "{ev:?}"
        );
        assert_eq!(
            ev[n - 1],
            DriverEvent::Done {
                stop: StopReason::Error
            }
        );
    }

    #[test]
    fn interrupted_turn_is_not_an_error() {
        let mut t = Translator::new();
        let mut out = Vec::new();
        let fixture = include_str!("testdata/interrupted.jsonl");
        let mut lines = fixture.lines();
        for l in lines.by_ref().take(4) {
            t.feed(l, &mut out);
        }
        t.mark_interrupted();
        let mut last = Parsed::default();
        for l in lines {
            last = t.feed(l, &mut out);
        }
        assert!(last.turn_done);
        assert_eq!(
            out.last(),
            Some(&DriverEvent::Done {
                stop: StopReason::Interrupted
            })
        );
        assert!(!out.iter().any(|e| matches!(e, DriverEvent::Error { .. })));
        // Without the stop request the same result is an error.
        let ev = run(fixture);
        assert_eq!(
            ev.last(),
            Some(&DriverEvent::Done {
                stop: StopReason::Error
            })
        );
    }

    #[test]
    fn max_turns() {
        let ev = run(
            r#"{"type":"result","subtype":"error_max_turns","is_error":true,"num_turns":200,"usage":{"input_tokens":5,"output_tokens":6},"total_cost_usd":1.5,"errors":[],"session_id":"s1"}"#,
        );
        assert_eq!(
            ev,
            vec![
                DriverEvent::SessionStarted {
                    session_id: "s1".into()
                },
                DriverEvent::Usage {
                    input: 5,
                    output: 6,
                    cost_usd: Some(1.5)
                },
                DriverEvent::Error {
                    key: keys::MAX_TURNS.into(),
                    detail: "error_max_turns".into()
                },
                DriverEvent::Done {
                    stop: StopReason::MaxTurns
                },
            ]
        );
    }

    #[test]
    fn junk_and_unknown_lines_are_skipped() {
        let mut t = Translator::new();
        let mut out = Vec::new();
        for l in [
            "",
            "running 1 test",
            "{not json",
            "[1,2]",
            r#"{"type":"brand_new_frame","x":1}"#,
            r#"{"type":"brand_new_frame"}"#,
            r#"{"type":"keep_alive"}"#,
        ] {
            assert_eq!(t.feed(l, &mut out), Parsed::default());
        }
        assert!(out.is_empty());
        assert_eq!(t.warned.len(), 1);
    }

    #[test]
    fn control_request_is_surfaced() {
        let mut t = Translator::new();
        let mut out = Vec::new();
        let p = t.feed(r#"{"type":"control_request","request_id":"r7","request":{"subtype":"can_use_tool","tool_name":"Bash"}}"#, &mut out);
        assert_eq!(p.control_request.as_deref(), Some("r7"));
        assert!(out.is_empty());
    }

    /// desk-bench's fake gives the whole-message frame another id than
    /// `message_start`; the twin must still not print twice.
    #[test]
    fn a_twin_with_another_message_id_is_not_printed_twice() {
        let ev = run(concat!(
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_a"}},"parent_tool_use_id":null}"#,
            "\n",
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"раз "}},"parent_tool_use_id":null}"#,
            "\n",
            r#"{"type":"assistant","message":{"id":"msg_b","content":[{"type":"text","text":"раз "}]},"parent_tool_use_id":null}"#,
            "\n",
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"два"}},"parent_tool_use_id":null}"#,
            "\n",
            r#"{"type":"assistant","message":{"id":"msg_c","content":[{"type":"text","text":"два"}]},"parent_tool_use_id":null}"#,
            "\n",
            r#"{"type":"assistant","message":{"id":"synthetic","content":[{"type":"text","text":" ошибка"}]},"parent_tool_use_id":null,"error":"rate_limit"}"#,
        ));
        assert_eq!(text_of(&ev), "раз два ошибка");
    }

    #[test]
    fn unstreamed_assistant_text_is_printed() {
        let ev = run(
            r#"{"type":"assistant","message":{"id":"m1","role":"assistant","content":[{"type":"text","text":"целиком"}]},"parent_tool_use_id":null,"session_id":"s"}"#,
        );
        assert_eq!(text_of(&ev), "целиком");
    }

    #[test]
    fn stdin_lines() {
        let u: Value =
            serde_json::from_str(user_line("строка \"с\" кавычками\nи переносом").trim_end())
                .unwrap();
        assert_eq!(u["type"], "user");
        assert_eq!(u["message"]["role"], "user");
        assert_eq!(
            u["message"]["content"],
            "строка \"с\" кавычками\nи переносом"
        );
        assert!(u["parent_tool_use_id"].is_null());
        assert!(user_line("a\nb").ends_with('\n'));
        assert_eq!(user_line("a\nb").matches('\n').count(), 1);
        let i: Value = serde_json::from_str(interrupt_line("q1").trim_end()).unwrap();
        assert_eq!(i["request"]["subtype"], "interrupt");
        assert_eq!(i["request_id"], "q1");
        let e: Value = serde_json::from_str(control_error_line("r7").trim_end()).unwrap();
        assert_eq!(e["response"]["request_id"], "r7");
        assert_eq!(e["response"]["subtype"], "error");
    }

    #[test]
    fn summaries() {
        assert_eq!(
            tool_summary("Bash", &json!({"command": "ls\n  -la"})),
            "ls -la"
        );
        assert_eq!(
            tool_summary("mcp__tradingview__tv_calendar", &json!({})),
            ""
        );
        assert_eq!(tool_summary("X", &json!({"a": 1})), r#"{"a":1}"#);
        let long = "я".repeat(300);
        let s = tool_summary("Read", &json!({ "file_path": long }));
        assert_eq!(s.chars().count(), SUMMARY_MAX + 1);
        assert!(s.ends_with('…'));
    }
}
