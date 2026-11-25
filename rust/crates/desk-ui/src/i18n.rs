//! Russian UI strings behind stable keys, the Rust twin of `src/renderer/i18n`.
//!
//! `t("macro.gauge")` returns the string; an unknown key returns the key
//! itself (same rule as the TS `t()`), so a typo shows up on screen instead of
//! crashing. Keys that already exist in `ru.ts` keep their spelling there;
//! keys new to the Rust desk live under `desk.*`.

use std::fmt::Display;

/// The whole dictionary. A flat slice instead of a map: it costs nothing at
/// start-up (no hashing before the first frame) and a linear scan over a few
/// dozen entries is a handful of nanoseconds per lookup.
pub(crate) static RU: &[(&str, &str)] = &[
    ("app.title", "Strategy Desk"),
    ("tabs.macro", "Обзор дня"),
    // ---- macro (keys from ru.ts) ----
    ("macro.title", "Обзор дня"),
    ("macro.gauge", "Перевес лонг / шорт"),
    ("macro.long", "Лонг"),
    ("macro.short", "Шорт"),
    ("macro.forLong", "за лонг"),
    ("macro.forShort", "за шорт"),
    ("macro.undecided", "неясно"),
    (
        "macro.splitAria",
        "за лонг {l} %, неясно {u} %, за шорт {s} %",
    ),
    ("macro.share", "вес"),
    ("macro.beforeTalk", "до обсуждения было {v}"),
    ("macro.zone.strong-short", "сильный шорт"),
    ("macro.zone.short", "шорт"),
    ("macro.zone.neutral", "нет перевеса"),
    ("macro.zone.long", "лонг"),
    ("macro.zone.strong-long", "сильный лонг"),
    ("macro.head.strong-short", "Всё указывает вниз. Ищем шорт."),
    ("macro.head.short", "Скорее вниз. Ищем шорт."),
    (
        "macro.head.neutral",
        "Явного перевеса нет — по правилам день пропускаем.",
    ),
    ("macro.head.long", "Скорее вверх. Ищем лонг."),
    ("macro.head.strong-long", "Всё указывает вверх. Ищем лонг."),
    ("macro.lightGreen", "Можно торговать, риск {r} %"),
    ("macro.lightYellow", "Торгуем осторожно, риск {r} %"),
    ("macro.lightRed", "Сегодня не торгуем"),
    ("macro.whereTitle", "Куда смотрит рынок"),
    ("macro.groups", "Откуда перевес"),
    ("macro.groupsNote", "Влево — в шорт, вправо — в лонг."),
    ("macro.today", "Сегодня"),
    // ---- sidebar (keys from ru.ts) ----
    ("sidebar.simple.title", "Подключения"),
    ("sidebar.chats", "Чаты"),
    ("sidebar.newChat", "Новый чат"),
    ("sidebar.empty", "Пока нет чатов — начните новый."),
    // ---- new in the Rust desk ----
    (
        "desk.soon",
        "Скоро здесь появятся лампочки подключений и чаты.",
    ),
    ("desk.demo.title", "Демо"),
    ("desk.demo.needle", "Стрелка"),
    ("desk.demo.ghost", "Показать «до обсуждения»"),
    ("desk.demo.theme", "Тема"),
    ("desk.theme.system", "Как в системе"),
    ("desk.theme.light", "Светлая"),
    ("desk.theme.dark", "Тёмная"),
    (
        "desk.mockNote",
        "Пример данных: настоящий обзор появится, когда подключим папку проекта.",
    ),
];

/// Looks a key up; an unknown key comes back as itself.
pub fn t(key: &str) -> &str {
    RU.iter().find(|(k, _)| *k == key).map_or(key, |(_, v)| *v)
}

/// `t()` plus `{name}` placeholders, e.g. `tf("macro.beforeTalk", &[("v", &"+12%")])`.
pub fn tf(key: &str, vars: &[(&str, &dyn Display)]) -> String {
    let mut s = t(key).to_owned();
    for (name, value) in vars {
        s = s.replace(&format!("{{{name}}}"), &value.to_string());
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_key_is_russian() {
        assert_eq!(t("macro.gauge"), "Перевес лонг / шорт");
    }

    #[test]
    fn unknown_key_is_itself() {
        assert_eq!(t("macro.nope"), "macro.nope");
    }

    #[test]
    fn placeholders_are_filled() {
        assert_eq!(
            tf("macro.splitAria", &[("l", &55), ("u", &15), ("s", &30)]),
            "за лонг 55 %, неясно 15 %, за шорт 30 %"
        );
        // A variable the string does not use is ignored.
        assert_eq!(tf("macro.long", &[("x", &1)]), "Лонг");
    }

    #[test]
    fn keys_are_unique() {
        let mut keys: Vec<_> = RU.iter().map(|(k, _)| *k).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len(), "duplicate i18n key");
    }
}
