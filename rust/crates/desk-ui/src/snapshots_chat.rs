//! kittest tests of the chat (stage 5) against a hand-made `ChatsView`: the
//! backend is being built next door, so the chats are written out here the
//! way CONTRACTS «Чат (этап 5)» describes them.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use desk_app::{ChatCommand, Command};
use desk_core::WindowId;
use eframe::egui::{self, Theme};
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_kittest::Harness;

use crate::app::DeskApp;
use crate::chat::{ChatItem, ChatKind, ChatMeta, ChatsView, Page, Transcript};
use crate::snapshots::{d, has, screen_sized, snapshot_options, theme_name, TODAY};

const MACRO_ID: &str = "m-2026-10-26";
const ASK_ID: &str = "a-1";

/// An answer with everything the renderer knows.
const ANSWER: &str = "## Что сейчас\n\nЕвропа открылась **выше** вчерашнего VAH, \
премаркет снял *ночной* максимум. Уровень `15 230` держится.\n\n\
1. Первый тест VAH — смотреть реакцию.\n2. Пробой ночного минимума отменяет лонг.\n\n\
| Фактор | Сторона | Вес |\n|---|---|---|\n| bias_a | лонг | 18 % |\n| night_sweep | шорт | 9 % |\n\n\
```bash\npython indices-kb/tools/scale.py review eu 2026-10-26\n```\n\n> Помни: в первые 15 минут не входить.\n\nСобираю итог";

fn at(h: u32, m: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 26, h, m, 0).unwrap()
}

fn meta(id: &str, kind: ChatKind, title: &str, busy: bool) -> ChatMeta {
    ChatMeta {
        id: id.into(),
        kind,
        title: title.into(),
        date: (kind == ChatKind::Macro).then(|| d("2026-10-26")),
        created_at: at(6, 30),
        updated_at: at(6, 40),
        session_id: Some("s".into()),
        busy,
        cost_usd: 0.0,
    }
}

/// The day's macro chat mid-review, an «Ask» chat that ended on a notice,
/// and two older chats for the list's groups.
pub(crate) fn sample_chats(busy: bool) -> ChatsView {
    let mut m = meta(MACRO_ID, ChatKind::Macro, "Обзор дня 26.10", busy);
    m.cost_usd = 0.42;
    let mut ask = meta(ASK_ID, ChatKind::Ask, "Какой урок повторяется?", false);
    ask.updated_at = Utc.with_ymd_and_hms(2026, 10, 25, 15, 0, 0).unwrap();
    let mut old = meta("a-0", ChatKind::Ask, "Разбор сентября", false);
    old.updated_at = Utc.with_ymd_and_hms(2026, 9, 30, 15, 0, 0).unwrap();
    let mut week = meta("m-22", ChatKind::Macro, "Обзор дня 22.10", false);
    week.date = Some(d("2026-10-22"));
    week.updated_at = Utc.with_ymd_and_hms(2026, 10, 22, 18, 0, 0).unwrap();
    let macro_tr = Transcript {
        items: vec![
            ChatItem::User {
                text: "/macro-review eu 2026-10-26".into(),
                at: at(6, 30),
            },
            ChatItem::Tool {
                id: "t1".into(),
                name: "mcp__tradingview__tv_scan_levels".into(),
                summary: "FDAX1! · 6 уровней".into(),
                ok: Some(true),
            },
            ChatItem::Tool {
                id: "t2".into(),
                name: "Bash".into(),
                summary: "python scale.py drift — нет файла".into(),
                ok: Some(false),
            },
            ChatItem::Tool {
                id: "t3".into(),
                name: "Write".into(),
                summary: "state/scale_2026-10-26_eu.json".into(),
                ok: None,
            },
            ChatItem::Assistant {
                text: ANSWER.into(),
                at: at(6, 40),
                live: busy,
            },
        ],
    };
    let ask_tr = Transcript {
        items: vec![
            ChatItem::User {
                text: "Какой урок повторяется?".into(),
                at: at(6, 0),
            },
            ChatItem::Assistant {
                text: "Чаще всего — **вход до конца IB**: 4 раза за октябрь.".into(),
                at: at(6, 1),
                live: false,
            },
            ChatItem::Notice {
                key: "chat.notice.stopped".into(),
                vars: Vec::new(),
            },
            ChatItem::Notice {
                key: "chat.error.spawn".into(),
                vars: vec![("detail".into(), "exit code 1".into())],
            },
        ],
    };
    ChatsView {
        list: vec![m, ask, week, old],
        open: None,
        transcripts: HashMap::from([
            (MACRO_ID.to_owned(), Arc::new(macro_tr)),
            (ASK_ID.to_owned(), Arc::new(ask_tr)),
        ]),
    }
}

fn harness(theme: Theme, chats: ChatsView, page: Page, height: f32) -> Harness<'static, DeskApp> {
    let mut h = screen_sized(theme, TODAY, "2026-10-26", Some(WindowId::Eu), height);
    h.state_mut().set_chats(chats);
    h.state_mut().chat_mut().page = page;
    h
}

fn chat_cmds(h: &Harness<'_, DeskApp>) -> Vec<ChatCommand> {
    h.state()
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Chat(c) => Some(c.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn list_groups_the_chats_by_age() {
    let mut h = harness(Theme::Light, sample_chats(false), Page::Day, 1000.0);
    h.run();
    for s in [
        "Сегодня",
        "Вчера",
        "За неделю",
        "Раньше",
        "Обзор дня 26.10",
        "Какой урок повторяется?",
        "Обзор дня 22.10",
        "Разбор сентября",
        "+ Новый чат",
    ] {
        assert!(has(&h, s), "missing: {s}");
    }
    assert!(!has(&h, "Пока нет чатов"));
}

#[test]
fn a_streaming_chat_shows_tools_markdown_cost_and_stop() {
    let mut h = harness(
        Theme::Light,
        sample_chats(true),
        Page::Chat(Some(MACRO_ID.into())),
        1000.0,
    );
    h.run();
    for s in [
        "Работает…",
        "стоимость $0.42",
        "/macro-review eu 2026-10-26",
        "mcp__tradingview__tv_scan_levels: ок",
        "Bash: ошибка",
        "Write: …",
        "tv_scan_levels",
        "FDAX1! · 6 уровней",
        "Что сейчас",
        "Первый тест VAH",
        "night_sweep",
        "scale.py review",
        "Помни: в первые 15 минут",
        "Копировать",
        "Стоп",
    ] {
        assert!(has(&h, s), "missing: {s}");
    }
    assert!(!has(&h, "Отправить"), "a busy chat has no send button");
    // The composer is disabled while the turn runs.
    let input = h.get_by_role(egui::accesskit::Role::MultilineTextInput);
    assert!(input.accesskit_node().is_disabled());
    h.get_by_label("Стоп").click();
    h.run();
    assert_eq!(
        chat_cmds(&h),
        vec![ChatCommand::Stop {
            chat_id: MACRO_ID.into()
        }]
    );
}

#[test]
fn enter_sends_into_an_idle_chat() {
    let mut h = harness(
        Theme::Light,
        sample_chats(false),
        Page::Chat(Some(ASK_ID.into())),
        900.0,
    );
    h.run();
    assert!(has(&h, "вход до конца IB"));
    assert!(has(&h, "Остановлено."));
    assert!(
        has(&h, "Claude Code не запустился: exit code 1"),
        "vars filled"
    );
    let input = h.get_by_role(egui::accesskit::Role::MultilineTextInput);
    input.focus();
    input.type_text("А в ноябре?");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(
        chat_cmds(&h),
        vec![ChatCommand::Send {
            chat_id: ASK_ID.into(),
            text: "А в ноябре?".into()
        }]
    );
}

#[test]
fn new_chat_asks_first_and_sends_once_it_exists() {
    let mut h = harness(Theme::Light, sample_chats(false), Page::Day, 900.0);
    h.run();
    h.get_by_label("+ Новый чат").click();
    h.run();
    assert_eq!(
        chat_cmds(&h),
        vec![ChatCommand::New {
            kind: ChatKind::Ask
        }]
    );
    for s in [
        "Спроси агента",
        "Был ли похожий день? Чем кончился?",
        "Создаю чат…",
    ] {
        assert!(has(&h, s), "missing: {s}");
    }
    h.get_by_label("Был ли похожий день? Чем кончился?").click();
    h.run();
    // desk-app made the chat and opened it; an «Ask» chat has no title
    // until its first message names it.
    let mut chats = sample_chats(false);
    chats
        .list
        .insert(0, meta("a-new", ChatKind::Ask, "", false));
    chats.open = Some("a-new".into());
    h.state_mut().set_chats(chats);
    h.run();
    assert_eq!(
        h.state_mut().chat_mut().page,
        Page::Chat(Some("a-new".into()))
    );
    assert!(
        h.query_all_by_label("Новый чат").count() >= 2,
        "the untitled chat is «Новый чат» in the head and the list"
    );
    assert_eq!(
        chat_cmds(&h)[1],
        ChatCommand::Send {
            chat_id: "a-new".into(),
            text: "Был ли похожий день? Чем кончился?".into()
        }
    );
}

#[test]
fn agent_strip_follows_the_macro_chat() {
    // Busy: the button waits, the last words stream under it, the chat opens.
    let mut h = harness(Theme::Light, sample_chats(true), Page::Day, 1900.0);
    h.run();
    assert!(has(&h, "Агент готовит обзор…"));
    // 26.10 EU waits for the owner's read: the agent's words wait with it.
    assert!(!has(&h, "Собираю итог"));
    h.get_by_label("Показать обзор без ответа").click();
    h.run();
    assert!(has(&h, "Собираю итог"));
    h.get_by_label("Весь разговор").click();
    h.run();
    assert_eq!(
        h.state_mut().chat_mut().page,
        Page::Chat(Some(MACRO_ID.into()))
    );
    assert_eq!(
        chat_cmds(&h),
        vec![ChatCommand::Open {
            chat_id: MACRO_ID.into()
        }]
    );
    // The sidebar tab brings the day back.
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Обзор дня")
        .click();
    h.run();
    assert_eq!(h.state_mut().chat_mut().page, Page::Day);
}

#[test]
fn run_is_a_chat_action_outside_the_demo() {
    let mut h = harness(Theme::Light, ChatsView::default(), Page::Day, 1900.0);
    // The demo vault blocks running; a real vault does not.
    let mut snap = h.state().snapshot().clone();
    let mut index = (*snap.index).clone();
    index.demo = None;
    snap.index = Arc::new(index);
    h.state_mut().set_snapshot(snap);
    h.run();
    // 26.10 EU has a review: «Пересобрать» asks twice.
    h.get_by_label("Пересобрать обзор").click();
    h.run();
    h.get_by_label_contains("Да, пересобрать").click();
    h.run();
    assert_eq!(
        chat_cmds(&h),
        vec![ChatCommand::Run {
            window: WindowId::Eu,
            date: d("2026-10-26")
        }]
    );
}

/// The chat page in both themes: a streaming review, and an «Ask» chat.
#[cfg(target_os = "linux")]
#[test]
fn chat_snapshots() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in [Theme::Dark, Theme::Light] {
        for (name, chats, page) in [
            (
                "stream",
                sample_chats(true),
                Page::Chat(Some(MACRO_ID.into())),
            ),
            ("ask", sample_chats(false), Page::Chat(Some(ASK_ID.into()))),
            ("new", sample_chats(false), Page::Chat(None)),
            ("day_strip", sample_chats(true), Page::Day),
        ] {
            let mut h = harness(theme, chats, page, 900.0);
            h.run();
            let name = format!("chat_{}_{name}", theme_name(theme));
            results.add(h.try_snapshot_options(name, &snapshot_options()));
        }
    }
}
