//! The chat page and the chat list of the sidebar.
//!
//! Nothing here animates by itself: a running turn shows «Работает…» and
//! «…» on a running tool instead of a spinner, so the window repaints only
//! when desk-app moves the snapshot (≤ 30 times a second while streaming).

use chrono::{DateTime, NaiveDate, Utc};
use desk_core::windows::kyiv_now;
use eframe::egui::{
    self, Align, CornerRadius, FontId, Frame, Key, KeyboardShortcut, Layout, Margin, Modifiers,
    ScrollArea, Sense, Stroke, TextEdit, Ui,
};

use super::{markdown, ChatAction, ChatItem, ChatKind, ChatMeta, ChatUi, ChatsView, Page};
use crate::i18n::{t, tf};
use crate::screen::kit::{self, bold, txt};
use crate::theme::{radius, space, text, Palette};

/// The reading column of the chat.
const CHAT_MAX_WIDTH: f32 = 820.0;

/// How long «Точно?» waits for the second press.
const CONFIRM_S: f64 = 6.0;

/// The six questions of the empty «Спроси агента» (ru.ts `home.ask*`).
const SUGGESTIONS: [&str; 6] = [
    "home.askToday",
    "home.askRule",
    "home.askSimilar",
    "home.askScale",
    "home.askWeek",
    "home.askLessons",
];

/// `mcp__tradingview__tv_scan_levels` → `tv_scan_levels`; `Bash` stays.
pub fn short_tool_name(name: &str) -> &str {
    match name.strip_prefix("mcp__") {
        Some(rest) => rest.split_once("__").map_or(rest, |(_, tool)| tool),
        None => name,
    }
}

/// «стоимость $0.12».
pub fn cost_label(usd: f64) -> String {
    format!("{} ${:.2}", t("composer.cost"), usd)
}

/// A notice's text: its key through `tf`, its variables filled in.
pub fn notice_text(key: &str, vars: &[(String, String)]) -> String {
    let vars: Vec<(&str, &dyn std::fmt::Display)> = vars
        .iter()
        .map(|(k, v)| (k.as_str(), v as &dyn std::fmt::Display))
        .collect();
    tf(key, &vars)
}

fn is_error(key: &str) -> bool {
    key.starts_with("chat.error") || key.starts_with("error.")
}

// ---------------------------------------------------------------- the page

/// The chat page: head, transcript, composer.
pub fn page(ui: &mut Ui, p: &Palette, st: &mut ChatUi, view: &ChatsView) -> Vec<ChatAction> {
    let mut out = Vec::new();
    let id = match &st.page {
        Page::Chat(id) => id.clone(),
        Page::Day => return out,
    };
    let meta = id.as_deref().and_then(|id| view.meta(id)).cloned();

    egui::Panel::top("chat-head")
        .frame(Frame::new().inner_margin(Margin {
            left: 0,
            right: 0,
            top: 0,
            bottom: space::S4 as i8,
        }))
        .show(ui, |ui| column(ui, |ui| head(ui, p, meta.as_ref())));
    egui::Panel::bottom("composer")
        .frame(Frame::new().inner_margin(Margin {
            left: 0,
            right: 0,
            top: space::S4 as i8,
            bottom: space::S8 as i8,
        }))
        .show(ui, |ui| {
            column(ui, |ui| composer(ui, p, st, view, meta.as_ref(), &mut out));
        });
    ScrollArea::vertical()
        .id_salt(("chat-scroll", id.as_deref().unwrap_or("")))
        .auto_shrink(false)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            column(ui, |ui| {
                ui.spacing_mut().item_spacing.y = space::S6;
                let transcript = id.as_deref().and_then(|id| view.transcript(id));
                match (meta.as_ref(), transcript) {
                    (_, Some(tr)) if !tr.items.is_empty() => {
                        let mut prev_tool = false;
                        for item in &tr.items {
                            let tool = matches!(item, ChatItem::Tool { .. });
                            if tool && prev_tool {
                                // A run of tool calls reads as one list.
                                ui.add_space(space::S2 - space::S6);
                            }
                            prev_tool = tool;
                            item_ui(ui, p, &mut st.md, item);
                        }
                    }
                    (Some(m), None) if m.kind == ChatKind::Macro || m.session_id.is_some() => {
                        ui.label(txt(t("desk.chat.loading"), text::BASE, p.fg_muted));
                    }
                    (Some(m), _) if m.kind == ChatKind::Macro => {
                        ui.label(txt(t("desk.chat.macroEmpty"), text::BASE, p.fg_muted));
                    }
                    _ => {
                        if let Some(text) = empty_ask(ui, p) {
                            st.send(view, text, &mut out);
                        }
                    }
                }
                ui.add_space(space::S4);
            });
        });
    out
}

/// The centred reading column.
fn column<R>(ui: &mut Ui, body: impl FnOnce(&mut Ui) -> R) -> R {
    let w = ui.available_width().min(CHAT_MAX_WIDTH);
    let pad = ((ui.available_width() - w) / 2.0).max(0.0);
    ui.horizontal_top(|ui| {
        ui.add_space(pad);
        ui.vertical(|ui| {
            ui.set_width(w);
            body(ui)
        })
        .inner
    })
    .inner
}

fn head(ui: &mut Ui, p: &Palette, meta: Option<&ChatMeta>) {
    ui.horizontal(|ui| {
        let title = meta.map_or(t("sidebar.newChat"), |m| m.title.as_str());
        ui.add(egui::Label::new(bold(title, text::HEADING, p.fg)).truncate());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if let Some(m) = meta {
                if m.cost_usd > 0.0 {
                    ui.label(txt(cost_label(m.cost_usd), text::SM, p.fg_muted));
                }
                if m.busy {
                    ui.label(bold(t("chat.working"), text::SM, p.accent));
                }
            } else {
                ui.label(txt(t("desk.chat.creating"), text::SM, p.fg_muted));
            }
        });
    });
}

/// «Спроси агента» with six ready questions; returns the one clicked.
fn empty_ask(ui: &mut Ui, p: &Palette) -> Option<String> {
    let mut picked = None;
    ui.add_space(space::S12);
    ui.label(bold(t("home.askTitle"), text::TITLE, p.fg));
    kit::para(ui, txt(t("home.askSubtitle"), text::BASE, p.fg));
    kit::para(ui, txt(t("chat.emptyAsk"), text::MD, p.fg_muted));
    kit::grid2_cells(ui, |ui, col| {
        for key in SUGGESTIONS.iter().skip(col).step_by(2) {
            let b = egui::Button::new(txt(t(key), text::MD, p.fg))
                .fill(p.bg_elev)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(radius::SM))
                .wrap();
            if ui.add_sized([ui.available_width(), 36.0], b).clicked() {
                picked = Some(t(key).to_owned());
            }
        }
    });
    picked
}

fn item_ui(ui: &mut Ui, p: &Palette, md: &mut markdown::Cache, item: &ChatItem) {
    match item {
        ChatItem::User { text: s, .. } => {
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                let max = ui.available_width() * 0.8;
                Frame::new()
                    .fill(p.accent_soft)
                    .corner_radius(CornerRadius::same(radius::MD))
                    .inner_margin(Margin::symmetric(space::S6 as i8, space::S4 as i8))
                    .show(ui, |ui| {
                        ui.set_max_width(max);
                        ui.add(egui::Label::new(txt(s, text::BASE, p.fg)).wrap());
                    });
            });
        }
        ChatItem::Assistant { text: s, live, .. } => {
            if s.trim().is_empty() {
                if *live {
                    ui.label(txt(t("chat.thinking"), text::BASE, p.fg_muted));
                }
                return;
            }
            let blocks = md.get(s, *live);
            ui.vertical(|ui| markdown::show(ui, p, &blocks));
        }
        ChatItem::Tool {
            name, summary, ok, ..
        } => tool_chip(ui, p, name, summary, *ok),
        ChatItem::Notice { key, vars } => {
            let (fill, fg) = if is_error(key) {
                (p.err_soft, p.err)
            } else {
                (p.warn_soft, p.warn)
            };
            kit::tinted(ui, fill, |ui| {
                kit::para(ui, txt(notice_text(key, vars), text::MD, fg));
            });
        }
    }
}

/// `✓ tv_scan_levels  · 3 уровня` — a tool call in one line.
pub fn tool_chip(ui: &mut Ui, p: &Palette, name: &str, summary: &str, ok: Option<bool>) {
    let (glyph, color, state) = match ok {
        Some(true) => ("✓", p.ok, t("chat.toolOk")),
        Some(false) => ("✗", p.err, t("chat.toolErr")),
        None => ("…", p.fg_muted, t("chat.toolRunning")),
    };
    let r = Frame::new()
        .fill(p.bg_elev)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::SM))
        .inner_margin(Margin::symmetric(space::S5 as i8, space::S2 as i8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = space::S4;
                ui.label(bold(glyph, text::MD, color));
                ui.label(
                    egui::RichText::new(short_tool_name(name))
                        .font(FontId::monospace(text::SM))
                        .color(p.fg),
                );
                if !summary.is_empty() {
                    ui.add(egui::Label::new(txt(summary, text::SM, p.fg_muted)).truncate());
                }
            });
        })
        .response;
    let label = format!("{name}: {state}");
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &label));
    r.on_hover_text(name);
}

fn composer(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ChatUi,
    view: &ChatsView,
    meta: Option<&ChatMeta>,
    out: &mut Vec<ChatAction>,
) {
    let busy = meta.is_some_and(|m| m.busy);
    let key = meta.map_or(String::new(), |m| m.id.clone());
    let mut draft = st.drafts.remove(&key).unwrap_or_default();
    let mut send = false;
    ui.horizontal(|ui| {
        let button_w = 110.0;
        let edit = TextEdit::multiline(&mut draft)
            .id_salt(("composer", key.as_str()))
            .hint_text(t("composer.placeholder"))
            .font(FontId::proportional(text::BASE))
            .desired_rows(2)
            .desired_width(ui.available_width() - button_w - space::S4)
            .return_key(KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter));
        let r = ui.add_enabled(!busy, edit);
        if r.has_focus() && ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift) {
            send = true;
        }
        ui.with_layout(Layout::top_down(Align::Max), |ui| match (busy, meta) {
            (true, Some(m)) => {
                let b = egui::Button::new(bold(t("composer.stop"), text::BASE, p.err))
                    .stroke(Stroke::new(1.0, p.err));
                if ui.add_sized([button_w, 32.0], b).clicked() {
                    out.push(ChatAction::Stop {
                        chat_id: m.id.clone(),
                    });
                }
            }
            _ => {
                let can = !draft.trim().is_empty();
                let b = if can {
                    egui::Button::new(bold(t("composer.send"), text::BASE, p.accent_fg))
                        .fill(p.accent_solid)
                } else {
                    egui::Button::new(txt(t("composer.send"), text::BASE, p.fg_muted))
                };
                if ui
                    .add_enabled_ui(can, |ui| ui.add_sized([button_w, 32.0], b))
                    .inner
                    .clicked()
                {
                    send = true;
                }
            }
        });
        if send && !busy {
            r.request_focus();
        }
    });
    if send && !busy {
        let text = std::mem::take(&mut draft);
        st.send(view, text, out);
    }
    if !draft.is_empty() {
        st.drafts.insert(key, draft);
    }
}

// ---------------------------------------------------------------- the list

/// The day an `updated_at` falls on in Kyiv.
fn kyiv_day(at: DateTime<Utc>) -> NaiveDate {
    kyiv_now(at).date
}

/// «Сегодня / Вчера / За неделю / Раньше» for a chat last touched on `day`.
pub fn group_key(day: NaiveDate, today: NaiveDate) -> &'static str {
    let age = (today - day).num_days();
    match age {
        i64::MIN..=0 => "sidebar.today",
        1 => "sidebar.yesterday",
        2..=6 => "sidebar.week",
        _ => "sidebar.older",
    }
}

/// «Чаты» in the sidebar: «+ Новый чат», then the chats by age.
pub fn sidebar_list(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ChatUi,
    view: &ChatsView,
    today: NaiveDate,
    max_height: f32,
) -> Vec<ChatAction> {
    let mut out = Vec::new();
    ui.horizontal(|ui| {
        kit::sub(ui, p, t("sidebar.chats"));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let label = format!("+ {}", t("sidebar.newChat"));
            if ui
                .add(egui::Button::new(bold(label, text::MD, p.accent)).frame(false))
                .clicked()
            {
                st.start_new(&mut out);
            }
        });
    });
    if view.list.is_empty() {
        kit::para(ui, txt(t("sidebar.empty"), text::SM, p.fg_muted));
        return out;
    }
    let now = ui.input(|i| i.time);
    if st
        .confirm_delete
        .as_ref()
        .is_some_and(|(_, t0)| now - t0 > CONFIRM_S)
    {
        st.confirm_delete = None;
    }
    ScrollArea::vertical()
        .id_salt("chat-list")
        .max_height(max_height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = space::S1;
            let mut last_group = "";
            for m in &view.list {
                let g = group_key(kyiv_day(m.updated_at), today);
                if g != last_group {
                    last_group = g;
                    ui.add_space(space::S3);
                    ui.label(txt(t(g), text::XS, p.fg_muted));
                }
                row(ui, p, st, m, now, &mut out);
            }
        });
    out
}

fn row(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ChatUi,
    m: &ChatMeta,
    now: f64,
    out: &mut Vec<ChatAction>,
) {
    let selected = st.open_chat() == Some(m.id.as_str());
    // Inline rename.
    if let Some((id, title)) = &mut st.rename {
        if *id == m.id {
            let r = ui.add(
                TextEdit::singleline(title)
                    .id_salt(("rename", m.id.as_str()))
                    .desired_width(f32::INFINITY),
            );
            if !r.has_focus() && !r.lost_focus() {
                r.request_focus();
            }
            let enter = ui.input(|i| i.key_pressed(Key::Enter));
            let esc = ui.input(|i| i.key_pressed(Key::Escape));
            if r.lost_focus() || enter || esc {
                let new = title.trim().to_owned();
                if !esc && enter && !new.is_empty() && new != m.title {
                    out.push(ChatAction::Rename {
                        chat_id: m.id.clone(),
                        title: new,
                    });
                }
                st.rename = None;
            }
            return;
        }
    }
    let confirming = st
        .confirm_delete
        .as_ref()
        .is_some_and(|(id, _)| *id == m.id);
    let fill = if selected {
        p.accent_soft
    } else {
        egui::Color32::TRANSPARENT
    };
    let inner = Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(radius::SM))
        .inner_margin(Margin::symmetric(space::S4 as i8, space::S2 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let color = if selected { p.accent } else { p.fg };
                if confirming {
                    ui.label(txt(t("chat.confirmDelete"), text::MD, p.err));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(bold(t("chat.delete"), text::SM, p.err))
                                    .frame(false),
                            )
                            .clicked()
                        {
                            out.push(ChatAction::Delete {
                                chat_id: m.id.clone(),
                            });
                            st.confirm_delete = None;
                        }
                        if ui
                            .add(
                                egui::Button::new(txt(t("home.cancel"), text::SM, p.fg_muted))
                                    .frame(false),
                            )
                            .clicked()
                        {
                            st.confirm_delete = None;
                        }
                    });
                    return;
                }
                // The title takes what the «…» of a running turn leaves.
                let room = ui.available_width() - if m.busy { 18.0 } else { 0.0 };
                ui.scope(|ui| {
                    ui.set_max_width(room);
                    ui.add(egui::Label::new(txt(&m.title, text::MD, color)).truncate());
                });
                if m.busy {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(bold("…", text::MD, p.accent))
                            .on_hover_text(t("chat.working"));
                    });
                }
            });
        });
    if confirming {
        return;
    }
    let r = ui
        .interact(
            inner.response.rect,
            ui.scope_id().with(("chat-row", m.id.as_str())),
            Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let label = m.title.clone();
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, &label));
    if r.hovered() && !selected {
        ui.painter().rect_stroke(
            inner.response.rect,
            CornerRadius::same(radius::SM),
            Stroke::new(1.0, p.border),
            egui::StrokeKind::Inside,
        );
    }
    if r.clicked() {
        st.open(m, out);
    }
    r.context_menu(|ui| {
        if ui.button(t("chat.rename")).clicked() {
            st.rename = Some((m.id.clone(), m.title.clone()));
            ui.close();
        }
        if ui.button(t("chat.delete")).clicked() {
            st.confirm_delete = Some((m.id.clone(), now));
            ui.close();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn tool_names_lose_the_server_prefix() {
        assert_eq!(
            short_tool_name("mcp__tradingview__tv_scan_levels"),
            "tv_scan_levels"
        );
        assert_eq!(short_tool_name("mcp__gt-database__gt_search"), "gt_search");
        assert_eq!(short_tool_name("Bash"), "Bash");
        assert_eq!(short_tool_name("mcp__odd"), "odd");
    }

    #[test]
    fn groups_by_age() {
        let today = d("2026-10-26");
        assert_eq!(group_key(today, today), "sidebar.today");
        assert_eq!(group_key(d("2026-10-25"), today), "sidebar.yesterday");
        assert_eq!(group_key(d("2026-10-20"), today), "sidebar.week");
        assert_eq!(group_key(d("2026-10-19"), today), "sidebar.older");
        // A clock behind the file: still today.
        assert_eq!(group_key(d("2026-10-27"), today), "sidebar.today");
    }

    #[test]
    fn notices_fill_their_variables() {
        assert_eq!(
            notice_text(
                "notice.mcp",
                &[
                    ("name".into(), "tradingview".into()),
                    ("status".into(), "failed".into())
                ]
            ),
            "MCP-сервер tradingview: failed"
        );
        assert!(is_error("chat.error.no_claude"));
        assert_eq!(cost_label(0.123), "стоимость $0.12");
    }
}
