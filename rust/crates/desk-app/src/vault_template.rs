//! Laying the vault template down in the owner's folder (the `vault_create`
//! task) — port of `src/main/vault-template.ts`.
//!
//! * an empty (or absent) folder gets everything;
//! * a folder that already IS a vault, even a partial one, gets only what it
//!   lacks. NOTHING existing is overwritten (CONTRACTS: «только недостающие
//!   файлы, ничего не перезаписывать» — stricter than the TS, which rewrote
//!   `template.json`);
//! * a non-empty folder that is not a vault is refused.
//!
//! The template is compiled in with the `vault-template` feature from
//! `resources/vault-template` (staged by `scripts/stage-vault.mjs`, stage 7);
//! without it the built-in template is empty and the task refuses.

use std::collections::BTreeSet;
use std::io::{self, Write};
use std::path::Path;

/// The three files that make a folder a vault (same as the preflight's check).
pub const VAULT_MARKERS: [&str; 3] = [
    ".claude/skills/macro-review/SKILL.md",
    "indices-kb/tools/scale.py",
    "indices-kb/config/scale.json",
];

/// One entry of a template: `/`-separated path; `None` bytes = a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry<'a> {
    pub rel: String,
    pub bytes: Option<&'a [u8]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan<'a> {
    Refuse,
    Copy {
        fresh: bool,
        entries: Vec<Entry<'a>>,
        skipped: usize,
    },
}

/// `planVaultCopy`: what to write, given every relative path already there.
pub fn plan<'a>(template: &[Entry<'a>], existing: &BTreeSet<String>) -> Plan<'a> {
    if existing.is_empty() {
        return Plan::Copy {
            fresh: true,
            entries: template.to_vec(),
            skipped: 0,
        };
    }
    if !VAULT_MARKERS.iter().any(|m| existing.contains(*m)) {
        return Plan::Refuse;
    }
    let entries: Vec<Entry<'a>> = template
        .iter()
        .filter(|e| !existing.contains(&e.rel))
        .cloned()
        .collect();
    Plan::Copy {
        fresh: false,
        skipped: template.len() - entries.len(),
        entries,
    }
}

/// Every relative path under `root` (files and folders), `/`-separated.
pub fn existing(root: &Path) -> BTreeSet<String> {
    fn walk(dir: &Path, rel: &str, out: &mut BTreeSet<String>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().into_owned();
            let r = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
            out.insert(r.clone());
            if is_dir {
                walk(&e.path(), &r, out);
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(root, "", &mut out);
    out
}

/// Lay `template` into `target`; `push` gets one line per file written.
/// Returns how many files were written.
pub fn copy(
    template: &[Entry<'_>],
    target: &Path,
    push: &mut dyn FnMut(String),
) -> io::Result<usize> {
    if template.is_empty() {
        return Err(io::Error::other("this build carries no vault template"));
    }
    let have = existing(target);
    let (fresh, entries, skipped) = match plan(template, &have) {
        Plan::Refuse => {
            return Err(io::Error::other(format!(
                "refused: {} is not empty and not a vault",
                target.display()
            )));
        }
        Plan::Copy {
            fresh,
            entries,
            skipped,
        } => (fresh, entries, skipped),
    };
    push(if fresh {
        format!("creating a vault in {}", target.display())
    } else {
        format!(
            "completing the vault in {} ({skipped} existing entries kept)",
            target.display()
        )
    });
    std::fs::create_dir_all(target)?;
    let mut files = 0;
    for e in entries {
        let to = e
            .rel
            .split('/')
            .fold(target.to_path_buf(), |p, s| p.join(s));
        match e.bytes {
            None => std::fs::create_dir_all(&to)?,
            Some(bytes) => {
                if let Some(dir) = to.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                // create_new: a file that appeared since the listing is kept.
                match std::fs::File::create_new(&to) {
                    Ok(mut f) => {
                        f.write_all(bytes)?;
                        files += 1;
                        push(format!("+ {}", e.rel));
                    }
                    Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(err) => return Err(err),
                }
            }
        }
    }
    push(format!("{files} files written"));
    Ok(files)
}

#[cfg(feature = "vault-template")]
static TEMPLATE: include_dir::Dir<'static> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../../resources/vault-template");

/// The template compiled into this build (empty without the feature).
pub fn builtin() -> Vec<Entry<'static>> {
    #[cfg(feature = "vault-template")]
    {
        fn walk(d: &'static include_dir::Dir<'static>, out: &mut Vec<Entry<'static>>) {
            for e in d.entries() {
                let rel = e.path().to_string_lossy().replace('\\', "/");
                match e {
                    include_dir::DirEntry::Dir(sub) => {
                        out.push(Entry { rel, bytes: None });
                        walk(sub, out);
                    }
                    include_dir::DirEntry::File(f) => out.push(Entry {
                        rel,
                        bytes: Some(f.contents()),
                    }),
                }
            }
        }
        let mut out = Vec::new();
        walk(&TEMPLATE, &mut out);
        out
    }
    #[cfg(not(feature = "vault-template"))]
    {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template() -> Vec<Entry<'static>> {
        vec![
            Entry {
                rel: ".claude".into(),
                bytes: None,
            },
            Entry {
                rel: ".claude/skills/macro-review/SKILL.md".into(),
                bytes: Some(b"skill"),
            },
            Entry {
                rel: "indices-kb/state".into(),
                bytes: None,
            },
            Entry {
                rel: "indices-kb/config/scale.json".into(),
                bytes: Some(b"{}"),
            },
            Entry {
                rel: "template.json".into(),
                bytes: Some(b"{\"version\":2}"),
            },
        ]
    }

    #[test]
    fn an_empty_folder_gets_everything() {
        let d = tempfile::tempdir().unwrap();
        let target = d.path().join("vault");
        let mut lines = Vec::new();
        assert_eq!(
            copy(&template(), &target, &mut |l| lines.push(l)).unwrap(),
            3
        );
        assert!(target.join("indices-kb/state").is_dir());
        assert_eq!(
            std::fs::read(target.join("indices-kb/config/scale.json")).unwrap(),
            b"{}"
        );
        assert!(lines[0].starts_with("creating"));
    }

    #[test]
    fn a_vault_gets_only_what_it_lacks_and_nothing_is_overwritten() {
        let d = tempfile::tempdir().unwrap();
        let v = d.path();
        std::fs::create_dir_all(v.join("indices-kb/config")).unwrap();
        std::fs::write(v.join("indices-kb/config/scale.json"), b"{\"mine\":true}").unwrap();
        std::fs::write(v.join("template.json"), b"{\"version\":1}").unwrap();
        let mut lines = Vec::new();
        assert_eq!(copy(&template(), v, &mut |l| lines.push(l)).unwrap(), 1);
        assert_eq!(
            std::fs::read(v.join("indices-kb/config/scale.json")).unwrap(),
            b"{\"mine\":true}"
        );
        assert_eq!(
            std::fs::read(v.join("template.json")).unwrap(),
            b"{\"version\":1}",
            "not even the marker"
        );
        assert!(v.join(".claude/skills/macro-review/SKILL.md").is_file());
        assert!(lines[0].starts_with("completing"));
    }

    #[test]
    fn a_stranger_folder_is_refused_and_an_empty_template_too() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("thesis.docx"), b"x").unwrap();
        assert!(copy(&template(), d.path(), &mut |_| {}).is_err());
        assert_eq!(existing(d.path()).len(), 1, "nothing written");
        assert!(copy(&[], &d.path().join("new"), &mut |_| {}).is_err());
    }

    #[test]
    fn without_the_feature_the_builtin_template_is_empty() {
        if cfg!(not(feature = "vault-template")) {
            assert!(builtin().is_empty());
        }
    }
}
