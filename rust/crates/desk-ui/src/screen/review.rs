//! The 20:00 day review (`ReviewView` of `MacroDesk.tsx`): «Твой день»,
//! «Как прошёл день», per window «Кто был прав», the week / «О тебе», ideas,
//! lessons; under «Подробнее» factor hits, 30-day factor stats and the
//! lessons log.

use chrono::NaiveDate;
use desk_app::{Command, Snapshot};
use desk_core::normalize::human_hit;
use desk_core::vault::{
    ByRule, DayAnswerRegion, DayAnswerRegions, DayAnswers, HumanRead, ReviewAgentRegion,
    ReviewRegion, Shots, TradeTaken,
};
use desk_core::zones::{zone_of, ZoneKey, Zones};
use desk_core::{Day, Region};
use eframe::egui::{self, Ui};

use super::kit::{self, bold, txt, CardKind, Tone};
use super::region::{human_side_key, more_frame, no_data};
use super::shots::{self, ShotTextures};
use crate::gauge::{zone_label, Gauge};
use crate::i18n::{t, tf};
use crate::theme::{space, text, Palette};

const REGIONS: [Region; 2] = [Region::Eu, Region::Us];

/// The «Твой день» form while it is being filled in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnswersUi {
    pub editing: bool,
    pub regions: DayAnswerRegions,
    pub note: String,
    /// `date + at` of the answers the form was filled from.
    pub seeded: Option<String>,
}

/// What the view asks of the screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewAction {
    None,
    OpenShot(Region, usize),
    OpenNote(String),
}

pub struct ReviewCtx<'a> {
    pub p: &'a Palette,
    pub day: &'a Day,
    pub date: NaiveDate,
    pub past: bool,
    /// desk-core's decision for the evening (`ask_day`).
    pub vis: desk_core::windows::Visibility,
}

fn trade_key(x: TradeTaken) -> &'static str {
    match x {
        TradeTaken::None => "macro.tradeNone",
        TradeTaken::Long => "macro.trade.long",
        TradeTaken::Short => "macro.trade.short",
    }
}

fn by_rule_key(x: ByRule) -> &'static str {
    match x {
        ByRule::Yes => "macro.byRule.yes",
        ByRule::No => "macro.byRule.no",
        ByRule::Unsure => "macro.byRule.unsure",
    }
}

fn region_answer(r: &DayAnswerRegions, region: Region) -> Option<&DayAnswerRegion> {
    r.get(region)
}

fn region_answer_mut(r: &mut DayAnswerRegions, region: Region) -> &mut Option<DayAnswerRegion> {
    match region {
        Region::Eu => &mut r.eu,
        Region::Us => &mut r.us,
    }
}

/// The hit that counts: after the discussion when there was one.
pub fn counted_hit(rv: &ReviewRegion) -> Option<&str> {
    let sc = rv.score.as_ref()?;
    if rv.after.is_some() {
        sc.after_hit.as_deref()
    } else {
        sc.needle_hit.as_deref()
    }
}

pub fn show(
    ui: &mut Ui,
    cx: &ReviewCtx<'_>,
    st: &mut AnswersUi,
    tex: &mut ShotTextures,
    snap: &Snapshot,
    out: &mut Vec<Command>,
) -> ReviewAction {
    let p = cx.p;
    let day = cx.day;
    let mech = day.review.mech.as_deref();
    let agent = day.review.agent.as_deref();
    let answers = day.answers.as_ref();
    // The owner's two questions come first (note 23): on the day itself even
    // before the agent's review exists; on a past day only when he answered.
    if cx.vis.ask_day {
        answers_card(ui, cx, answers, st, out);
    }
    if mech.is_none() && agent.is_none() {
        // QA Q25: on a past day «появится в 20:00» is wrong; say it did not happen.
        no_data(
            ui,
            p,
            t(if cx.past {
                "desk.noReviewPast"
            } else {
                "macro.noReview"
            }),
        );
        return ReviewAction::None;
    }
    let mut action = ReviewAction::None;
    if let Some(sum) = agent.and_then(|a| a.summary.as_ref()) {
        let note = day.review.note.as_ref();
        let mut open = false;
        let extra = |ui: &mut Ui| {
            if note.is_some() && kit::link(ui, p, t("macro.openNote")).clicked() {
                open = true;
            }
        };
        kit::card_ex(
            ui,
            p,
            Some(t("macro.reviewSummary")),
            CardKind::Plain,
            extra,
            |ui| {
                kit::para(ui, txt(sum, text::LG, p.fg));
            },
        );
        if let (true, Some(n)) = (open, note) {
            action = ReviewAction::OpenNote(n.clone());
        }
    }
    // Two windows side by side when there is room.
    kit::grid2_cells(ui, |ui, i| {
        let r = REGIONS[i];
        let rv = mech.and_then(|m| m.regions.get(r));
        let reflection = agent.and_then(|a| a.regions.get(r));
        let shots = match r {
            Region::Eu => day.review.shots.eu.as_ref(),
            Region::Us => day.review.shots.us.as_ref(),
        };
        let human = day.region(r).human.as_ref();
        let answer = answers.and_then(|a| region_answer(&a.regions, r));
        if let Some(k) = region_card(
            ui, p, r, rv, reflection, &day.zones, shots, human, answer, cx.past, tex, snap, out,
        ) {
            action = ReviewAction::OpenShot(r, k);
        }
    });

    // Note 23 §3: on a Friday the day's result becomes the week's; «О тебе»
    // rides along, else it stands alone.
    let date = cx.date.format("%Y-%m-%d").to_string();
    let trader = day.trader_lines();
    let friday = desk_core::names::is_friday(&date)
        && day.week.as_ref().is_some_and(|w| !w.lines.is_empty());
    if !(friday && super::memory::week(ui, p, day.week.as_ref(), trader)) {
        super::memory::trader(ui, p, trader);
    }
    super::memory::ideas(ui, p, day.ideas.as_ref());
    if let Some(lessons) = agent.map(|a| &a.lessons).filter(|l| !l.is_empty()) {
        kit::card(
            ui,
            p,
            Some(t("macro.lessonsToday")),
            CardKind::Plain,
            |ui| {
                for (i, x) in lessons.iter().enumerate() {
                    let r = if i == 0 {
                        bold(x, text::LG, p.fg)
                    } else {
                        txt(x, text::BASE, p.fg_muted)
                    };
                    kit::para(ui, r);
                }
            },
        );
    }

    more_frame(
        ui,
        p,
        t("macro.moreReview"),
        egui::Id::NULL.with("more-review"),
        |ui| {
            for r in REGIONS {
                factor_hits(ui, p, r, mech.and_then(|m| m.regions.get(r)), day);
            }
            stats_card(ui, p, day);
            if let Some(ideas) = agent.map(|a| &a.weight_ideas).filter(|w| !w.is_empty()) {
                kit::card(ui, p, Some(t("macro.weightIdeas")), CardKind::Plain, |ui| {
                    kit::bullets(ui, p, ideas, text::MD)
                });
            }
            if !day.lessons.is_empty() {
                kit::card(ui, p, Some(t("macro.lessonsLog")), CardKind::Plain, |ui| {
                    kit::bullets(ui, p, &day.lessons, text::SM)
                });
            }
        },
    );
    action
}

// ---------------------------------------------------------------- «Твой день»

fn answers_card(
    ui: &mut Ui,
    cx: &ReviewCtx<'_>,
    answers: Option<&DayAnswers>,
    st: &mut AnswersUi,
    out: &mut Vec<Command>,
) {
    let p = cx.p;
    // Re-seed the form when the file on disk changed (or another day opened).
    let seed = answers.map(|a| format!("{}|{}", a.date, a.at));
    if st.seeded != seed || (seed.is_none() && st.seeded.is_none() && !st.editing) {
        st.editing = answers.is_none();
        st.regions = answers.map(|a| a.regions.clone()).unwrap_or_default();
        st.note = answers.map(|a| a.note.clone()).unwrap_or_default();
        st.seeded = seed;
    }
    if let (false, Some(a)) = (st.editing, answers) {
        kit::card(ui, p, None, CardKind::Plain, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = space::S8;
                for r in REGIONS {
                    let x = region_answer(&a.regions, r);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = space::S3;
                        ui.label(txt(
                            format!("{}:", t(&format!("macro.window.{}", r.as_str()))),
                            text::BASE,
                            p.fg,
                        ));
                        let trade = x.map_or(TradeTaken::None, |x| x.trade);
                        ui.label(bold(t(trade_key(trade)), text::BASE, p.fg));
                        if let (Some(br), false) =
                            (x.and_then(|x| x.by_rule), trade == TradeTaken::None)
                        {
                            ui.label(txt(
                                format!("· {}", t(by_rule_key(br))),
                                text::BASE,
                                p.fg_muted,
                            ));
                        }
                    });
                }
                if kit::link(ui, p, t("macro.edit")).clicked() {
                    st.editing = true;
                }
            });
            if !a.note.is_empty() {
                kit::para(
                    ui,
                    txt(format!("«{}»", a.note), text::BASE, p.fg_muted).italics(),
                );
            }
        });
        return;
    }
    kit::card(ui, p, Some(t("macro.yourDayTitle")), CardKind::Ask, |ui| {
        kit::para(ui, txt(t("macro.yourDayQ1"), text::LG, p.fg));
        for r in REGIONS {
            ui.horizontal_wrapped(|ui| {
                ui.add_sized(
                    [72.0, 20.0],
                    egui::Label::new(bold(
                        t(&format!("macro.window.{}", r.as_str())),
                        text::BASE,
                        p.fg,
                    )),
                );
                let cur = region_answer(&st.regions, r).cloned();
                let trades = [TradeTaken::None, TradeTaken::Long, TradeTaken::Short];
                let items = [
                    (t(trade_key(TradeTaken::None)), Tone::Accent),
                    (t(trade_key(TradeTaken::Long)), Tone::Good),
                    (t(trade_key(TradeTaken::Short)), Tone::Bad),
                ];
                let sel = cur
                    .as_ref()
                    .and_then(|c| trades.iter().position(|x| *x == c.trade));
                if let Some(i) = kit::seg(ui, p, &items, sel, false) {
                    let slot = region_answer_mut(&mut st.regions, r);
                    let by_rule = slot.as_ref().and_then(|s| s.by_rule);
                    *slot = Some(DayAnswerRegion {
                        trade: trades[i],
                        by_rule,
                    });
                }
                if let Some(c) = cur.filter(|c| c.trade != TradeTaken::None) {
                    let rules = [ByRule::Yes, ByRule::No, ByRule::Unsure];
                    let items = rules.map(|x| (t(by_rule_key(x)), Tone::Accent));
                    let sel = c.by_rule.and_then(|b| rules.iter().position(|x| *x == b));
                    if let Some(i) = kit::seg(ui, p, &items, sel, false) {
                        *region_answer_mut(&mut st.regions, r) = Some(DayAnswerRegion {
                            trade: c.trade,
                            by_rule: Some(rules[i]),
                        });
                    }
                }
            });
        }
        let q2 = kit::para(ui, txt(t("macro.yourDayQ2"), text::BASE, p.fg));
        let edit = egui::TextEdit::singleline(&mut st.note)
            .char_limit(300)
            .desired_width(f32::INFINITY)
            .hint_text(t("macro.yourDayPlaceholder"));
        ui.add(edit).labelled_by(q2.id);
        let complete = REGIONS
            .iter()
            .all(|r| region_answer(&st.regions, *r).is_some());
        ui.horizontal(|ui| {
            if kit::button(ui, p, t("macro.save"), true, complete).clicked() {
                out.push(Command::WriteDayAnswers {
                    date: cx.date,
                    regions: st.regions.clone(),
                    note: st.note.clone(),
                });
                st.editing = false;
            }
            if answers.is_some() && kit::link(ui, p, t("macro.cancel")).clicked() {
                st.seeded = None;
            }
        });
        kit::para(ui, txt(t("macro.yourDayWhy"), text::SM, p.fg_muted));
    });
}

// ---------------------------------------------------------------- per window

fn plan_text(hit: &str) -> Option<&'static str> {
    Some(match hit {
        "да" => "planYes",
        "нет" => "planNo",
        "нет пробоя" => "planNoBreak",
        "пробой без IB1" => "planHalf",
        _ => return None,
    })
}

#[allow(clippy::too_many_arguments)] // one card shows one window's whole evening
fn region_card(
    ui: &mut Ui,
    p: &Palette,
    region: Region,
    rv: Option<&ReviewRegion>,
    reflection: Option<&ReviewAgentRegion>,
    zones: &Zones,
    shots: Option<&Shots>,
    human: Option<&HumanRead>,
    answer: Option<&DayAnswerRegion>,
    past: bool,
    tex: &mut ShotTextures,
    snap: &Snapshot,
    out: &mut Vec<Command>,
) -> Option<usize> {
    let title = t(&format!("macro.window.{}", region.as_str())).to_owned();
    let Some(rv) = rv.filter(|rv| rv.missing.as_deref().is_none_or(str::is_empty)) else {
        kit::card(ui, p, Some(&title), CardKind::Plain, |ui| {
            ui.label(txt(t("macro.noReviewRegion"), text::BASE, p.fg_muted));
        });
        return None;
    };
    let facts = rv.facts.clone().unwrap_or_default();
    let dir = facts.dir.clone().unwrap_or_else(|| "FLAT".into());
    let needle = rv.after.or(rv.needle).unwrap_or(0.0);
    let after_skip = rv
        .after
        .is_some_and(|a| zone_of(a, zones) == ZoneKey::Neutral);
    let hit = counted_hit(rv);
    let plan = rv
        .score
        .as_ref()
        .and_then(|s| s.plan_hit.as_deref())
        .and_then(plan_text);
    let head = if facts.partial == Some(true) {
        format!("{title} · {}", t("macro.partial"))
    } else {
        title
    };
    let mut open = None;
    kit::card(ui, p, Some(&head), CardKind::Plain, |ui| {
        ui.horizontal(|ui| {
            ui.allocate_ui(egui::vec2(110.0, 80.0), |ui| {
                Gauge::new(needle as f32, zones.clone())
                    .before(rv.after.and(rv.needle).map(|n| n as f32))
                    .mini(true)
                    .id_salt(("review-gauge", region.as_str()))
                    .show(ui);
            });
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(txt(t("macro.factWas"), text::BASE, p.fg));
                    let c = match dir.as_str() {
                        "LONG" => p.ok,
                        "SHORT" => p.err,
                        _ => p.fg,
                    };
                    ui.label(bold(t(&format!("macro.dir.{dir}")), text::BASE, c));
                });
                if let (true, Some(nh)) = (
                    rv.after.is_some(),
                    rv.score.as_ref().and_then(|s| s.needle_hit.as_deref()),
                ) {
                    let k = if nh == "1" {
                        "macro.morningRight"
                    } else {
                        "macro.morningWrong"
                    };
                    ui.label(txt(t(k), text::SM, p.fg_muted));
                }
            });
        });
        let scale_hit = if after_skip { Some("skip") } else { hit };
        who_was_right(
            ui,
            p,
            human,
            answer,
            past,
            &dir,
            zone_label(zone_of(needle, zones)),
            scale_hit,
            plan,
        );
        open = shots::inline(ui, p, tex, snap, shots, out);
        match reflection {
            Some(r) => {
                for (k, v) in [
                    ("macro.worked", &r.what_worked),
                    ("macro.failed", &r.what_failed),
                    ("macro.whoRight", &r.discussion),
                ] {
                    if let Some(v) = v {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(bold(t(k), text::MD, p.fg));
                            ui.add(egui::Label::new(txt(v, text::MD, p.fg)).wrap());
                        });
                    }
                }
            }
            None => {
                ui.label(txt(t("macro.noReflection"), text::BASE, p.fg_muted));
            }
        }
    });
    open
}

/// «Кто был прав»: the owner's read, the scale, the plan, each scored against the window.
#[allow(clippy::too_many_arguments)] // the row needs every party of the evening
fn who_was_right(
    ui: &mut Ui,
    p: &Palette,
    human: Option<&HumanRead>,
    answer: Option<&DayAnswerRegion>,
    past: bool,
    dir: &str,
    scale_zone: &str,
    scale_hit: Option<&str>,
    plan: Option<&str>,
) {
    let hh = human_hit(human.map(|h| h.side.as_str()), Some(dir));
    let trade = answer.map(|a| match (a.trade, a.by_rule) {
        (TradeTaken::None, _) => t("macro.tradeNone").to_owned(),
        (tr, Some(br)) => format!("{} · {}", t(trade_key(tr)), t(by_rule_key(br))),
        (tr, None) => t(trade_key(tr)).to_owned(),
    });
    let row = |ui: &mut Ui, k: &str, body: &mut dyn FnMut(&mut Ui)| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = space::S4;
            ui.add_sized([56.0, 20.0], egui::Label::new(bold(k, text::BASE, p.fg)));
            body(ui);
        });
    };
    if human.is_some() || !past {
        row(ui, t("macro.whoYou"), &mut |ui| {
            match human {
                Some(h) => {
                    let mark = match hh {
                        Some(true) => " ✓",
                        Some(false) => " ✗",
                        None => "",
                    };
                    let tone = match hh {
                        Some(true) => Tone::Good,
                        Some(false) => Tone::Bad,
                        None => Tone::Neutral,
                    };
                    kit::chip(
                        ui,
                        p,
                        &format!("{}{mark}", t(&human_side_key(h.side))),
                        tone,
                        text::MD,
                    );
                }
                None => {
                    ui.label(txt(t("macro.whoNoRead"), text::MD, p.fg_muted));
                }
            }
            if let Some(tr) = &trade {
                ui.label(txt(tr, text::SM, p.fg_muted));
            }
        });
    } else if let Some(tr) = &trade {
        row(ui, t("macro.whoYou"), &mut |ui| {
            ui.label(txt(tr, text::SM, p.fg_muted));
        });
    }
    row(ui, t("macro.whoScale"), &mut |ui| {
        let (mark, tone) = match scale_hit {
            Some("1") => (" ✓", Tone::Good),
            Some("0") => (" ✗", Tone::Bad),
            _ => ("", Tone::Neutral),
        };
        kit::chip(ui, p, &format!("{scale_zone}{mark}"), tone, text::MD);
        match scale_hit {
            Some("skip") => {
                ui.label(txt(t("macro.hitSkip"), text::SM, p.fg_muted));
            }
            Some("1") | Some("0") => {}
            _ => {
                ui.label(txt(t("macro.hitNone"), text::SM, p.fg_muted));
            }
        }
    });
    if let Some(pl) = plan {
        row(ui, t("macro.whoPlan"), &mut |ui| {
            let tone = match pl {
                "planYes" => Tone::Good,
                "planNo" => Tone::Bad,
                _ => Tone::Neutral,
            };
            kit::chip(ui, p, t(&format!("macro.{pl}")), tone, text::MD);
        });
    }
}

// ---------------------------------------------------------------- «Подробнее»

fn factor_hits(ui: &mut Ui, p: &Palette, region: Region, rv: Option<&ReviewRegion>, day: &Day) {
    let rows: Vec<_> = rv
        .map(|rv| {
            rv.factors
                .iter()
                .filter(|x| matches!(x.hit.as_deref(), Some("1") | Some("0")))
                .collect()
        })
        .unwrap_or_default();
    if rows.is_empty() {
        return;
    }
    let title = tf(
        "macro.factorHits",
        &[("r", &t(&format!("macro.window.{}", region.as_str())))],
    );
    kit::card(ui, p, Some(&title), CardKind::Plain, |ui| {
        ui.horizontal_wrapped(|ui| {
            for x in rows {
                let id = x.factor.clone().unwrap_or_default();
                let name = day
                    .factor_name_in(region, &id)
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        if id.starts_with("talk_") {
                            t("macro.talkArg").to_owned()
                        } else {
                            id.clone()
                        }
                    });
                let good = x.hit.as_deref() == Some("1");
                let s = format!("{} {name}", if good { "✓" } else { "✗" });
                kit::chip(
                    ui,
                    p,
                    &s,
                    if good { Tone::Good } else { Tone::Bad },
                    text::SM,
                );
            }
        });
    });
}

/// «Какие факторы угадывают (30 дней)»: best first.
fn stats_card(ui: &mut Ui, p: &Palette, day: &Day) {
    let mut rows: Vec<_> = day.stats.iter().filter(|s| s.total >= 1).collect();
    if rows.is_empty() {
        return;
    }
    let rate = |s: &desk_core::vault::FactorStat| s.hits as f64 / s.total as f64;
    rows.sort_by(|a, b| {
        rate(b)
            .partial_cmp(&rate(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    kit::card(ui, p, Some(t("macro.stats")), CardKind::Plain, |ui| {
        for s in rows {
            let r = rate(s) as f32;
            let id = s.factor.clone().unwrap_or_default();
            let name = day.factor_name_in(s.region, &id).unwrap_or(&id).to_owned();
            let fill = if r >= 0.6 {
                p.ok
            } else if r < 0.45 {
                p.err
            } else {
                p.border_strong
            };
            ui.horizontal(|ui| {
                let w = ui.available_width();
                let name_w = (w * 0.45).max(120.0);
                ui.allocate_ui(egui::vec2(name_w, 20.0), |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(txt(&name, text::MD, p.fg)).truncate());
                        ui.label(txt(
                            t(&format!("macro.window.{}", s.region.as_str())),
                            text::SM,
                            p.fg_muted,
                        ));
                    });
                });
                ui.allocate_ui(egui::vec2((w - name_w - 90.0).max(60.0), 20.0), |ui| {
                    kit::meter(ui, p, r, fill, true)
                        .on_hover_text(tf("macro.statTip", &[("h", &s.hits), ("n", &s.total)]));
                });
                ui.label(txt(
                    tf("macro.hitsOf", &[("ok", &s.hits), ("n", &s.total)]),
                    text::MD,
                    p.fg,
                ));
            });
        }
        kit::para(ui, txt(t("macro.statsNote"), text::SM, p.fg_muted));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_words() {
        assert_eq!(plan_text("да"), Some("planYes"));
        assert_eq!(plan_text("пробой без IB1"), Some("planHalf"));
        assert_eq!(plan_text("?"), None);
    }
}
