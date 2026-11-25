//! A window of the day, 09:30 Europe / 16:00 USA (`RegionView` of
//! `MacroDesk.tsx`): «Твой взгляд», the memory, «Куда смотрит рынок»,
//! «Что делать», «Почему так» (every factor), «Не согласен?», and «Подробнее».

use chrono::NaiveDate;
use desk_app::{ChatCommand, Command, Snapshot};
use desk_core::gauge::{split_factors, FactorSplit};
use desk_core::prompts::{discuss_prompt, macro_chat_title};
use desk_core::stats::fmt;
use desk_core::vault::{
    DiscussionItem, HumanRead, HumanSide, MacroVerdict, PlanLevel, ScaleFactor, ScaleFile,
    ScaleLight, ScaleResult, Side,
};
use desk_core::zones::{zone_of, ZoneKey, Zones};
use desk_core::{Day, Region, RegionSnapshot};
use eframe::egui::{self, CollapsingHeader, Ui};

use super::gating::{region_layout, RegionLayout, ReviewTop};
use super::kit::{self, bold, txt, CardKind, Tone};
use super::shots::{self, ShotTextures};
use crate::fonts;
use crate::gauge::{group_bar, split_bar, Gauge};
use crate::i18n::{t, tf};
use crate::theme::{space, text, Palette};

/// What the owner is typing into «Не согласен?».
#[derive(Clone, Debug, PartialEq)]
pub struct TalkDraft {
    pub side: Side,
    pub share: f64,
    pub text: String,
}

impl Default for TalkDraft {
    fn default() -> Self {
        Self {
            side: Side::Long,
            share: 10.0,
            text: String::new(),
        }
    }
}

/// UI-only state of a region view.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RegionUi {
    /// The factor whose explanation is open.
    pub factor: Option<String>,
    pub talk: TalkDraft,
}

/// What the view asks of the screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegionAction {
    None,
    Peek,
    OpenShot(usize),
    /// Open a vault note (vault-relative) in the system's app.
    OpenNote(String),
}

pub struct RegionCtx<'a> {
    pub p: &'a Palette,
    pub day: &'a Day,
    pub date: NaiveDate,
    pub region: Region,
    /// desk-core's «Сначала человек» decision for this window.
    pub vis: desk_core::windows::Visibility,
    /// Chat buttons off: a turn is running, or this is the demo vault.
    pub busy: bool,
}

pub const HUMAN_SIDES: [HumanSide; 4] = [
    HumanSide::Long,
    HumanSide::Short,
    HumanSide::Neutral,
    HumanSide::NoTrade,
];

pub fn human_side_key(s: HumanSide) -> String {
    format!("macro.humanSide.{}", s.as_str())
}

/// The scale's zone as a human side, for «вы сходитесь / расходитесь».
pub fn zone_side(z: ZoneKey) -> HumanSide {
    match z {
        ZoneKey::Neutral => HumanSide::Neutral,
        ZoneKey::Long | ZoneKey::StrongLong => HumanSide::Long,
        ZoneKey::Short | ZoneKey::StrongShort => HumanSide::Short,
    }
}

pub fn side_color(p: &Palette, s: HumanSide) -> egui::Color32 {
    match s {
        HumanSide::Long => p.ok,
        HumanSide::Short => p.err,
        _ => p.fg,
    }
}

/// `headline()`: a red light wins («ищем лонг» next to «не торгуем» would contradict itself).
pub fn headline(res: &ScaleResult, zones: &Zones, light: &ScaleLight) -> String {
    let base = if light.color.as_deref() == Some("red") {
        "macro.headRed"
    } else {
        "macro.head"
    };
    t(&format!("{base}.{}", zone_of(res.needle(), zones).as_str())).to_owned()
}

/// `lightText()`.
pub fn light_text(l: &ScaleLight) -> String {
    let num = |d: f64| desk_core::js::number_to_string(l.risk_pct.unwrap_or(d));
    match l.color.as_deref() {
        Some("red") => t("macro.lightRed").to_owned(),
        Some("yellow") => tf("macro.lightYellow", &[("r", &num(0.5))]),
        _ => tf("macro.lightGreen", &[("r", &num(1.0))]),
    }
}

pub fn light_fill(p: &Palette, l: &ScaleLight) -> egui::Color32 {
    match l.color.as_deref() {
        Some("red") => p.err_soft,
        Some("yellow") => p.warn_soft,
        Some("green") => p.ok_soft,
        _ => p.bg_elev,
    }
}

fn send(out: &mut Vec<Command>, date: NaiveDate, text: String) {
    out.push(Command::Chat(ChatCommand::Send {
        chat_id: macro_chat_title(date),
        text,
    }));
}

// ---------------------------------------------------------------- the view

pub fn show(
    ui: &mut Ui,
    cx: &RegionCtx<'_>,
    st: &mut RegionUi,
    tex: &mut ShotTextures,
    snap: &Snapshot,
    out: &mut Vec<Command>,
) -> RegionAction {
    let p = cx.p;
    let snap_r: &RegionSnapshot = cx.day.region(cx.region);
    let human = snap_r.human.as_ref();
    let mut action = RegionAction::None;
    let scale = snap_r.scale.as_deref();
    let res = scale.map(|s| s.after.as_ref().unwrap_or(&s.result));
    let scale_zone = res.map(|r| zone_of(r.needle(), &cx.day.zones));
    let pick = |side: HumanSide, out: &mut Vec<Command>| {
        out.push(Command::WriteHumanRead {
            date: cx.date,
            region: cx.region,
            side,
            before_scale: cx.vis.before_scale,
        });
    };

    match region_layout(cx.vis, human.is_some()) {
        RegionLayout::NoData { ask } => {
            if ask {
                if let Some(s) = human_read_ask(ui, p, cx.region, cx.date, false) {
                    match s {
                        AskAnswer::Side(side) => pick(side, out),
                        AskAnswer::Peek => action = RegionAction::Peek,
                    }
                }
            } else if let Some(h) = human {
                human_said(ui, p, h, None);
            }
            no_data(ui, p, t("macro.noData"));
            return action;
        }
        RegionLayout::AskOnly => {
            match human_read_ask(ui, p, cx.region, cx.date, true) {
                Some(AskAnswer::Side(side)) => pick(side, out),
                Some(AskAnswer::Peek) => action = RegionAction::Peek,
                None => {}
            }
            return action;
        }
        RegionLayout::Review { top } => match top {
            ReviewTop::Ask => {
                if let Some(AskAnswer::Side(side)) =
                    human_read_ask(ui, p, cx.region, cx.date, false)
                {
                    pick(side, out);
                }
            }
            ReviewTop::Said => {
                if let Some(h) = human {
                    human_said(ui, p, h, scale_zone);
                }
            }
            ReviewTop::Nothing => {}
        },
    }

    // Note 23 §2: the memory first — what to keep in mind today, and what changed.
    super::memory::remember(ui, p, snap_r.remember.as_ref());
    super::memory::drift(ui, p, snap_r.drift.as_ref());

    let verdict = snap_r.verdict.as_deref();
    let split = scale.map(|s| split_factors(&s.factors));
    kit::grid2(
        ui,
        |ui| {
            if let (Some(s), Some(res)) = (scale, res) {
                hero(ui, p, s, res, &cx.day.zones);
            }
        },
        |ui| {
            if let Some(i) = todo(ui, p, scale, verdict, snap_r, tex, snap, out) {
                action = RegionAction::OpenShot(i);
            }
        },
    );
    kit::grid2(
        ui,
        |ui| {
            if let Some(sp) = split
                .as_ref()
                .filter(|s| !s.long.is_empty() || !s.short.is_empty())
            {
                why(ui, p, sp, &mut st.factor);
            }
        },
        |ui| {
            if let Some(s) = scale {
                discussion(ui, p, &s.discussion, cx, &mut st.talk, out);
            }
        },
    );
    if let Some(sp) = split.as_ref() {
        factor_dialog(ui.ctx(), p, sp, cx, &mut st.factor);
    }

    more(
        ui,
        cx,
        scale,
        verdict,
        split.as_ref(),
        snap_r,
        tex,
        snap,
        out,
        &mut action,
    );
    action
}

pub fn no_data(ui: &mut Ui, p: &Palette, s: &str) {
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, p.border_strong))
        .corner_radius(egui::CornerRadius::same(crate::theme::radius::MD))
        .inner_margin(egui::Margin::same(space::S12 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| ui.label(txt(s, text::BASE, p.fg_muted)));
        });
}

// ---------------------------------------------------------------- «Твой взгляд»

pub enum AskAnswer {
    Side(HumanSide),
    Peek,
}

/// The question, one tap; `peek_link` adds «Показать обзор без ответа».
pub fn human_read_ask(
    ui: &mut Ui,
    p: &Palette,
    region: Region,
    date: NaiveDate,
    peek_link: bool,
) -> Option<AskAnswer> {
    let mut answer = None;
    kit::card(ui, p, Some(t("macro.yourReadTitle")), CardKind::Ask, |ui| {
        let id = match region {
            Region::Eu => desk_core::WindowId::Eu,
            Region::Us => desk_core::WindowId::Us,
        };
        let q = super::with_real_hours(
            t(&format!("macro.yourReadAsk.{}", region.as_str())),
            id,
            date,
        );
        kit::para(ui, txt(q, text::LG, p.fg));
        let labels: Vec<String> = HUMAN_SIDES
            .iter()
            .map(|s| t(&human_side_key(*s)).to_owned())
            .collect();
        let items: Vec<(&str, Tone)> = HUMAN_SIDES
            .iter()
            .zip(&labels)
            .map(|(s, l)| {
                let tone = match s {
                    HumanSide::Long => Tone::Good,
                    HumanSide::Short => Tone::Bad,
                    _ => Tone::Accent,
                };
                (l.as_str(), tone)
            })
            .collect();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = space::S4;
            for (i, (label, tone)) in items.iter().enumerate() {
                let color = match tone {
                    Tone::Good => p.ok,
                    Tone::Bad => p.err,
                    _ => p.fg,
                };
                let b = egui::Button::new(bold(*label, text::LG, color))
                    .min_size(egui::vec2(120.0, 40.0));
                if ui.add(b).clicked() {
                    answer = Some(AskAnswer::Side(HUMAN_SIDES[i]));
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(txt(t("macro.yourReadWhy"), text::SM, p.fg_muted));
            if peek_link && kit::link(ui, p, t("macro.veilPeek")).clicked() {
                answer = Some(AskAnswer::Peek);
            }
        });
    });
    answer
}

/// The owner's read and how it compares with the scale.
pub fn human_said(ui: &mut Ui, p: &Palette, h: &HumanRead, scale: Option<ZoneKey>) {
    let ss = scale.map(zone_side);
    let agree = ss.map(|ss| {
        if ss == h.side {
            "same"
        } else if h.side == HumanSide::NoTrade
            || ss == HumanSide::Neutral
            || h.side == HumanSide::Neutral
        {
            "near"
        } else {
            "differ"
        }
    });
    let inner = kit::card(ui, p, None, CardKind::Plain, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = space::S3;
            ui.label(txt(t("macro.yourReadWas"), text::BASE, p.fg));
            ui.label(bold(
                t(&human_side_key(h.side)),
                text::BASE,
                side_color(p, h.side),
            ));
            if !h.before_scale {
                ui.label(txt(
                    format!("· {}", t("macro.afterGauge")),
                    text::BASE,
                    p.fg_muted,
                ));
            }
            ui.add_space(space::S8);
            match (ss, agree) {
                (Some(ss), Some(a)) => {
                    ui.label(txt(t("macro.scaleSays"), text::BASE, p.fg));
                    ui.label(bold(
                        t(&format!("macro.scaleSide.{}", ss.as_str())),
                        text::BASE,
                        side_color(p, ss),
                    ));
                    ui.label(txt(
                        format!("— {}", t(&format!("macro.agree.{a}"))),
                        text::BASE,
                        p.fg_muted,
                    ));
                }
                _ => {
                    ui.label(txt(t("macro.scaleNotYet"), text::BASE, p.fg_muted));
                }
            }
        });
        ui.min_rect()
    });
    // `.said.same` / `.said.differ`: a coloured rule on the left.
    let rule = match agree {
        Some("same") => Some(p.ok),
        Some("differ") => Some(p.warn),
        _ => None,
    };
    if let Some(c) = rule {
        let r = inner.expand2(egui::vec2(space::S6, space::S5));
        ui.painter()
            .vline(r.left() + 1.5, r.y_range(), egui::Stroke::new(3.0, c));
    }
}

// ---------------------------------------------------------------- the main cards

fn hero(ui: &mut Ui, p: &Palette, s: &ScaleFile, res: &ScaleResult, zones: &Zones) {
    kit::card(ui, p, Some(t("macro.whereTitle")), CardKind::Plain, |ui| {
        let before = s.after.as_ref().map(|_| s.result.needle() as f32);
        Gauge::new(res.needle() as f32, zones.clone())
            .before(before)
            .show(ui);
        ui.vertical_centered(|ui| {
            ui.add(egui::Label::new(bold(headline(res, zones, &s.light), text::LG, p.fg)).wrap());
        });
        split_bar(
            ui,
            res.long.unwrap_or(0.0) as f32,
            res.short.unwrap_or(0.0) as f32,
            res.undecided.unwrap_or(0.0) as f32,
        );
        kit::tinted(ui, light_fill(p, &s.light), |ui| {
            ui.spacing_mut().item_spacing.y = space::S1;
            ui.label(bold(light_text(&s.light), text::BASE, p.fg));
            if let Some(why) = s.light.red.iter().chain(&s.light.yellow).next() {
                kit::para(ui, txt(why, text::SM, p.fg_muted));
            }
        });
    });
}

#[allow(clippy::too_many_arguments)] // the card reads the whole window
fn todo(
    ui: &mut Ui,
    p: &Palette,
    scale: Option<&ScaleFile>,
    verdict: Option<&MacroVerdict>,
    snap_r: &RegionSnapshot,
    tex: &mut ShotTextures,
    snap: &Snapshot,
    out: &mut Vec<Command>,
) -> Option<usize> {
    let opinion: Vec<String> = match verdict.filter(|v| !v.plan.is_empty()) {
        Some(v) => v.plan.clone(),
        None => scale
            .and_then(|s| s.plan.as_ref())
            .and_then(|pl| pl.summary.clone())
            .into_iter()
            .collect(),
    };
    let mut open = None;
    kit::card(ui, p, Some(t("macro.todoTitle")), CardKind::Plain, |ui| {
        if opinion.is_empty() {
            ui.label(txt(t("macro.noOpinion"), text::BASE, p.fg_muted));
        } else {
            kit::bullets(ui, p, &opinion, text::BASE);
        }
        let cancel = scale
            .and_then(|s| s.plan.as_ref())
            .and_then(|pl| pl.cancel.as_ref());
        if let (Some(c), true) = (cancel, verdict.is_none_or(|v| v.plan.is_empty())) {
            kit::tinted(ui, p.bg_elev, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(bold(t("macro.cancelTitle"), text::MD, p.fg));
                    ui.add(egui::Label::new(txt(c, text::MD, p.fg)).wrap());
                });
            });
        }
        open = shots::inline(ui, p, tex, snap, snap_r.shots.as_ref(), out);
        if let Some(v) = verdict {
            ui.separator();
            let key = v.verdict.clone().unwrap_or_default();
            let tone = match key.as_str() {
                "LONG" => Tone::Good,
                "SHORT" => Tone::Bad,
                "NO_TRADE" => Tone::Warn,
                _ => Tone::Neutral,
            };
            ui.horizontal(|ui| {
                kit::chip(
                    ui,
                    p,
                    t(&format!("macro.newsVerdict.{key}")),
                    tone,
                    text::SM,
                )
            });
            if let Some(sum) = &v.summary {
                kit::para(ui, txt(sum, text::BASE, p.fg_muted));
            }
        }
    });
    open
}

// ---------------------------------------------------------------- «Почему так»

fn factor_id(f: &ScaleFactor) -> &str {
    f.id.as_deref().unwrap_or("")
}

fn factor_name(f: &ScaleFactor) -> &str {
    f.name.as_deref().or(f.id.as_deref()).unwrap_or("—")
}

fn why(ui: &mut Ui, p: &Palette, sp: &FactorSplit<'_>, open: &mut Option<String>) {
    kit::card(ui, p, Some(t("macro.whyTitle")), CardKind::Plain, |ui| {
        kit::columns(ui, 2, |i, ui| {
            if i == 0 {
                why_column(ui, p, true, &sp.long, open);
            } else {
                why_column(ui, p, false, &sp.short, open);
            }
        });
        kit::para(ui, txt(t("macro.whyHint"), text::SM, p.fg_muted));
    });
}

fn why_column(
    ui: &mut Ui,
    p: &Palette,
    long: bool,
    list: &[&ScaleFactor],
    open: &mut Option<String>,
) {
    let color = if long { p.ok } else { p.err };
    // `+ 0.0`: an empty f64 sum is −0.0, which would print as «-0 %».
    let sum: f64 = list.iter().map(|f| f.points()).sum::<f64>() + 0.0;
    let head = ui
        .horizontal(|ui| {
            ui.label(bold(
                format!("{} %", desk_core::js::round(sum)),
                20.0,
                color,
            ));
            ui.label(bold(
                t(if long {
                    "macro.forLongTitle"
                } else {
                    "macro.forShortTitle"
                }),
                text::BASE,
                color,
            ));
        })
        .response;
    ui.painter().hline(
        head.rect.x_range(),
        head.rect.bottom() + 3.0,
        egui::Stroke::new(1.0, p.border),
    );
    ui.add_space(space::S3);
    if list.is_empty() {
        ui.label(txt(t("macro.noneSide"), text::MD, p.fg_muted));
        return;
    }
    for f in list {
        let id = factor_id(f).to_owned();
        let is_open = open.as_deref() == Some(id.as_str());
        let r = ui
            .horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = space::S3;
                ui.add_sized(
                    [48.0, 18.0],
                    egui::Label::new(bold(
                        format!("{} %", fmt(Some(f.points()), 1)),
                        text::MD,
                        color,
                    )),
                );
                let mut name = txt(factor_name(f), text::MD, p.fg);
                if is_open {
                    name = name.underline();
                }
                ui.add(egui::Label::new(name).wrap().sense(egui::Sense::click()))
            })
            .inner;
        let r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
        if r.clicked() {
            *open = if is_open { None } else { Some(id) };
        }
    }
}

fn about_key(id: &str) -> &str {
    if id.starts_with("talk_") {
        "talk"
    } else {
        id
    }
}

/// The explanation of a factor as a modal: Esc or a click outside closes it.
fn factor_dialog(
    ctx: &egui::Context,
    p: &Palette,
    sp: &FactorSplit<'_>,
    cx: &RegionCtx<'_>,
    open: &mut Option<String>,
) {
    let Some(id) = open.clone() else {
        return;
    };
    let Some(f) = sp.long.iter().chain(&sp.short).find(|f| factor_id(f) == id) else {
        *open = None;
        return;
    };
    let stat = cx
        .day
        .stats
        .iter()
        .find(|s| s.region == cx.region && s.factor.as_deref() == Some(id.as_str()));
    let modal = egui::Modal::new(egui::Id::NULL.with("desk-factor")).show(ctx, |ui| {
        ui.set_max_width(520.0);
        ui.spacing_mut().item_spacing.y = space::S4;
        let side_color = match f.side {
            Some(Side::Long) => p.ok,
            Some(Side::Short) => p.err,
            None => p.fg,
        };
        let mut close = false;
        ui.horizontal(|ui| {
            ui.label(bold(factor_name(f), text::LG, side_color));
            let side_key = match f.side {
                Some(Side::Long) => "macro.forLong",
                Some(Side::Short) => "macro.forShort",
                None => "macro.silentShort",
            };
            ui.label(txt(format!("· {}", t(side_key)), text::BASE, p.fg_muted));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                close = ui
                    .add(egui::Button::new(txt("×", text::LG, p.fg)).frame(false))
                    .on_hover_text(t("macro.shotsClose"))
                    .clicked();
            });
        });
        if let Some(w) = &f.why {
            line(ui, p, t("macro.factorToday"), w, p.fg);
        }
        let key = format!("macro.factorAbout.{}", about_key(&id));
        let what_key = format!("{key}.what");
        let what = t(&what_key);
        if what != what_key {
            line(ui, p, t("macro.factorWhat"), what, p.fg);
            let lc = if f.side == Some(Side::Long) {
                p.ok
            } else {
                p.fg
            };
            let sc = if f.side == Some(Side::Short) {
                p.err
            } else {
                p.fg
            };
            line(ui, p, t("macro.factorLong"), t(&format!("{key}.long")), lc);
            line(
                ui,
                p,
                t("macro.factorShort"),
                t(&format!("{key}.short")),
                sc,
            );
        } else {
            kit::para(ui, txt(t("macro.factorNoAbout"), text::BASE, p.fg_muted));
        }
        let mut w = tf(
            "macro.factorWeight",
            &[
                ("w", &desk_core::js::round(f.share())),
                (
                    "s",
                    &desk_core::js::round(f.strength.unwrap_or(0.0) * 100.0),
                ),
                ("p", &fmt(Some(f.points()), 1)),
            ],
        );
        if let Some(st) = stat.filter(|s| s.total >= 3) {
            let r = desk_core::js::round(st.hits as f64 * 100.0 / st.total as f64);
            w.push_str(&format!(
                " · {}",
                tf("macro.factorHitRate", &[("r", &r), ("n", &st.total)])
            ));
        }
        kit::para(ui, txt(w, text::SM, p.fg_muted));
        close
    });
    if modal.inner || modal.should_close() {
        *open = None;
    }
}

fn line(ui: &mut Ui, p: &Palette, head: &str, body: &str, color: egui::Color32) {
    let mut job = egui::text::LayoutJob::default();
    let fmt_of = |c, fam| egui::TextFormat::simple(egui::FontId::new(text::BASE, fam), c);
    job.append(head, 0.0, fmt_of(p.fg, fonts::bold()));
    job.append(body, 6.0, fmt_of(color, egui::FontFamily::Proportional));
    ui.add(egui::Label::new(job).wrap());
}

// ---------------------------------------------------------------- «Не согласен?»

const WEIGHTS: [(f64, &str); 3] = [(5.0, "weak"), (10.0, "important"), (20.0, "key")];

fn view_icon(v: Option<&str>) -> &'static str {
    match v {
        Some("agree") => "✓",
        Some("partial") => "≈",
        Some("disagree") => "✗",
        _ => "»",
    }
}

fn discussion(
    ui: &mut Ui,
    p: &Palette,
    items: &[DiscussionItem],
    cx: &RegionCtx<'_>,
    draft: &mut TalkDraft,
    out: &mut Vec<Command>,
) {
    kit::card(ui, p, Some(t("macro.discussion")), CardKind::Plain, |ui| {
        for d in items {
            let c = if d.side == Some(Side::Long) {
                p.ok
            } else {
                p.err
            };
            kit::ruled(ui, c, |ui| {
                ui.spacing_mut().item_spacing.y = space::S1;
                ui.horizontal(|ui| {
                    let by = if d.by.as_deref() == Some("owner") {
                        "macro.byOwner"
                    } else {
                        "macro.byAgent"
                    };
                    ui.label(bold(t(by), text::MD, p.fg));
                    let side = if d.side == Some(Side::Long) {
                        "macro.forLong"
                    } else {
                        "macro.forShort"
                    };
                    ui.label(txt(format!("· {}", t(side)), text::MD, p.fg_muted));
                });
                if let Some(x) = &d.text {
                    kit::para(ui, txt(x, text::MD, p.fg));
                }
                if d.reply.is_some() || d.agent_view.as_deref().is_some_and(|v| !v.is_empty()) {
                    kit::para(
                        ui,
                        txt(
                            format!(
                                "{} {} {}",
                                view_icon(d.agent_view.as_deref()),
                                t("macro.agentSays"),
                                d.reply.as_deref().unwrap_or("")
                            ),
                            text::MD,
                            p.fg_muted,
                        ),
                    );
                }
            });
        }
        ui.separator();
        let hint = kit::para(ui, txt(t("macro.discussHint"), text::MD, p.fg_muted));
        ui.horizontal_wrapped(|ui| {
            let sides = [
                (t("macro.forLongBtn"), Tone::Good),
                (t("macro.forShortBtn"), Tone::Bad),
            ];
            let sel = if draft.side == Side::Long { 0 } else { 1 };
            if let Some(i) = kit::seg(ui, p, &sides, Some(sel), false) {
                draft.side = if i == 0 { Side::Long } else { Side::Short };
            }
            let names: Vec<String> = WEIGHTS
                .iter()
                .map(|(_, k)| t(&format!("macro.weightName.{k}")).to_owned())
                .collect();
            let items: Vec<(&str, Tone)> =
                names.iter().map(|n| (n.as_str(), Tone::Accent)).collect();
            let sel = WEIGHTS.iter().position(|(s, _)| *s == draft.share);
            if let Some(i) = kit::seg(ui, p, &items, sel, false) {
                draft.share = WEIGHTS[i].0;
            }
        });
        let edit = egui::TextEdit::multiline(&mut draft.text)
            .desired_rows(2)
            .desired_width(f32::INFINITY)
            .hint_text(t("macro.argPlaceholder"));
        ui.add(edit).labelled_by(hint.id);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            let ready = !cx.busy && !draft.text.trim().is_empty();
            if kit::button(ui, p, t("macro.sendArg"), true, ready).clicked() {
                let prompt =
                    discuss_prompt(cx.region, draft.side, draft.share, &draft.text, cx.date);
                send(out, cx.date, prompt);
                draft.text.clear();
            }
        });
    });
}

// ---------------------------------------------------------------- «Подробнее»

#[allow(clippy::too_many_arguments)] // «Подробнее» carries every detail of the window
fn more(
    ui: &mut Ui,
    cx: &RegionCtx<'_>,
    scale: Option<&ScaleFile>,
    verdict: Option<&MacroVerdict>,
    split: Option<&FactorSplit<'_>>,
    snap_r: &RegionSnapshot,
    tex: &mut ShotTextures,
    snap: &Snapshot,
    out: &mut Vec<Command>,
    action: &mut RegionAction,
) {
    let p = cx.p;
    more_frame(
        ui,
        p,
        t("macro.more"),
        egui::Id::NULL.with(("more", cx.region.as_str())),
        |ui| {
            if let Some(i) = shots::card(ui, p, tex, snap, snap_r.shots.as_ref(), out) {
                *action = RegionAction::OpenShot(i);
            }
            kit::grid2(
                ui,
                |ui| {
                    if let Some(pl) = scale.and_then(|s| s.plan.as_ref()) {
                        plan_card(ui, p, pl);
                    }
                },
                |ui| {
                    if let Some(v) = verdict {
                        if news_card(ui, p, v, snap_r.note.is_some()) {
                            if let Some(n) = &snap_r.note {
                                *action = RegionAction::OpenNote(n.clone());
                            }
                        }
                    }
                },
            );
            if let Some(g) = scale.and_then(|s| s.result.groups.as_ref()) {
                groups_card(ui, p, g);
            }
            if let Some(sp) = split {
                factor_table(ui, p, sp, cx.day, cx.region);
            }
            context_card(ui, p, snap_r);
            if let Some(s) = scale {
                let mut line = tf(
                    "macro.techLine",
                    &[
                        ("sym", &s.sym.clone().unwrap_or_default()),
                        ("asof", &s.asof.clone().unwrap_or_default()),
                        ("version", &s.version.clone().unwrap_or_else(|| "—".into())),
                    ],
                );
                if s.history.len() > 1 {
                    line.push_str(&format!(
                        " · {}",
                        tf("macro.rebuilt", &[("n", &s.history.len())])
                    ));
                }
                ui.label(txt(line, text::SM, p.fg_muted));
            }
        },
    );
}

/// `details.more`: a dashed frame with a collapsed header (closed by default).
pub fn more_frame(ui: &mut Ui, p: &Palette, title: &str, id: egui::Id, body: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, p.border_strong))
        .corner_radius(egui::CornerRadius::same(crate::theme::radius::MD))
        .inner_margin(egui::Margin::symmetric(space::S6 as i8, space::S4 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            CollapsingHeader::new(txt(title, text::BASE, p.fg_muted))
                .id_salt(id)
                .default_open(false)
                .show_unindented(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = space::S6;
                    ui.add_space(space::S4);
                    body(ui);
                });
        });
}

fn levels(ui: &mut Ui, p: &Palette, title: &str, list: &[PlanLevel]) {
    if list.is_empty() {
        return;
    }
    ui.vertical(|ui| {
        kit::sub(ui, p, title);
        for l in list {
            ui.horizontal_wrapped(|ui| {
                ui.label(txt(l.name.clone().unwrap_or_default(), text::MD, p.fg));
                if let Some(a) = l.adr {
                    ui.label(txt(
                        tf("macro.adrAway", &[("x", &fmt(Some(a), 2))]),
                        text::MD,
                        p.fg_muted,
                    ));
                }
                if let Some(pr) = l.price {
                    ui.label(
                        egui::RichText::new(desk_core::js::number_to_string(pr))
                            .monospace()
                            .color(p.fg),
                    );
                }
            });
        }
    });
}

fn plan_card(ui: &mut Ui, p: &Palette, pl: &desk_core::vault::ScalePlan) {
    kit::card(ui, p, Some(t("macro.planTitle")), CardKind::Plain, |ui| {
        kit::columns(ui, 2, |i, ui| {
            if i == 0 {
                levels(ui, p, t("macro.manip"), &pl.manipulation_levels);
            } else {
                levels(ui, p, t("macro.targets"), &pl.targets);
            }
        });
        for x in &pl.setups {
            ui.horizontal_wrapped(|ui| {
                ui.label(bold(x.setup.clone().unwrap_or_default(), text::MD, p.fg));
                if let Some(w) = &x.why {
                    ui.add(egui::Label::new(txt(format!("— {w}"), text::MD, p.fg_muted)).wrap());
                }
            });
            egui::Grid::new(("setup", x.setup.clone()))
                .num_columns(2)
                .spacing([space::S6, space::S2])
                .show(ui, |ui| {
                    for (k, v) in [
                        ("macro.entry", &x.entry),
                        ("macro.stop", &x.stop),
                        ("macro.target", &x.target),
                        ("macro.until", &x.until),
                    ] {
                        if let Some(v) = v {
                            ui.label(txt(t(k), text::MD, p.fg_muted));
                            ui.add(egui::Label::new(txt(v, text::MD, p.fg)).wrap());
                            ui.end_row();
                        }
                    }
                });
        }
    });
}

fn list(ui: &mut Ui, p: &Palette, title: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }
    kit::sub(ui, p, title);
    kit::bullets(ui, p, items, text::MD);
}

/// Returns `true` when «Открыть заметку» was pressed.
fn news_card(ui: &mut Ui, p: &Palette, v: &MacroVerdict, has_note: bool) -> bool {
    let mut open = false;
    let extra = |ui: &mut Ui| {
        if has_note && kit::link(ui, p, t("macro.openNote")).clicked() {
            open = true;
        }
    };
    kit::card_ex(
        ui,
        p,
        Some(t("macro.newsTitle")),
        CardKind::Plain,
        extra,
        |ui| {
            list(ui, p, t("macro.reasons"), &v.reasons);
            list(ui, p, t("macro.risks"), &v.risks);
            list(ui, p, t("macro.watch"), &v.watch);
            let judgment: Vec<String> = v
                .scale_judgment
                .iter()
                .map(|j| {
                    let side = if j.side == Some(Side::Long) {
                        "macro.forLong"
                    } else {
                        "macro.forShort"
                    };
                    format!("{}: {}", t(side), j.text.clone().unwrap_or_default())
                })
                .collect();
            list(ui, p, t("macro.judgment"), &judgment);
            if !v.sources.is_empty() {
                kit::sub(ui, p, t("macro.sources"));
                for s in &v.sources {
                    ui.horizontal_wrapped(|ui| {
                        let title = s.title.clone().unwrap_or_default();
                        match &s.url {
                            Some(url) => {
                                ui.hyperlink_to(txt(title, text::MD, p.accent), url);
                            }
                            None => {
                                ui.label(txt(title, text::MD, p.fg));
                            }
                        }
                        if let Some(src) = &s.source {
                            ui.label(txt(format!("— {src}"), text::MD, p.fg_muted));
                        }
                    });
                }
            }
        },
    );
    open
}

fn groups_card(ui: &mut Ui, p: &Palette, groups: &desk_core::vault::indexed::Groups) {
    let list: Vec<_> = groups.iter().collect();
    if list.is_empty() {
        return;
    }
    let scale = list
        .iter()
        .map(|(_, g)| g.share.unwrap_or(0.0))
        .fold(1.0, f64::max) as f32;
    kit::card(ui, p, Some(t("macro.groups")), CardKind::Plain, |ui| {
        for (name, g) in list {
            group_bar(
                ui,
                name,
                g.share.unwrap_or(0.0) as f32,
                g.long.unwrap_or(0.0) as f32,
                g.short.unwrap_or(0.0) as f32,
                scale,
            );
        }
        ui.label(txt(t("macro.groupsNote"), text::SM, p.fg_muted));
    });
}

/// «Все факторы»: every factor that is on, never hidden (TZ item 6).
fn factor_table(ui: &mut Ui, p: &Palette, sp: &FactorSplit<'_>, day: &Day, region: Region) {
    kit::card(ui, p, Some(t("macro.allFactors")), CardKind::Plain, |ui| {
        let rows = sp
            .long
            .iter()
            .map(|f| (f, Some(true)))
            .chain(sp.short.iter().map(|f| (f, Some(false))))
            .chain(sp.silent.iter().map(|f| (f, None)));
        egui::Grid::new(("ftable", region.as_str()))
            .num_columns(5)
            .striped(true)
            .spacing([space::S6, space::S3])
            .show(ui, |ui| {
                for k in [
                    "macro.colFactor",
                    "macro.colSide",
                    "macro.colWeight",
                    "macro.colGave",
                    "macro.colHit",
                ] {
                    ui.label(bold(t(k), text::SM, p.fg_muted));
                }
                ui.end_row();
                for (f, side) in rows {
                    let r = ui.add(egui::Label::new(txt(factor_name(f), text::MD, p.fg)).wrap());
                    if let Some(w) = &f.why {
                        r.on_hover_text(w);
                    }
                    let (s, c) = match side {
                        Some(true) => (t("macro.forLong"), p.ok),
                        Some(false) => (t("macro.forShort"), p.err),
                        None => (t("macro.silentShort"), p.fg_muted),
                    };
                    ui.label(txt(s, text::MD, c));
                    ui.label(txt(
                        format!("{} %", desk_core::js::round(f.share())),
                        text::MD,
                        p.fg,
                    ));
                    let gave = if f.points() != 0.0 {
                        format!("{} %", fmt(Some(f.points()), 1))
                    } else {
                        "—".into()
                    };
                    ui.label(txt(gave, text::MD, p.fg));
                    let st = day
                        .stats
                        .iter()
                        .find(|s| s.region == region && s.factor.as_deref() == f.id.as_deref())
                        .filter(|s| s.total >= 3);
                    let hit = st.map_or_else(
                        || "—".to_owned(),
                        |s| {
                            format!(
                                "{} % ({})",
                                desk_core::js::round(s.hits as f64 * 100.0 / s.total as f64),
                                s.total
                            )
                        },
                    );
                    ui.label(txt(hit, text::MD, p.fg_muted));
                    ui.end_row();
                }
            });
        kit::para(ui, txt(t("macro.factorNote"), text::SM, p.fg_muted));
    });
}

fn context_card(ui: &mut Ui, p: &Palette, snap_r: &RegionSnapshot) {
    let c = snap_r.collect.as_deref();
    let shape = snap_r.verdict.as_deref().and_then(|v| v.day_shape.as_ref());
    if c.is_none() && shape.is_none() {
        return;
    }
    let events: Vec<_> = c
        .and_then(|c| c.calendar.as_ref())
        .map(|cal| cal.events.iter().take(10).collect())
        .unwrap_or_default();
    let markets: Vec<_> = c
        .map(|c| {
            c.markets
                .iter()
                .filter(|m| m.chg_pct.is_some())
                .take(14)
                .collect()
        })
        .unwrap_or_default();
    kit::card(ui, p, Some(t("macro.context")), CardKind::Plain, |ui| {
        kit::columns(ui, 3, |col, ui| {
            if let (0, Some(sh)) = (col, shape) {
                kit::sub(ui, p, t("macro.dayShape"));
                if let Some(s) = &sh.summary {
                    kit::para(ui, txt(s, text::MD, p.fg));
                }
                for w in &sh.windows {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(w.window.clone().unwrap_or_default())
                                .monospace()
                                .color(p.fg),
                        );
                        ui.add(
                            egui::Label::new(txt(
                                w.expect.clone().unwrap_or_default(),
                                text::MD,
                                p.fg,
                            ))
                            .wrap(),
                        );
                    });
                }
            }
            if col == 1 && !events.is_empty() {
                kit::sub(ui, p, t("macro.calendar"));
                for e in &events {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(e.time_eet.clone().unwrap_or_default())
                                .monospace()
                                .color(p.fg),
                        );
                        ui.add(
                            egui::Label::new(txt(
                                e.title.clone().unwrap_or_default(),
                                text::MD,
                                p.fg,
                            ))
                            .wrap(),
                        );
                        if e.impact.as_deref() == Some("High") {
                            ui.label(bold(t("macro.important"), text::XS, p.err));
                        }
                    });
                }
            }
            if col == 2 && !markets.is_empty() {
                kit::sub(ui, p, t("macro.markets"));
                for m in &markets {
                    let ch = m.chg_pct.unwrap_or(0.0);
                    let c = if ch > 0.0 {
                        p.ok
                    } else if ch < 0.0 {
                        p.err
                    } else {
                        p.fg
                    };
                    ui.horizontal(|ui| {
                        ui.label(txt(m.label.clone().unwrap_or_default(), text::MD, p.fg));
                        let sign = if ch > 0.0 { "+" } else { "" };
                        ui.label(txt(format!("{sign}{} %", fmt(Some(ch), 2)), text::MD, c));
                    });
                }
            }
        });
    });
}
