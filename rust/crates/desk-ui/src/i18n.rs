//! Russian UI strings behind stable keys, the Rust twin of `src/renderer/i18n`.
//!
//! `t("macro.gauge")` returns the string; an unknown key returns the key
//! itself (same rule as the TS `t()`), so a typo shows up on screen instead of
//! crashing. The `app`, `tabs`, `macro` and `sidebar` sections are generated
//! from `ru.ts` into [`crate::i18n_ru`] (`tools/gen-i18n.mjs`); keys new to the
//! Rust desk live here under `desk.*` and `error.*` (desk-app's error keys).

use std::fmt::Display;

use crate::i18n_ru::RU_TS;

/// Strings the TS desk does not have. Few, so a linear scan is fine.
pub(crate) static DESK: &[(&str, &str)] = &[
    ("desk.soon", "Скоро здесь появятся лампочки подключений и чаты."),
    ("desk.theme.title", "Тема"),
    ("desk.theme.system", "Как в системе"),
    ("desk.theme.light", "Светлая"),
    ("desk.theme.dark", "Тёмная"),
    ("desk.loading", "Загружаю день…"),
    (
        "desk.noVaultHint",
        "Пока папка задаётся при запуске: strategy-desk --vault <папка> или переменная DESK_VAULT. Выбор папки появится в настройках.",
    ),
    ("desk.days", "Дни"),
    ("desk.markWindow", "есть обзор"),
    ("desk.markReview", "есть итог дня"),
    ("desk.close", "Закрыть"),
    (
        "desk.zoomHint",
        "Колёсико — крупнее или мельче, перетаскивание — сдвиг, ←/→ — другой снимок, Esc — закрыть",
    ),
    ("desk.zoomReset", "Вписать"),
    ("desk.weekday.1", "понедельник"),
    ("desk.weekday.2", "вторник"),
    ("desk.weekday.3", "среда"),
    ("desk.weekday.4", "четверг"),
    ("desk.weekday.5", "пятница"),
    ("desk.weekday.6", "суббота"),
    ("desk.weekday.7", "воскресенье"),
    ("desk.wd.1", "пн"),
    ("desk.wd.2", "вт"),
    ("desk.wd.3", "ср"),
    ("desk.wd.4", "чт"),
    ("desk.wd.5", "пт"),
    ("desk.wd.6", "сб"),
    ("desk.wd.7", "вс"),
    ("desk.month.1", "января"),
    ("desk.month.2", "февраля"),
    ("desk.month.3", "марта"),
    ("desk.month.4", "апреля"),
    ("desk.month.5", "мая"),
    ("desk.month.6", "июня"),
    ("desk.month.7", "июля"),
    ("desk.month.8", "августа"),
    ("desk.month.9", "сентября"),
    ("desk.month.10", "октября"),
    ("desk.month.11", "ноября"),
    ("desk.month.12", "декабря"),
    // desk-app's `UserError` keys (crates/desk-app/src/app.rs `keys`).
    ("error.not_implemented", "Это появится в следующей версии Desk."),
    ("error.vault.not_set", "Папка проекта не указана."),
    (
        "error.vault.read_failed",
        "Не получилось прочитать папку проекта: {detail}",
    ),
    (
        "error.vault.watch_failed",
        "Не получилось следить за папкой проекта — обновляй день вручную. {detail}",
    ),
];

/// Looks a key up; an unknown key comes back as itself.
pub fn t(key: &str) -> &str {
    if let Ok(i) = RU_TS.binary_search_by(|(k, _)| (*k).cmp(key)) {
        return RU_TS[i].1;
    }
    DESK.iter()
        .find(|(k, _)| *k == key)
        .map_or(key, |(_, v)| *v)
}

/// `t()` plus `{name}` placeholders, e.g. `tf("macro.beforeTalk", &[("v", &"+12%")])`.
pub fn tf(key: &str, vars: &[(&str, &dyn Display)]) -> String {
    let mut s = t(key).to_owned();
    for (name, value) in vars {
        s = s.replace(&format!("{{{name}}}"), &value.to_string());
    }
    s
}

/// Every string the desk can show (tests: glyph coverage).
#[cfg(test)]
pub(crate) fn all() -> impl Iterator<Item = &'static str> {
    RU_TS.iter().chain(DESK).map(|(_, v)| *v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_key_is_russian() {
        assert_eq!(t("macro.gauge"), "Перевес лонг / шорт");
        assert_eq!(t("desk.month.10"), "октября");
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
    fn generated_table_is_sorted_and_unique() {
        assert!(
            RU_TS.windows(2).all(|w| w[0].0 < w[1].0),
            "RU_TS must be sorted"
        );
        let mut keys: Vec<_> = DESK.iter().map(|(k, _)| *k).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len(), "duplicate desk key");
        assert!(DESK
            .iter()
            .all(|(k, _)| RU_TS.binary_search_by(|(x, _)| (*x).cmp(k)).is_err()));
    }
}
