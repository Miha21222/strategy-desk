//! Chats on disk: one `<id>.json` per chat (meta + transcript) under
//! `<data dir>/strategy-desk-rs/chats/` — `%APPDATA%` on Windows,
//! `$XDG_DATA_HOME` (or `~/.local/share`) on Linux. Written atomically with
//! the vault's helper; the Electron app's `chats.json` is not read (CONTRACTS).

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::model::{ChatId, ChatMeta, Transcript};
use crate::vault_write::atomic_write;

/// The app's folder name inside the platform data dir.
pub const APP_DIR: &str = "strategy-desk-rs";
const FORMAT: u32 = 1;

/// `<platform data dir>/strategy-desk-rs`, if the platform has one.
pub fn default_data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(APP_DIR))
}

/// Ids become file names: only `[a-z0-9-]`, 1–64 chars.
pub fn is_valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// A new id: the creation time to the millisecond, a per-process sequence
/// number and the pid, so two desks started at once still differ.
pub fn new_id(now: chrono::DateTime<chrono::Utc>, seq: u64) -> ChatId {
    format!(
        "{}-{:x}-{:x}",
        now.format("%Y%m%d-%H%M%S%3f"),
        std::process::id(),
        seq
    )
}

#[derive(Serialize)]
struct FileOut<'a> {
    v: u32,
    meta: &'a ChatMeta,
    transcript: &'a Transcript,
}

#[derive(Deserialize)]
struct FileIn {
    meta: ChatMeta,
    #[serde(default)]
    transcript: Transcript,
}

/// Only the meta; the transcript is skipped without being built.
#[derive(Deserialize)]
struct MetaIn {
    meta: ChatMeta,
}

#[derive(Debug, Clone)]
pub struct ChatStore {
    dir: PathBuf,
}

impl ChatStore {
    /// The store under `<data_dir>/chats`.
    pub fn new(data_dir: &Path) -> Self {
        Self {
            dir: data_dir.join("chats"),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, id: &str) -> Option<PathBuf> {
        is_valid_id(id).then(|| self.dir.join(format!("{id}.json")))
    }

    /// The text of a chat's file.
    pub fn encode(meta: &ChatMeta, transcript: &Transcript) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&FileOut {
            v: FORMAT,
            meta,
            transcript,
        })
    }

    /// Every readable chat, newest first. A broken or foreign file is skipped
    /// (logged), never fatal: one bad file must not hide the others.
    pub fn list(&self) -> io::Result<Vec<ChatMeta>> {
        let rd = match std::fs::read_dir(&self.dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let mut out = Vec::new();
        for e in rd.filter_map(Result::ok) {
            let name = e.file_name();
            let Some(id) = name.to_str().and_then(|n| n.strip_suffix(".json")) else {
                continue;
            };
            if !is_valid_id(id) {
                continue;
            }
            let parsed = std::fs::read(e.path())
                .ok()
                .and_then(|b| serde_json::from_slice::<MetaIn>(&b).ok());
            match parsed {
                Some(m) if m.meta.id == id => out.push(m.meta),
                _ => tracing::warn!(chat = id, "chats: skipped an unreadable chat file"),
            }
        }
        out.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        Ok(out)
    }

    /// One chat with its transcript. A live answer saved mid-stream is
    /// closed: after a restart nothing is being written.
    pub fn load(&self, id: &str) -> io::Result<Option<(ChatMeta, Transcript)>> {
        let Some(p) = self.path(id) else {
            return Ok(None);
        };
        let bytes = match std::fs::read(p) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let f: FileIn = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        if f.meta.id != id {
            return Ok(None);
        }
        let mut t = f.transcript;
        t.close_live();
        Ok(Some((f.meta, t)))
    }

    pub fn save(&self, id: &str, bytes: &[u8]) -> io::Result<()> {
        let Some(_) = self.path(id) else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "bad chat id"));
        };
        std::fs::create_dir_all(&self.dir)?;
        atomic_write(&self.dir, &format!("{id}.json"), bytes).map(drop)
    }

    pub fn delete(&self, id: &str) -> io::Result<()> {
        let Some(p) = self.path(id) else {
            return Ok(());
        };
        match std::fs::remove_file(p) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::model::{ChatItem, ChatKind};
    use chrono::{TimeZone, Utc};

    fn meta(id: &str, minute: u32) -> ChatMeta {
        let at = Utc.with_ymd_and_hms(2026, 11, 26, 7, minute, 0).unwrap();
        ChatMeta {
            id: id.into(),
            kind: ChatKind::Macro,
            title: "Обзор дня 26.11".into(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 11, 26),
            created_at: at,
            updated_at: at,
            session_id: Some("s-1".into()),
            busy: true,
            cost_usd: 0.5,
        }
    }

    #[test]
    fn saves_lists_and_loads() {
        let d = tempfile::tempdir().unwrap();
        let s = ChatStore::new(d.path());
        assert!(
            s.list().unwrap().is_empty(),
            "no folder yet is an empty list"
        );
        let mut t = Transcript::default();
        t.push_user("привет", Utc::now());
        t.items.push(ChatItem::Assistant {
            text: "полу".into(),
            at: Utc::now(),
            live: true,
        });
        s.save("a-1", &ChatStore::encode(&meta("a-1", 1), &t).unwrap())
            .unwrap();
        s.save(
            "b-2",
            &ChatStore::encode(&meta("b-2", 5), &Transcript::default()).unwrap(),
        )
        .unwrap();
        std::fs::write(s.dir().join("broken.json"), b"{not json").unwrap();
        std::fs::write(s.dir().join("Bad Name.json"), b"{}").unwrap();
        let list = s.list().unwrap();
        assert_eq!(
            list.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["b-2", "a-1"],
            "newest first, junk skipped"
        );
        assert!(!list[0].busy, "busy is never stored");
        let (m, t) = s.load("a-1").unwrap().unwrap();
        assert_eq!(m.session_id.as_deref(), Some("s-1"));
        assert!(
            matches!(t.items[1], ChatItem::Assistant { live: false, .. }),
            "a saved live answer is closed"
        );
        assert!(s.load("nope").unwrap().is_none());
        assert!(s.load("../x").unwrap().is_none());
        s.delete("a-1").unwrap();
        s.delete("a-1").unwrap();
        assert_eq!(s.list().unwrap().len(), 1);
    }

    #[test]
    fn ids_are_file_safe() {
        let id = new_id(Utc.with_ymd_and_hms(2026, 11, 26, 7, 0, 0).unwrap(), 42);
        assert!(is_valid_id(&id), "{id}");
        assert_ne!(
            id,
            new_id(Utc.with_ymd_and_hms(2026, 11, 26, 7, 0, 0).unwrap(), 43)
        );
        for bad in ["", "A", "a/b", "..", "a.b", &"a".repeat(65)] {
            assert!(!is_valid_id(bad), "{bad}");
        }
        let d = tempfile::tempdir().unwrap();
        assert!(ChatStore::new(d.path()).save("../evil", b"{}").is_err());
    }
}
