//! «Обзор дня» — the macro-review screen (`MacroDesk.tsx`), read from the
//! latest desk-app `Snapshot`. The screen never waits: it pushes `Command`s
//! into `out` and draws what the snapshot holds.
//!
//! One trading day, three windows: Europe, USA, the 20:00 review. The day bar
//! (‹ date › «Сегодня» and the strip of days with marks) picks the day, the
//! tiles pick the window.

pub mod gating;
pub mod kit;
pub mod memory;
pub mod region;
pub mod review;
pub mod shots;

use chrono::{Datelike, NaiveDate, NaiveTime, Timelike};
use desk_app::{ChatCommand, Command, Snapshot};
use desk_core::prompts::{macro_chat_title, run_command};
use desk_core::windows::{
    default_window_at, standard_times, window_state_at, window_times, KyivNow, WindowId,
    WindowState,
};
use desk_core::zones::zone_of;
use desk_core::{Day, Region};
use eframe::egui::{self, Color32, CornerRadius, Frame, Key, Margin, Sense, Stroke, Ui};

use self::gating::{unread, Facts};
use self::kit::{bold, txt, CardKind, Tone};
use self::region::{RegionAction, RegionCtx, RegionUi};
use self::review::{AnswersUi, ReviewAction, ReviewCtx};
use self::shots::{Lightbox, ShotTextures};
use crate::gauge::{zone_label, Gauge};
use crate::i18n::{t, tf};
use crate::theme::{radius, space, text, Palette};

/// Everything the screen remembers between frames.
#[derive(Debug, Default)]
pub struct ScreenState {
    /// The day on screen; `None` until the first frame picks today.
    pub date: Option<NaiveDate>,
    /// The window picked on a tile; `None` = the default for the clock.
    pub win: Option<WindowId>,
    /// «Показать обзор без ответа», per day (reset with the date).
    pub peek: Option<WindowId>,
    pub region: RegionUi,
    pub answers: AnswersUi,
    pub lightbox: Option<Lightbox>,
    /// When the «Пересобрать» confirmation was asked (input time, s).
    pub confirm_rerun: Option<f64>,
    /// How many of `Snapshot::errors` the owner closed.
    pub errors_seen: usize,
    /// The last day asked of desk-app.
    requested: Option<NaiveDate>,
}

impl ScreenState {
    /// Open another day: the window, the peek and the drafts start over.
    pub fn go(&mut self, d: NaiveDate) {
        if self.date != Some(d) {
            self.date = Some(d);
            self.win = None;
            self.peek = None;
            self.lightbox = None;
            self.region = RegionUi::default();
            self.confirm_rerun = None;
        }
    }

    pub fn shown_date(&self) -> Option<NaiveDate> {
        self.date
    }
}

/// What the app does for the screen (things outside the snapshot).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreenEvent {
    OpenNote(String),
}

/// «Понедельник, 26 октября».
pub fn human_date(d: NaiveDate) -> String {
    let wd = t(&format!(
        "desk.weekday.{}",
        d.weekday().number_from_monday()
    ))
    .to_owned();
    let mut c = wd.chars();
    let cap: String = c
        .next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default();
    format!(
        "{cap}, {} {}",
        d.day(),
        t(&format!("desk.month.{}", d.month()))
    )
}

fn hhmm(tm: NaiveTime) -> String {
    format!("{:02}:{:02}", tm.hour(), tm.minute())
}

/// TZ item 21: the TS strings carry the standard hours; the real ones come
/// from the exchanges' clocks, which move in the weeks Europe and the US
/// switch to or from summer time on different days.
pub fn with_real_hours(s: &str, id: WindowId, date: NaiveDate) -> String {
    let (std, real) = (standard_times(id), window_times(id, date));
    let mut out = s.to_owned();
    if let (Some(so), Some(ro)) = (std.open, real.open) {
        let from = format!("{}–{}", hhmm(so), hhmm(std.until));
        let to = format!("{}–{}", hhmm(ro), hhmm(real.until));
        out = out.replace(&from, &to);
    }
    out.replace(&hhmm(std.at), &hhmm(real.at))
}

fn window_region(id: WindowId) -> Option<Region> {
    match id {
        WindowId::Eu => Some(Region::Eu),
        WindowId::Us => Some(Region::Us),
        WindowId::Review => None,
    }
}

fn state_key(s: WindowState) -> &'static str {
    match s {
        WindowState::Ready => "ready",
        WindowState::Partial => "partial",
        WindowState::Waiting => "waiting",
        WindowState::Due => "due",
        WindowState::Missed => "missed",
    }
}

/// Days the ‹ › buttons and ←/→ go to (`prevDay` / `nextDay` of the TS).
pub fn neighbours(
    days: &[NaiveDate],
    date: NaiveDate,
    today: NaiveDate,
) -> (Option<NaiveDate>, Option<NaiveDate>) {
    let prev = days.iter().filter(|d| **d < date).max().copied();
    let next = days
        .iter()
        .filter(|d| **d > date && **d <= today)
        .min()
        .copied()
        .or((date < today).then_some(today));
    (prev, next)
}

/// Draw the screen. `vault_known` = a vault was given at all.
pub fn show(
    ui: &mut Ui,
    st: &mut ScreenState,
    snap: &Snapshot,
    now: KyivNow,
    tex: &mut ShotTextures,
    out: &mut Vec<Command>,
) -> Vec<ScreenEvent> {
    let p = Palette::of(ui);
    let mut events = Vec::new();
    let today = now.date;
    let date = *st.date.get_or_insert(today);
    if st.requested != Some(date) {
        // `OpenDay`, not `Today`: the screen's clock may be fixed (demo, tests).
        out.push(Command::OpenDay(date));
        st.requested = Some(date);
        tex.forget_requests();
    }
    tex.poll(ui.ctx());

    // Before the first answer from desk-app the index is a placeholder.
    if snap.rev > 0 && !snap.index.ok {
        setup_card(ui, &p);
        return events;
    }
    let day = snap.day.as_deref().filter(|d| d.date == date);
    let days: Vec<NaiveDate> = snap.days_index.iter().map(|m| m.date).collect();
    let (prev, next) = neighbours(&days, date, today);

    // ←/→ between days, unless a picture, a dialog or a text field has the keys.
    let keys_free = st.lightbox.is_none()
        && st.region.factor.is_none()
        && ui.ctx().memory(|m| m.focused().is_none());
    if keys_free {
        let (l, r) = ui.input(|i| {
            (
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
            )
        });
        if let (true, Some(d)) = (l, prev) {
            st.go(d);
        } else if let (true, Some(d)) = (r, next) {
            st.go(d);
        }
    }

    day_bar(ui, &p, st, date, today, prev, next);
    day_strip(ui, &p, st, snap, date, today);
    if snap.index.demo == Some(true) {
        kit::tinted(ui, p.warn_soft, |ui| {
            kit::para(ui, bold(t("macro.demoBanner"), text::MD, p.warn))
        });
    }
    errors(ui, &p, st, snap);

    let facts = Facts::of(day, date, today);
    let active = st.win.unwrap_or_else(|| default_window_at(day, date, now));
    let busy = snap.index.demo == Some(true);
    tiles(ui, &p, st, day, date, now, facts, active);
    agent_strip(ui, &p, st, day, date, now, active, busy, out);

    let Some(day) = day else {
        ui.add_space(space::S8);
        ui.vertical_centered(|ui| ui.label(txt(t("desk.loading"), text::BASE, p.fg_muted)));
        return events;
    };
    let past = date < today;
    match window_region(active) {
        Some(region) => {
            let cx = RegionCtx {
                p: &p,
                day,
                date,
                region,
                past,
                peeked: st.peek == Some(active),
                busy,
            };
            match region::show(ui, &cx, &mut st.region, tex, snap, out) {
                RegionAction::Peek => st.peek = Some(active),
                RegionAction::OpenShot(i) => st.lightbox = Some(Lightbox::new(region.as_str(), i)),
                RegionAction::OpenNote(n) => events.push(ScreenEvent::OpenNote(n)),
                RegionAction::None => {}
            }
        }
        None => {
            let cx = ReviewCtx {
                p: &p,
                day,
                date,
                past,
            };
            match review::show(ui, &cx, &mut st.answers, tex, snap, out) {
                ReviewAction::OpenShot(r, i) => {
                    st.lightbox = Some(Lightbox::new(format!("review-{}", r.as_str()), i))
                }
                ReviewAction::OpenNote(n) => events.push(ScreenEvent::OpenNote(n)),
                ReviewAction::None => {}
            }
        }
    }

    // The magnifier over everything.
    if let Some(lb) = &mut st.lightbox {
        let list = lightbox_shots(day, &lb.key);
        let keep = list.is_some_and(|s| shots::lightbox(ui.ctx(), &p, lb, tex, snap, s, out));
        if !keep {
            st.lightbox = None;
        }
    }
    events
}

fn lightbox_shots<'a>(day: &'a Day, key: &str) -> Option<&'a desk_core::vault::Shots> {
    let s = match key {
        "eu" => day.eu.shots.as_ref(),
        "us" => day.us.shots.as_ref(),
        "review-eu" => day.review.shots.eu.as_ref(),
        "review-us" => day.review.shots.us.as_ref(),
        _ => None,
    }?;
    (s.ok && !s.shots.is_empty()).then_some(s)
}

fn setup_card(ui: &mut Ui, p: &Palette) {
    kit::card(ui, p, Some(t("macro.setupTitle")), CardKind::Ask, |ui| {
        kit::para(ui, txt(t("macro.noVault"), text::BASE, p.fg));
        kit::para(ui, txt(t("desk.noVaultHint"), text::MD, p.fg_muted));
    });
}

// ---------------------------------------------------------------- the day bar

fn nav_button(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    tip: &str,
    target: Option<NaiveDate>,
) -> Option<NaiveDate> {
    let b = egui::Button::new(txt(
        label,
        22.0,
        if target.is_some() { p.fg } else { p.fg_muted },
    ))
    .frame(false);
    let r = ui
        .add_enabled(target.is_some(), b)
        .on_hover_text(tip)
        .on_disabled_hover_text(tip);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, target.is_some(), tip));
    r.clicked().then_some(target).flatten()
}

fn day_bar(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ScreenState,
    date: NaiveDate,
    today: NaiveDate,
    prev: Option<NaiveDate>,
    next: Option<NaiveDate>,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = space::S4;
        if let Some(d) = nav_button(ui, p, "‹", t("macro.prevDay"), prev) {
            st.go(d);
        }
        ui.label(bold(human_date(date), text::TITLE, p.fg));
        if let Some(d) = nav_button(ui, p, "›", t("macro.nextDay"), next) {
            st.go(d);
        }
        if date != today && kit::button(ui, p, t("macro.today"), false, true).clicked() {
            st.go(today);
        }
    });
}

/// TZ item 13: the strip of days with marks (a review, the evening's result).
fn day_strip(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ScreenState,
    snap: &Snapshot,
    date: NaiveDate,
    today: NaiveDate,
) {
    let mut marks: Vec<(NaiveDate, bool, bool)> = snap
        .days_index
        .iter()
        .map(|m| (m.date, m.window_review, m.day_review))
        .collect();
    for d in [today, date] {
        if !marks.iter().any(|m| m.0 == d) {
            marks.push((d, false, false));
        }
    }
    marks.sort_by_key(|m| m.0);
    let pill = 54.0;
    let fit = ((ui.available_width() + space::S3) / (pill + space::S3))
        .floor()
        .max(1.0) as usize;
    let sel = marks.iter().position(|m| m.0 == date).unwrap_or(0);
    // Keep the open day in view, leaning to the newest days.
    let end = (sel + 1 + fit / 3).clamp(fit.min(marks.len()), marks.len());
    let start = end.saturating_sub(fit);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = space::S3;
        for &(d, win, rev) in &marks[start..end] {
            let on = d == date;
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(pill, 46.0), Sense::click());
            let fill = if on {
                p.accent_soft
            } else if resp.hovered() {
                p.bg_hover
            } else {
                p.bg_raised
            };
            let stroke = if on {
                p.accent
            } else if d == today {
                p.border_strong
            } else {
                p.border
            };
            let painter = ui.painter();
            painter.rect(
                rect,
                CornerRadius::same(radius::SM),
                fill,
                Stroke::new(1.0, stroke),
                egui::StrokeKind::Inside,
            );
            let fg = if on { p.accent } else { p.fg };
            painter.text(
                rect.center_top() + egui::vec2(0.0, 11.0),
                egui::Align2::CENTER_CENTER,
                t(&format!("desk.wd.{}", d.weekday().number_from_monday())),
                egui::FontId::proportional(text::XS),
                p.fg_muted,
            );
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("{:02}.{:02}", d.day(), d.month()),
                egui::FontId::new(text::MD, crate::fonts::bold()),
                fg,
            );
            let y = rect.bottom() - 8.0;
            let dots: Vec<Color32> = [(win, p.accent), (rev, p.ok)]
                .iter()
                .filter(|(on, _)| *on)
                .map(|(_, c)| *c)
                .collect();
            let w = dots.len() as f32 * 8.0;
            for (k, c) in dots.iter().enumerate() {
                painter.circle_filled(
                    egui::pos2(rect.center().x - w / 2.0 + 4.0 + k as f32 * 8.0, y),
                    2.5,
                    *c,
                );
            }
            let mut tip = human_date(d);
            if win {
                tip.push_str(&format!(" · {}", t("desk.markWindow")));
            }
            if rev {
                tip.push_str(&format!(" · {}", t("desk.markReview")));
            }
            let resp = resp.on_hover_text(&tip);
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &tip));
            if resp.clicked() {
                st.go(d);
            }
        }
    });
}

fn errors(ui: &mut Ui, p: &Palette, st: &mut ScreenState, snap: &Snapshot) {
    if st.errors_seen > snap.errors.len() {
        st.errors_seen = 0; // the list was trimmed
    }
    let Some(e) = snap
        .errors
        .last()
        .filter(|_| snap.errors.len() > st.errors_seen)
    else {
        return;
    };
    let vars: Vec<(&str, &dyn std::fmt::Display)> = e
        .vars
        .iter()
        .map(|(k, v)| (k.as_str(), v as &dyn std::fmt::Display))
        .collect();
    let msg = tf(&e.key, &vars);
    kit::tinted(ui, p.err_soft, |ui| {
        ui.horizontal(|ui| {
            ui.add(egui::Label::new(bold(msg, text::MD, p.err)).wrap());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(egui::Button::new(txt("×", text::BASE, p.err)).frame(false))
                    .on_hover_text(t("desk.close"))
                    .clicked()
                {
                    st.errors_seen = snap.errors.len();
                }
            });
        });
    });
}

// ---------------------------------------------------------------- the tiles

#[allow(clippy::too_many_arguments)] // the strip reads the whole day
fn tiles(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ScreenState,
    day: Option<&Day>,
    date: NaiveDate,
    now: KyivNow,
    facts: Option<Facts>,
    active: WindowId,
) {
    let peek = st.peek;
    kit::columns(ui, 3, |k, ui| {
        if let Some(&id) = WindowId::ALL.get(k) {
            let state = window_state_at(id, day, date, now);
            let hidden = unread(id, facts, peek);
            if tile(ui, p, id, day, date, state, hidden, active == id).clicked() {
                st.win = Some(id);
            }
        }
    });
}

#[allow(clippy::too_many_arguments)] // a tile is one window's whole summary
fn tile(
    ui: &mut Ui,
    p: &Palette,
    id: WindowId,
    day: Option<&Day>,
    date: NaiveDate,
    state: WindowState,
    hidden: bool,
    on: bool,
) -> egui::Response {
    let fill = if on {
        crate::theme::mix_oklab(p.accent_soft, p.bg_raised, 0.6)
    } else {
        p.bg_raised
    };
    let stroke = if on { p.accent } else { p.border };
    let inner = Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(CornerRadius::same(radius::MD))
        .inner_margin(Margin::symmetric(space::S6 as i8, space::S5 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = space::S3;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = space::S3;
                ui.label(bold(
                    t(&format!("macro.window.{}", id.as_str())),
                    text::LG,
                    p.fg,
                ));
                ui.label(txt(
                    format!("· {}", hhmm(window_times(id, date).at)),
                    text::BASE,
                    p.fg_muted,
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let tone = match state {
                        WindowState::Ready => Tone::Good,
                        WindowState::Partial => Tone::Warn,
                        WindowState::Due => Tone::Accent,
                        _ => Tone::Neutral,
                    };
                    kit::chip(
                        ui,
                        p,
                        t(&format!("macro.state.{}", state_key(state))),
                        tone,
                        text::XS,
                    );
                });
            });
            ui.set_min_height(64.0);
            if hidden {
                let k = if id == WindowId::Review {
                    "macro.tileLater"
                } else {
                    "macro.tileAsk"
                };
                let c = if id == WindowId::Review {
                    p.fg_muted
                } else {
                    p.accent
                };
                ui.label(txt(t(k), text::MD, c));
                return;
            }
            let empty = |ui: &mut Ui| {
                ui.label(txt(
                    t(&format!("macro.stateLong.{}", state_key(state))),
                    text::MD,
                    p.fg_muted,
                ));
            };
            match (id, day) {
                (WindowId::Review, Some(d)) if d.review.mech.is_some() => {
                    let hits: Vec<&str> = [Region::Eu, Region::Us]
                        .iter()
                        .filter_map(|r| d.review.mech.as_deref().and_then(|m| m.regions.get(*r)))
                        .filter_map(review::counted_hit)
                        .filter(|h| *h == "1" || *h == "0")
                        .collect();
                    let ok = hits.iter().filter(|h| **h == "1").count();
                    let head = if hits.is_empty() {
                        t("macro.noScoreYet").to_owned()
                    } else {
                        tf("macro.hitsOf", &[("ok", &ok), ("n", &hits.len())])
                    };
                    ui.label(bold(head, text::LG, p.fg));
                    let k = if d.review.agent.is_some() {
                        "macro.reflectionReady"
                    } else {
                        "macro.reflectionWait"
                    };
                    ui.label(txt(t(k), text::SM, p.fg_muted));
                }
                (WindowId::Eu | WindowId::Us, Some(d)) => {
                    let r = window_region(id).unwrap_or(Region::Eu);
                    match d.region(r).scale.as_deref() {
                        Some(s) => {
                            let n = s.after.as_ref().unwrap_or(&s.result).needle();
                            let zone = zone_of(n, &d.zones);
                            ui.horizontal(|ui| {
                                ui.allocate_ui(egui::vec2(74.0, 50.0), |ui| {
                                    Gauge::new(n as f32, d.zones.clone())
                                        .mini(true)
                                        .id_salt(("tile", id.as_str()))
                                        .show(ui);
                                });
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 0.0;
                                    ui.label(bold(
                                        zone_label(zone),
                                        text::LG,
                                        crate::gauge::zone_text(p, zone),
                                    ));
                                    let light = s.light.color.as_deref().unwrap_or("");
                                    ui.label(txt(
                                        t(&format!("macro.lightShort.{light}")),
                                        text::SM,
                                        p.fg_muted,
                                    ));
                                });
                            });
                        }
                        None => empty(ui),
                    }
                }
                _ => empty(ui),
            }
        });
    let resp = ui.interact(
        inner.response.rect,
        egui::Id::NULL.with(("tile", id.as_str())),
        Sense::click(),
    );
    let label = format!(
        "{} · {}",
        t(&format!("macro.window.{}", id.as_str())),
        t(&format!("macro.state.{}", state_key(state)))
    );
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, &label));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

// ---------------------------------------------------------------- the agent strip

#[allow(clippy::too_many_arguments)] // the strip reads the whole day
fn agent_strip(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ScreenState,
    day: Option<&Day>,
    date: NaiveDate,
    now: KyivNow,
    active: WindowId,
    blocked: bool,
    out: &mut Vec<Command>,
) {
    let state = window_state_at(active, day, date, now);
    let has = matches!(state, WindowState::Ready | WindowState::Partial);
    let title = match state {
        WindowState::Partial => t(&format!("macro.agentPartial.{}", active.as_str())).to_owned(),
        _ if has => t("macro.agentDone").to_owned(),
        _ => with_real_hours(
            t(&format!("macro.agentIdle.{}", active.as_str())),
            active,
            date,
        ),
    };
    // Rebuilding a review costs minutes and tokens: two taps, the second one
    // says so and goes away by itself after 6 s.
    let time = ui.input(|i| i.time);
    if st.confirm_rerun.is_some_and(|t0| time - t0 > 6.0) {
        st.confirm_rerun = None;
    }
    let confirm = st.confirm_rerun.is_some();
    if confirm {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(1));
    }
    Frame::new()
        .fill(p.bg_elev)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::MD))
        .inner_margin(Margin::symmetric(space::S6 as i8, space::S4 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(bold(&title, text::BASE, p.fg)).wrap());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if has {
                        if confirm {
                            t("macro.rerunConfirm")
                        } else {
                            t("macro.rerun")
                        }
                    } else {
                        t("macro.run")
                    };
                    let primary = state == WindowState::Due || confirm;
                    if kit::button(ui, p, label, primary, !blocked).clicked() {
                        if has && !confirm {
                            st.confirm_rerun = Some(time);
                        } else {
                            st.confirm_rerun = None;
                            out.push(Command::Chat(ChatCommand::Send {
                                chat_id: macro_chat_title(date),
                                text: run_command(active, date),
                            }));
                        }
                    }
                    if confirm {
                        ui.label(txt(t("macro.rerunCost"), text::SM, p.fg_muted));
                    }
                });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn russian_date() {
        assert_eq!(human_date(d("2026-10-26")), "Понедельник, 26 октября");
        assert_eq!(human_date(d("2026-09-24")), "Четверг, 24 сентября");
    }

    #[test]
    fn day_neighbours() {
        let days = [d("2026-09-14"), d("2026-09-15"), d("2026-10-26")];
        let today = d("2026-10-26");
        assert_eq!(
            neighbours(&days, d("2026-09-15"), today),
            (Some(d("2026-09-14")), Some(d("2026-10-26")))
        );
        assert_eq!(
            neighbours(&days, d("2026-09-14"), today),
            (None, Some(d("2026-09-15")))
        );
        assert_eq!(
            neighbours(&days, today, today),
            (Some(d("2026-09-15")), None)
        );
        // A day after the last file still leads to today.
        assert_eq!(neighbours(&days, d("2026-10-01"), today).1, Some(today));
    }

    #[test]
    fn hours_follow_the_exchange_clocks() {
        // 26.10.2026: Europe is on winter time, New York still on summer time.
        let date = d("2026-10-26");
        let us = with_real_hours(
            "Куда пойдёт окно открытия США (16:30–18:30)?",
            WindowId::Us,
            date,
        );
        assert_eq!(us, "Куда пойдёт окно открытия США (15:30–17:30)?");
        let eu = with_real_hours(
            "Куда пойдёт окно открытия Европы (10:00–12:00)?",
            WindowId::Eu,
            date,
        );
        assert_eq!(eu, "Куда пойдёт окно открытия Европы (10:00–12:00)?");
        assert_eq!(with_real_hours("в 16:00.", WindowId::Us, date), "в 15:00.");
        assert_eq!(
            with_real_hours("в 16:00.", WindowId::Us, d("2026-09-14")),
            "в 16:00."
        );
    }
}
