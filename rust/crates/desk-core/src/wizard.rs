//! The first-run wizard's own logic (`src/renderer/setup.ts`), the six pages
//! of the macro-review MVP: which checks belong to which page, what
//! «skipped» means and how it survives a restart, and which task a page may
//! press by itself.
//!
//! Skipping is a real answer, not a silent one: the page is remembered in
//! `Settings.setup_skipped`, and every feature it weakens says so
//! ([`limitation_keys`]) instead of failing an hour later.

use crate::preflight::{worst_state, Check, CheckId, CheckState, Fix, TaskId};
use crate::settings::Settings;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A wizard page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepId {
    Vault,
    Claude,
    Tvmcp,
    Tradingview,
    Session,
    Schedule,
}

impl StepId {
    /// The pages in order: the folder everything lives in, the engine, the market
    /// server, the chart, the sessionid, the schedule.
    pub const ALL: [StepId; 6] = [
        StepId::Vault,
        StepId::Claude,
        StepId::Tvmcp,
        StepId::Tradingview,
        StepId::Session,
        StepId::Schedule,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            StepId::Vault => "vault",
            StepId::Claude => "claude",
            StepId::Tvmcp => "tvmcp",
            StepId::Tradingview => "tradingview",
            StepId::Session => "session",
            StepId::Schedule => "schedule",
        }
    }

    /// The id as the old app stored it; the legacy pages (codex, knowledge, official, files) are `None`.
    pub fn parse(s: &str) -> Option<StepId> {
        StepId::ALL.into_iter().find(|x| x.as_str() == s)
    }

    /// The checks shown on this page, in list order.
    pub fn checks(self) -> &'static [CheckId] {
        match self {
            StepId::Vault => &[CheckId::Vault],
            StepId::Claude => &[CheckId::Claude, CheckId::ClaudeAuth],
            StepId::Tvmcp => &[CheckId::Uv, CheckId::Tvmcp],
            StepId::Tradingview => &[CheckId::Tradingview, CheckId::Cdp],
            StepId::Session => &[CheckId::TvSession],
            StepId::Schedule => &[CheckId::Scheduler, CheckId::Timezone],
        }
    }
}

/// Unique and in page order.
pub(crate) fn order_steps(v: &mut Vec<StepId>) {
    let set: BTreeSet<StepId> = v.drain(..).collect();
    v.extend(StepId::ALL.into_iter().filter(|s| set.contains(s)));
}

/// `checksOfStep`: the page's checks the diagnosis produced, in list order
/// (`uv` exists only for a checkout of the owner's own).
pub fn checks_of_step(id: StepId, checks: &[Check]) -> Vec<&Check> {
    id.checks()
        .iter()
        .filter_map(|c| checks.iter().find(|x| x.id == *c))
        .collect()
}

/// `visibleStepIds`: the pages this diagnosis has something to say about;
/// before the first diagnosis, every page.
pub fn visible_steps(checks: &[Check]) -> Vec<StepId> {
    if checks.is_empty() {
        return StepId::ALL.to_vec();
    }
    StepId::ALL
        .into_iter()
        .filter(|s| !checks_of_step(*s, checks).is_empty())
        .collect()
}

/// `stepStatus`: the worst state on the page; a page with no checks is `Skipped` (TS: `unknown`).
pub fn step_state(id: StepId, checks: &[Check]) -> CheckState {
    let own: Vec<Check> = checks_of_step(id, checks).into_iter().cloned().collect();
    if own.is_empty() {
        CheckState::Skipped
    } else {
        worst_state(&own)
    }
}

/// `stepDone`: nothing on the page failed and nothing is unchecked; a warning is good enough to move on.
pub fn step_done(id: StepId, checks: &[Check]) -> bool {
    matches!(step_state(id, checks), CheckState::Ok | CheckState::Warn)
}

pub fn next_step(id: StepId, ids: &[StepId]) -> Option<StepId> {
    let i = ids.iter().position(|x| *x == id)?;
    ids.get(i + 1).copied()
}

pub fn prev_step(id: StepId, ids: &[StepId]) -> Option<StepId> {
    let i = ids.iter().position(|x| *x == id)?;
    i.checked_sub(1).map(|j| ids[j])
}

// ---------------------------------------------------------------- skipped pages

pub fn is_skipped(settings: &Settings, id: StepId) -> bool {
    settings.setup_skipped.contains(&id)
}

/// `applySkip`: add or remove a page, keeping the list unique and in page order.
pub fn apply_skip(skipped: &[StepId], id: StepId, on: bool) -> Vec<StepId> {
    let mut v: Vec<StepId> = skipped.iter().copied().filter(|s| *s != id).collect();
    if on {
        v.push(id);
    }
    order_steps(&mut v);
    v
}

/// `reconcileSkips`: a skipped page that is all green now is no longer
/// «skipped» — the warning would be a lie the owner could not find the switch for.
pub fn reconcile_skips(skipped: &[StepId], checks: &[Check]) -> Vec<StepId> {
    StepId::ALL
        .into_iter()
        .filter(|s| skipped.contains(s) && step_state(*s, checks) != CheckState::Ok)
        .collect()
}

/// `shouldOpenWizard`: by itself only before it was ever finished.
pub fn should_open_wizard(settings: &Settings) -> bool {
    settings.setup_completed_at.is_none()
}

/// `finishPatch`: the settings after «Готово» — when, and which pages were left unfinished.
pub fn finish(settings: &Settings, skipped: &[StepId], now: DateTime<Utc>) -> Settings {
    let mut s = settings.clone();
    s.setup_completed_at = Some(now);
    s.setup_skipped = skipped.to_vec();
    order_steps(&mut s.setup_skipped);
    s
}

/// `limitationKeys`: i18n keys of the «this is limited because you skipped it»
/// notes, in page order (`setup.limited.<page>`).
pub fn limitation_keys(settings: &Settings) -> Vec<String> {
    StepId::ALL
        .into_iter()
        .filter(|s| is_skipped(settings, *s))
        .map(|s| format!("setup.limited.{}", s.as_str()))
        .collect()
}

// ---------------------------------------------------------------- what a page starts by itself

/// `AUTO_TASKS`: what a page may press the moment it is shown — only laying
/// down what the app owns (its Python environment) and re-checking a secret
/// already stored. Never the vault (the folder is the owner's choice), the
/// Claude install or login (a browser, the owner's account), the schedule (it
/// acts in his name) or anything that removes something.
pub const AUTO_TASKS: [TaskId; 2] = [TaskId::ProvisionRuntime, TaskId::SessionCheck];

/// The state of a task as the wizard needs it (desk-app keeps the full one).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TaskRun {
    pub running: bool,
    /// `None` = never finished in this app run.
    pub ok: Option<bool>,
}

/// `autoFixes`: the ONE task of a page to start now, first in page order: in
/// [`AUTO_TASKS`], with a command (an empty one = the plan refused, e.g. a
/// development run without the payload), not running, never finished in this
/// app run, not started by this wizard session before. Nothing starts while
/// any task runs — the next one waits for the re-probe; a failed task is
/// retried by the owner, never in a loop.
pub fn auto_task(
    step: StepId,
    checks: &[Check],
    tasks: &BTreeMap<TaskId, TaskRun>,
    started: &BTreeSet<TaskId>,
    commands: Option<&BTreeMap<TaskId, String>>,
) -> Option<TaskId> {
    if tasks.values().any(|t| t.running) {
        return None;
    }
    checks_of_step(step, checks).into_iter().find_map(|c| {
        let Some(Fix::Task(task)) = c.fix else {
            return None;
        };
        if !AUTO_TASKS.contains(&task) || started.contains(&task) {
            return None;
        }
        if commands.is_some_and(|m| m.get(&task).is_some_and(String::is_empty)) {
            return None;
        }
        let st = tasks.get(&task).copied().unwrap_or_default();
        (!st.running && st.ok.is_none()).then_some(task)
    })
}

/// `stepTaskIds`: the tasks a page's checks carry as fixes.
pub fn step_task_ids(step: StepId, checks: &[Check]) -> Vec<TaskId> {
    checks_of_step(step, checks)
        .into_iter()
        .filter_map(|c| match c.fix {
            Some(Fix::Task(t)) => Some(t),
            _ => None,
        })
        .collect()
}

/// Which settings field a `PickFolder` fix writes: the check says it.
pub fn pick_field(check: CheckId) -> Option<PickField> {
    match check {
        CheckId::Vault => Some(PickField::Vault),
        CheckId::Tvmcp | CheckId::Uv => Some(PickField::TvmcpPath),
        _ => None,
    }
}

/// The two folders the app ever asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PickField {
    Vault,
    TvmcpPath,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn check(id: CheckId, state: CheckState) -> Check {
        Check {
            id,
            state,
            detail_key: format!("preflight.{id:?}.x"),
            vars: BTreeMap::new(),
            fix: None,
            fix_label_key: None,
            note_key: None,
        }
    }

    fn with_fix(id: CheckId, state: CheckState, fix: Fix) -> Check {
        Check {
            fix: Some(fix),
            ..check(id, state)
        }
    }

    fn all_ok() -> Vec<Check> {
        CheckId::ALL
            .into_iter()
            .map(|id| check(id, CheckState::Ok))
            .collect()
    }

    // ---- ported from setup.test.ts

    #[test]
    fn covers_every_check_exactly_once() {
        let mut covered: Vec<CheckId> = StepId::ALL
            .iter()
            .flat_map(|s| s.checks().iter().copied())
            .collect();
        let n = covered.len();
        covered.sort();
        covered.dedup();
        assert_eq!(covered.len(), n, "a check on two pages");
        let mut all = CheckId::ALL.to_vec();
        all.sort();
        assert_eq!(covered, all);
    }

    #[test]
    fn six_pages_in_order() {
        assert_eq!(
            StepId::ALL.map(StepId::as_str),
            [
                "vault",
                "claude",
                "tvmcp",
                "tradingview",
                "session",
                "schedule"
            ]
        );
        let ids = StepId::ALL;
        assert_eq!(next_step(StepId::Schedule, &ids), None);
        assert_eq!(prev_step(StepId::Vault, &ids), None);
        assert_eq!(prev_step(StepId::Tradingview, &ids), Some(StepId::Tvmcp));
        assert_eq!(next_step(StepId::Tradingview, &ids), Some(StepId::Session));
    }

    #[test]
    fn shows_every_page_before_the_first_diagnosis_and_only_pages_with_checks_after() {
        assert_eq!(visible_steps(&[]), StepId::ALL);
        let run = [
            check(CheckId::Vault, CheckState::Ok),
            check(CheckId::Tvmcp, CheckState::Ok),
            check(CheckId::Timezone, CheckState::Warn),
        ];
        assert_eq!(
            visible_steps(&run),
            [StepId::Vault, StepId::Tvmcp, StepId::Schedule]
        );
        assert_eq!(
            next_step(StepId::Vault, &visible_steps(&run)),
            Some(StepId::Tvmcp)
        );
    }

    #[test]
    fn a_page_shows_its_checks_in_order_skipping_what_the_run_did_not_produce() {
        assert_eq!(
            checks_of_step(StepId::Tvmcp, &all_ok())
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>(),
            [CheckId::Uv, CheckId::Tvmcp]
        );
        assert_eq!(
            checks_of_step(StepId::Tvmcp, &[check(CheckId::Tvmcp, CheckState::Ok)]).len(),
            1
        );
    }

    #[test]
    fn a_page_is_done_on_ok_or_warn_not_on_fail_or_unchecked() {
        let warn: Vec<Check> = all_ok()
            .into_iter()
            .map(|c| {
                if c.id == CheckId::Timezone {
                    check(c.id, CheckState::Warn)
                } else {
                    c
                }
            })
            .collect();
        assert_eq!(step_state(StepId::Schedule, &warn), CheckState::Warn);
        assert!(step_done(StepId::Schedule, &warn));
        let failed: Vec<Check> = all_ok()
            .into_iter()
            .map(|c| {
                if c.id == CheckId::Uv {
                    check(c.id, CheckState::Fail)
                } else {
                    c
                }
            })
            .collect();
        assert!(!step_done(StepId::Tvmcp, &failed));
        let unchecked: Vec<Check> = all_ok()
            .into_iter()
            .map(|c| {
                if c.id == CheckId::Scheduler {
                    check(c.id, CheckState::Skipped)
                } else {
                    c
                }
            })
            .collect();
        assert!(!step_done(StepId::Schedule, &unchecked));
        assert_eq!(step_state(StepId::Session, &[]), CheckState::Skipped);
    }

    #[test]
    fn a_skip_is_remembered_unique_and_in_page_order() {
        let mut s = apply_skip(&[], StepId::Tradingview, true);
        assert_eq!(s, [StepId::Tradingview]);
        s = apply_skip(&s, StepId::Tradingview, true);
        assert_eq!(s, [StepId::Tradingview]);
        s = apply_skip(&s, StepId::Claude, true);
        assert_eq!(s, [StepId::Claude, StepId::Tradingview]);
        assert_eq!(apply_skip(&s, StepId::Claude, false), [StepId::Tradingview]);
    }

    #[test]
    fn finishing_keeps_the_date_and_the_skips() {
        let now = DateTime::from_timestamp_millis(1_700_000_000_000).unwrap();
        let fresh = Settings::default();
        assert!(should_open_wizard(&fresh));
        let done = finish(
            &fresh,
            &[StepId::Schedule, StepId::Vault, StepId::Schedule],
            now,
        );
        assert_eq!(done.setup_completed_at, Some(now));
        assert_eq!(done.setup_skipped, [StepId::Vault, StepId::Schedule]);
        assert!(is_skipped(&done, StepId::Schedule) && !is_skipped(&done, StepId::Claude));
        assert!(!should_open_wizard(&done));
        // ...and reads back the same way after a restart.
        let back = Settings::from_json(&serde_json::to_vec(&done).unwrap());
        assert_eq!(back, done);
    }

    #[test]
    fn a_skipped_page_that_is_green_again_is_not_skipped() {
        let skipped = [StepId::Tradingview, StepId::Schedule];
        let checks: Vec<Check> = all_ok()
            .into_iter()
            .map(|c| {
                if c.id == CheckId::Timezone {
                    check(c.id, CheckState::Warn)
                } else {
                    c
                }
            })
            .collect();
        assert_eq!(reconcile_skips(&skipped, &checks), [StepId::Schedule]);
        assert!(reconcile_skips(&skipped, &all_ok()).is_empty());
    }

    #[test]
    fn skipped_pages_become_the_notes_the_features_show() {
        let mut s = Settings::default();
        assert!(limitation_keys(&s).is_empty());
        s.setup_skipped = vec![StepId::Claude, StepId::Schedule];
        assert_eq!(
            limitation_keys(&s),
            ["setup.limited.claude", "setup.limited.schedule"]
        );
    }

    // ---- what a page starts by itself

    fn task_checks() -> Vec<Check> {
        vec![
            with_fix(
                CheckId::Vault,
                CheckState::Fail,
                Fix::Task(TaskId::VaultCreate),
            ),
            with_fix(
                CheckId::Tvmcp,
                CheckState::Fail,
                Fix::Task(TaskId::ProvisionRuntime),
            ),
            with_fix(
                CheckId::ClaudeAuth,
                CheckState::Fail,
                Fix::Task(TaskId::ClaudeLogin),
            ),
            with_fix(
                CheckId::Claude,
                CheckState::Fail,
                Fix::Task(TaskId::ClaudeInstall),
            ),
            with_fix(
                CheckId::Scheduler,
                CheckState::Warn,
                Fix::Task(TaskId::SchedulerRegister),
            ),
            with_fix(
                CheckId::TvSession,
                CheckState::Warn,
                Fix::Task(TaskId::SessionCheck),
            ),
        ]
    }

    #[test]
    fn starts_the_runtime_and_the_session_check_never_the_vault_claude_or_schedule() {
        let none = BTreeMap::new();
        let fresh = BTreeSet::new();
        let c = task_checks();
        assert_eq!(
            auto_task(StepId::Tvmcp, &c, &none, &fresh, None),
            Some(TaskId::ProvisionRuntime)
        );
        assert_eq!(
            auto_task(StepId::Session, &c, &none, &fresh, None),
            Some(TaskId::SessionCheck)
        );
        for s in [
            StepId::Vault,
            StepId::Claude,
            StepId::Schedule,
            StepId::Tradingview,
        ] {
            assert_eq!(auto_task(s, &c, &none, &fresh, None), None, "{s:?}");
        }
        for t in [
            TaskId::VaultCreate,
            TaskId::ClaudeInstall,
            TaskId::ClaudeLogin,
            TaskId::SchedulerRegister,
            TaskId::SchedulerUnregister,
        ] {
            assert!(!AUTO_TASKS.contains(&t));
        }
    }

    #[test]
    fn starts_a_task_once_and_one_at_a_time() {
        let c = task_checks();
        let fresh = BTreeSet::new();
        let one = |run: TaskRun| BTreeMap::from([(TaskId::ProvisionRuntime, run)]);
        assert_eq!(
            auto_task(
                StepId::Tvmcp,
                &c,
                &one(TaskRun {
                    running: true,
                    ok: None
                }),
                &fresh,
                None
            ),
            None
        );
        assert_eq!(
            auto_task(
                StepId::Tvmcp,
                &c,
                &one(TaskRun {
                    running: false,
                    ok: Some(false)
                }),
                &fresh,
                None
            ),
            None
        );
        assert_eq!(
            auto_task(
                StepId::Tvmcp,
                &c,
                &one(TaskRun {
                    running: false,
                    ok: Some(true)
                }),
                &fresh,
                None
            ),
            None
        );
        assert_eq!(
            auto_task(
                StepId::Tvmcp,
                &c,
                &BTreeMap::new(),
                &BTreeSet::from([TaskId::ProvisionRuntime]),
                None
            ),
            None
        );
        assert_eq!(
            auto_task(StepId::Tvmcp, &c, &one(TaskRun::default()), &fresh, None),
            Some(TaskId::ProvisionRuntime)
        );
        // Something else running: nothing starts meanwhile.
        let other = BTreeMap::from([(
            TaskId::VaultCreate,
            TaskRun {
                running: true,
                ok: None,
            },
        )]);
        assert_eq!(auto_task(StepId::Tvmcp, &c, &other, &fresh, None), None);
        // A refused plan (empty command) never starts.
        let refused = BTreeMap::from([(TaskId::ProvisionRuntime, String::new())]);
        assert_eq!(
            auto_task(StepId::Tvmcp, &c, &BTreeMap::new(), &fresh, Some(&refused)),
            None
        );
        let planned = BTreeMap::from([(TaskId::ProvisionRuntime, "uv sync --frozen".to_string())]);
        assert_eq!(
            auto_task(StepId::Tvmcp, &c, &BTreeMap::new(), &fresh, Some(&planned)),
            Some(TaskId::ProvisionRuntime)
        );
    }

    #[test]
    fn page_task_ids_and_pick_fields() {
        let c = task_checks();
        assert_eq!(
            step_task_ids(StepId::Claude, &c),
            [TaskId::ClaudeInstall, TaskId::ClaudeLogin]
        );
        let pick = [with_fix(CheckId::Vault, CheckState::Fail, Fix::PickFolder)];
        assert!(step_task_ids(StepId::Vault, &pick).is_empty());
        assert_eq!(pick_field(CheckId::Vault), Some(PickField::Vault));
        assert_eq!(pick_field(CheckId::Tvmcp), Some(PickField::TvmcpPath));
        assert_eq!(pick_field(CheckId::Timezone), None);
    }
}
