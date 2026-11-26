//! Start-up switches for A/B runs on the owner's machine (`DESK_OPT`): a
//! comma list of names, each flipping one choice away from the default, so
//! one session can trace every variant with the same exe.
//!
//! | name | default | flipped |
//! | --- | --- | --- |
//! | `vsync0` | vsync on | swap without waiting for vblank |
//! | `app_sync` | desk-app starts on a helper thread; the creator returns at once | start it inside the creator (the b8d06cb way) |
//! | `app_late` | desk-app starts at once | start it after the first frame is presented |
//! | `shots_full` | screenshots downscaled to ≤ 1280 px wide (+ mipmaps), full size only in the magnifier | full size + mipmaps everywhere, kept for the whole session |
//! | `no_inter` | Inter Regular + SemiBold | egui's own fonts only |
//! | `shot_mips` | previews without mipmaps (the box downscale already filters) | previews with mipmaps |
//! | `no_shots` | screenshots shown | never loaded (diagnostic: what do they cost?) |
//! | `shot_small` | previews ≤ 1280 px wide | ≤ 640 px (diagnostic: does memory follow texture size?) |
//!
//! Unknown names are reported on stderr and ignored.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opts {
    pub vsync: bool,
    pub app_async: bool,
    pub app_late: bool,
    pub shots_full: bool,
    pub inter: bool,
    pub shot_mips: bool,
    pub shots: bool,
    pub shot_small: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            vsync: true,
            app_async: true,
            app_late: false,
            shots_full: false,
            inter: true,
            shot_mips: false,
            shots: true,
            shot_small: false,
        }
    }
}

impl Opts {
    /// Parse a `DESK_OPT` value; returns the options and the unknown names.
    pub fn parse(s: &str) -> (Opts, Vec<String>) {
        let mut o = Opts::default();
        let mut unknown = Vec::new();
        for name in s.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            match name {
                "vsync0" => o.vsync = false,
                "app_sync" => o.app_async = false,
                "app_late" => o.app_late = true,
                "shots_full" => o.shots_full = true,
                "no_inter" => o.inter = false,
                "shot_mips" => o.shot_mips = true,
                "no_shots" => o.shots = false,
                "shot_small" => o.shot_small = true,
                other => unknown.push(other.to_owned()),
            }
        }
        (o, unknown)
    }

    pub fn from_env() -> Opts {
        let raw = std::env::var("DESK_OPT").unwrap_or_default();
        let (o, unknown) = Opts::parse(&raw);
        for u in unknown {
            eprintln!("strategy-desk: DESK_OPT: unknown switch {u:?} (ignored)");
        }
        o
    }

    /// The flipped switches, for the trace (`""` = defaults).
    pub fn label(&self) -> String {
        let d = Opts::default();
        let mut v = Vec::new();
        if self.vsync != d.vsync {
            v.push("vsync0");
        }
        if self.app_async != d.app_async {
            v.push("app_sync");
        }
        if self.app_late != d.app_late {
            v.push("app_late");
        }
        if self.shots_full != d.shots_full {
            v.push("shots_full");
        }
        if self.inter != d.inter {
            v.push("no_inter");
        }
        if self.shot_mips != d.shot_mips {
            v.push("shot_mips");
        }
        if self.shots != d.shots {
            v.push("no_shots");
        }
        if self.shot_small != d.shot_small {
            v.push("shot_small");
        }
        v.join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_flips() {
        assert_eq!(Opts::parse(""), (Opts::default(), vec![]));
        let (o, u) = Opts::parse(" vsync0, app_late ,bogus,no_inter");
        assert!(!o.vsync && o.app_late && !o.inter && o.app_async && !o.shots_full);
        assert_eq!(u, ["bogus"]);
        assert_eq!(o.label(), "vsync0,app_late,no_inter");
        assert_eq!(Opts::default().label(), "");
    }
}
