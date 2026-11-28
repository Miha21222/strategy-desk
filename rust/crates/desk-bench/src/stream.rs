//! The lines `claude -p --output-format stream-json --verbose
//! --include-partial-messages` writes, generated instead of bought.
//!
//! Shapes follow `@anthropic-ai/claude-agent-sdk@0.3.278` `sdk.d.ts`
//! (`SDKSystemMessage` init, `SDKPartialAssistantMessage` = `stream_event`,
//! `SDKAssistantMessage`, `SDKUserMessage` with `tool_result`,
//! `SDKResultSuccess`) and what `translate()` in `src/main/drivers/claude.ts`
//! actually reads: `content_block_delta/text_delta`, `assistant` content
//! `tool_use`, `user` content `tool_result`, `result` with `usage` +
//! `total_cost_usd`. Fields the driver ignores are present but small.
//!
//! One turn is exactly `events` lines (the init line is extra, once per
//! process), so a consumer can assert it saw every event.

use crate::rng::Rng;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct StreamSpec {
    /// Lines per turn, `result` included.
    pub events: usize,
    pub seed: u64,
    /// Every n-th content block is a tool call (0 = never).
    pub tool_every: usize,
}

impl Default for StreamSpec {
    fn default() -> Self {
        StreamSpec {
            events: 10_000,
            seed: 23,
            tool_every: 4,
        }
    }
}

fn uuid(rng: &mut Rng) -> String {
    let a = rng.next_u64();
    let b = rng.next_u64();
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        a >> 32,
        (a >> 16) & 0xffff,
        a & 0x0fff,
        ((b >> 48) & 0x3fff) | 0x8000,
        b & 0xffff_ffff_ffff
    )
}

pub fn session_id(seed: u64) -> String {
    uuid(&mut Rng::fork(seed, "session"))
}

fn line(v: &Value) -> String {
    // serde_json::to_string of a Value cannot fail (no non-string map keys).
    serde_json::to_string(v).unwrap_or_default()
}

pub fn init_line(seed: u64, cwd: &str) -> String {
    line(&json!({
        "type": "system",
        "subtype": "init",
        "apiKeySource": "none",
        "claude_code_version": "0.0.0-fake",
        "cwd": cwd,
        "tools": ["Bash", "Read", "Write", "Edit", "Glob", "Grep", "Skill"],
        "mcp_servers": [],
        "model": "claude-fake",
        "permissionMode": "default",
        "slash_commands": ["macro-review"],
        "output_style": "default",
        "skills": ["macro-review", "humanizer"],
        "plugins": [],
        "uuid": uuid(&mut Rng::fork(seed, "init")),
        "session_id": session_id(seed),
    }))
}

const WORDS: &[&str] = &[
    "DAX",
    "окно",
    "Европы",
    "открылось",
    "ниже",
    "ночного",
    "минимума,",
    "стрелка",
    "−18",
    "→",
    "шорт.",
    "**Почему так:**",
    "\n\n- ",
    "ночь",
    "сняла",
    "вчерашний",
    "максимум",
    "и",
    "вернулась",
    "ниже;",
    "IB1",
    "0.37",
    "ADR",
    "`scale.py`",
    "\n\n",
    "### Итог",
    "Bund",
    "+7 б.п.",
    "VIX",
    "растёт",
    "—",
    "осторожно.",
    "The",
    "needle",
    "moved",
    "after",
    "the",
    "argument.",
    "\n\n```text\nscale 25100 → 25040\n```\n",
];

const TOOLS: &[(&str, &str)] = &[
    (
        "Bash",
        r#"{"command":"python indices-kb/tools/scale.py review --date DATE","description":"Evening review"}"#,
    ),
    (
        "Read",
        r#"{"file_path":"indices-kb/state/scale_DATE_eu.json"}"#,
    ),
    ("Skill", r#"{"skill":"macro-review","args":"--window eu"}"#),
    (
        "Write",
        r#"{"file_path":"indices-kb/state/review_DATE_agent.json","content":"{}"}"#,
    ),
];

struct Turn<'a> {
    rng: Rng,
    sid: &'a str,
    out: Vec<String>,
    text: String,
    block: usize,
}

impl Turn<'_> {
    fn ev(&mut self, event: Value) {
        let u = uuid(&mut self.rng);
        self.out.push(line(&json!({
            "type": "stream_event",
            "event": event,
            "parent_tool_use_id": null,
            "uuid": u,
            "session_id": self.sid,
        })));
    }

    fn assistant(&mut self, content: Value) {
        let u = uuid(&mut self.rng);
        self.out.push(line(&json!({
            "type": "assistant",
            "message": {
                "id": format!("msg_fake_{}", self.block),
                "type": "message",
                "role": "assistant",
                "model": "claude-fake",
                "content": content,
                "stop_reason": null,
                "usage": {"input_tokens": 3, "output_tokens": 12},
            },
            "parent_tool_use_id": null,
            "uuid": u,
            "session_id": self.sid,
        })));
    }

    /// 3 + `deltas` lines.
    fn text_block(&mut self, deltas: usize) {
        let i = self.block;
        self.ev(json!({"type": "content_block_start", "index": i, "content_block": {"type": "text", "text": ""}}));
        let mut block_text = String::new();
        for _ in 0..deltas {
            let n = 1 + self.rng.below(3);
            let mut piece = String::new();
            for _ in 0..n {
                if let Some(w) = self.rng.pick(WORDS) {
                    piece.push_str(w);
                    piece.push(' ');
                }
            }
            block_text.push_str(&piece);
            self.ev(json!({"type": "content_block_delta", "index": i, "delta": {"type": "text_delta", "text": piece}}));
        }
        self.ev(json!({"type": "content_block_stop", "index": i}));
        self.assistant(json!([{"type": "text", "text": block_text}]));
        self.text.push_str(&block_text);
        self.block += 1;
    }

    /// 7 lines: start, 3 json deltas, stop, assistant tool_use, user tool_result.
    fn tool_block(&mut self, date: &str) {
        let i = self.block;
        let (name, input) = self.rng.pick(TOOLS).copied().unwrap_or(TOOLS[0]);
        let input = input.replace("DATE", date);
        let id = format!("toolu_fake_{:06}", self.rng.below(1_000_000));
        self.ev(json!({"type": "content_block_start", "index": i,
            "content_block": {"type": "tool_use", "id": id, "name": name, "input": {}}}));
        let third = input.len() / 3;
        let cuts = [0, third, 2 * third, input.len()];
        for w in cuts.windows(2) {
            let part = input.get(w[0]..w[1]).unwrap_or("");
            self.ev(json!({"type": "content_block_delta", "index": i,
                "delta": {"type": "input_json_delta", "partial_json": part}}));
        }
        self.ev(json!({"type": "content_block_stop", "index": i}));
        let parsed: Value = serde_json::from_str(&input).unwrap_or(json!({}));
        self.assistant(json!([{"type": "tool_use", "id": id, "name": name, "input": parsed}]));
        let ok = self.rng.chance(0.9);
        let u = uuid(&mut self.rng);
        self.out.push(line(&json!({
            "type": "user",
            "message": {"role": "user", "content": [{
                "type": "tool_result",
                "tool_use_id": id,
                "content": if ok { "ok: wrote indices-kb/state/review.json (synthetic)" } else { "Error: exit code 1 (synthetic)" },
                "is_error": !ok,
            }]},
            "parent_tool_use_id": null,
            "uuid": u,
            "session_id": self.sid,
        })));
        self.block += 1;
    }
}

/// Exactly `spec.events` lines for turn number `turn_no` (0-based).
pub fn turn(spec: &StreamSpec, turn_no: usize, date: &str) -> Vec<String> {
    let sid = session_id(spec.seed);
    let mut t = Turn {
        rng: Rng::fork(spec.seed, &format!("turn{turn_no}")),
        sid: &sid,
        out: Vec::with_capacity(spec.events),
        text: String::new(),
        block: 0,
    };
    let n = spec.events.max(1);
    const TAIL: usize = 3; // message_delta, message_stop, result
    if n > TAIL {
        t.ev(json!({"type": "message_start", "message": {"id": "msg_fake", "type": "message", "role": "assistant",
            "model": "claude-fake", "content": [], "usage": {"input_tokens": 1200, "output_tokens": 1}}}));
    }
    loop {
        let avail = n.saturating_sub(t.out.len() + TAIL);
        let tool_due = spec.tool_every > 0 && t.block % spec.tool_every == spec.tool_every - 1;
        if tool_due && avail >= 7 {
            t.tool_block(date);
        } else if avail >= 4 {
            let want = 20 + t.rng.below(60);
            t.text_block(want.min(avail - 3));
        } else if avail > 0 {
            t.ev(json!({"type": "ping"}));
        } else {
            break;
        }
    }
    let out_tokens = (t.text.len() / 4) as u64;
    if n > TAIL {
        t.ev(json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": out_tokens}}));
        t.ev(json!({"type": "message_stop"}));
    } else {
        while t.out.len() + 1 < n {
            t.ev(json!({"type": "ping"}));
        }
    }
    let mut summary: String = t.text.chars().take(2000).collect();
    if summary.is_empty() {
        summary = "ok".into();
    }
    let usage = json!({"input_tokens": 1200, "output_tokens": out_tokens,
        "cache_read_input_tokens": 24_000, "cache_creation_input_tokens": 800});
    let u = uuid(&mut t.rng);
    t.out.push(line(&json!({
        "type": "result",
        "subtype": "success",
        "is_error": false,
        "duration_ms": 1000 + n as u64,
        "duration_api_ms": 900 + n as u64,
        "num_turns": 1 + t.block / 4,
        "result": summary,
        "stop_reason": "end_turn",
        "session_id": sid,
        "total_cost_usd": 0.0123 + n as f64 * 1e-6,
        "usage": usage,
        "modelUsage": {"claude-fake": {"inputTokens": 1200, "outputTokens": out_tokens, "costUSD": 0.0123}},
        "permission_denials": [],
        "errors": [],
        "uuid": u,
    })));
    t.out
}

/// The k-th kind of broken line, rotating: not JSON, half a JSON line,
/// a JSON frame of a type nobody knows, bytes that are not UTF-8, empty.
pub fn garbage(k: usize) -> Vec<u8> {
    match k % 5 {
        0 => b"this is not json at all {".to_vec(),
        1 => br#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_de"#.to_vec(),
        2 => br#"{"type":"mystery_frame","subtype":"from_the_future","payload":[1,2,3]}"#.to_vec(),
        3 => vec![0xff, 0xfe, 0x00, 0xc3, 0x28, b'{', b'}'],
        _ => Vec::new(),
    }
}

/// Count the frames a stream-json consumer must understand. The lower bound
/// a driver's parser is compared with (HOOK(desk-app): swap in
/// `desk_app::claude`'s line parser when it exists).
pub fn count_frames(lines: &[String]) -> (usize, usize) {
    let mut ok = 0;
    let mut text_deltas = 0;
    for l in lines {
        if let Ok(v) = serde_json::from_str::<Value>(l) {
            ok += 1;
            if v["event"]["delta"]["type"] == "text_delta" {
                text_deltas += 1;
            }
        }
    }
    (ok, text_deltas)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_count_and_valid_json() {
        for n in [1, 2, 3, 4, 7, 50, 333, 10_000] {
            let spec = StreamSpec {
                events: n,
                ..StreamSpec::default()
            };
            let lines = turn(&spec, 0, "2026-11-25");
            assert_eq!(lines.len(), n, "n={n}");
            let (ok, _) = count_frames(&lines);
            assert_eq!(ok, n);
            let last: Value = serde_json::from_str(&lines[n - 1]).unwrap_or_default();
            assert_eq!(last["type"], "result");
        }
    }

    #[test]
    fn has_what_the_driver_reads() {
        let lines = turn(
            &StreamSpec {
                events: 500,
                ..StreamSpec::default()
            },
            0,
            "2026-11-25",
        );
        let all = lines.join("\n");
        assert!(all.contains("\"text_delta\""));
        assert!(all.contains("\"tool_use\""));
        assert!(all.contains("\"tool_result\""));
        assert!(all.contains("\"total_cost_usd\""));
        let (_, deltas) = count_frames(&lines);
        assert!(deltas > 300, "{deltas}");
        assert!(init_line(1, "/v").contains("\"init\""));
    }

    #[test]
    fn deterministic() {
        let s = StreamSpec {
            events: 200,
            ..StreamSpec::default()
        };
        assert_eq!(turn(&s, 0, "d"), turn(&s, 0, "d"));
        assert_ne!(turn(&s, 0, "d"), turn(&s, 1, "d"));
    }

    #[test]
    fn garbage_is_not_a_frame() {
        for k in 0..5 {
            let g = garbage(k);
            let parsed = std::str::from_utf8(&g)
                .ok()
                .and_then(|s| serde_json::from_str::<Value>(s).ok());
            if let Some(v) = parsed {
                assert_eq!(v["type"], "mystery_frame");
            }
        }
    }
}
