//! A minimal argument reader, so the bins need no clap.
//!
//! `fake-claude` has to swallow whatever flags the real `claude` gets
//! (`-p --output-format stream-json --verbose --resume <id> ...`), which a
//! strict parser would reject; the other bins call `finish()` to refuse
//! anything they did not take.

use anyhow::{anyhow, bail, Result};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct Args {
    items: Vec<String>,
}

impl Args {
    pub fn from_env() -> Self {
        Args {
            items: std::env::args().skip(1).collect(),
        }
    }

    pub fn from_vec<S: Into<String>>(items: Vec<S>) -> Self {
        Args {
            items: items.into_iter().map(Into::into).collect(),
        }
    }

    /// `--name` present? Removes it.
    pub fn flag(&mut self, name: &str) -> bool {
        if let Some(i) = self.items.iter().position(|a| a == name) {
            self.items.remove(i);
            true
        } else {
            false
        }
    }

    /// `--name value` or `--name=value`. Removes both.
    pub fn value(&mut self, name: &str) -> Result<Option<String>> {
        let eq = format!("{name}=");
        if let Some(i) = self.items.iter().position(|a| a.starts_with(&eq)) {
            let v = self.items.remove(i)[eq.len()..].to_string();
            return Ok(Some(v));
        }
        if let Some(i) = self.items.iter().position(|a| a == name) {
            if i + 1 >= self.items.len() {
                bail!("{name} needs a value");
            }
            self.items.remove(i);
            return Ok(Some(self.items.remove(i)));
        }
        Ok(None)
    }

    pub fn parsed<T: FromStr>(&mut self, name: &str) -> Result<Option<T>>
    where
        T::Err: std::fmt::Display,
    {
        match self.value(name)? {
            None => Ok(None),
            Some(v) => v
                .parse::<T>()
                .map(Some)
                .map_err(|e| anyhow!("{name} {v:?}: {e}")),
        }
    }

    /// Like `parsed`, falling back to an environment variable, then `default`.
    pub fn parsed_or_env<T: FromStr>(&mut self, name: &str, env: &str, default: T) -> Result<T>
    where
        T::Err: std::fmt::Display,
    {
        if let Some(v) = self.parsed(name)? {
            return Ok(v);
        }
        match std::env::var(env) {
            Ok(v) if !v.trim().is_empty() => v
                .trim()
                .parse::<T>()
                .map_err(|e| anyhow!("{env}={v:?}: {e}")),
            _ => Ok(default),
        }
    }

    /// The next positional argument (anything not starting with `-`).
    pub fn positional(&mut self) -> Option<String> {
        let i = self.items.iter().position(|a| !a.starts_with('-'))?;
        Some(self.items.remove(i))
    }

    pub fn rest(&self) -> &[String] {
        &self.items
    }

    /// Fail on anything left over.
    pub fn finish(self) -> Result<()> {
        if self.items.is_empty() {
            Ok(())
        } else {
            bail!("unknown arguments: {}", self.items.join(" "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_flags_values_positionals() {
        let mut a = Args::from_vec(vec!["out", "--scale", "10", "--seed=7", "--fat"]);
        assert!(a.flag("--fat"));
        assert!(!a.flag("--fat"));
        assert_eq!(a.parsed::<f64>("--scale").ok().flatten(), Some(10.0));
        assert_eq!(a.parsed::<u64>("--seed").ok().flatten(), Some(7));
        assert_eq!(a.positional().as_deref(), Some("out"));
        assert!(a.finish().is_ok());
    }

    #[test]
    fn rejects_leftovers_and_missing_values() {
        let mut a = Args::from_vec(vec!["--days"]);
        assert!(a.value("--days").is_err());
        let a = Args::from_vec(vec!["--what"]);
        assert!(a.finish().is_err());
        let mut a = Args::from_vec(vec!["--n", "x"]);
        assert!(a.parsed::<u32>("--n").is_err());
    }
}
