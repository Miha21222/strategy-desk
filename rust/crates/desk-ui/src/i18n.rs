//! Russian UI strings behind stable keys, the Rust twin of `src/renderer/i18n`.
//!
//! `t("macro.gauge")` returns the string; an unknown key returns the key
//! itself (same rule as the TS `t()`), so a typo shows up on screen instead of
//! crashing. The `app`, `tabs`, `macro`, `sidebar`, `home`, `chat`, `composer`
//! `notice` and `md` sections are generated
//! from `ru.ts` into [`crate::i18n_ru`] (`tools/gen-i18n.mjs`); keys new to the
//! Rust desk live here under `desk.*` and `error.*` (desk-app's error keys).
//!
//! [`THESIS`] overrides some `ru.ts` strings with the owner's thesis style
//! (25.11: «тезисность, как в скилле, но без жертв для смысла»): one idea per
//! piece, pieces joined with « · », every fact kept. The TS app keeps its own
//! wording until it is retired.

use std::fmt::Display;

use crate::i18n_ru::RU_TS;

/// The Rust desk's wording of strings `ru.ts` also has; looked up first.
pub(crate) static THESIS: &[(&str, &str)] = &[
    ("macro.yourReadAsk.eu", "Окно Европы, {open}–{until}: куда пойдёт?"),
    ("macro.yourReadAsk.us", "Окно США, {open}–{until}: куда пойдёт?"),
    ("macro.yourReadWhy", "Ответь — откроется обзор · вечером сверим: ты или шкала"),
    ("macro.whyHint", "Нажми на довод — что он значит и почему он сегодня такой"),
    ("macro.noData", "Обзора не было"),
    ("macro.noReview", "Итог дня — в 20:00"),
    ("macro.noReviewRegion", "Обзора не было — нечего оценивать"),
    ("macro.noOpinion", "Мнение агента пишется…"),
    ("macro.noReflection", "Разбор агента пишется…"),
    ("macro.groupsNote", "← в шорт · в лонг →"),
    ("macro.factorNote", "«Дал» = вес × сила сегодня · за лонг − за шорт = стрелка"),
    (
        "macro.statsNote",
        "Ниже 50 % — кандидат на меньший вес, от 60 % — на больший · меняем раз в месяц, только с твоего «да»",
    ),
    (
        "macro.yourDayWhy",
        "От тебя вечером — только это · остальное агент свяжет с правилами и запомнит",
    ),
    ("macro.yourDayQ2", "Что помогло или помешало — одной строкой"),
    ("macro.agentIdle.eu", "Обзор Европы — сам в {at} · можно запустить сейчас"),
    ("macro.agentIdle.us", "Обзор США — сам в {at} · можно запустить сейчас"),
    ("macro.agentIdle.review", "Итог дня — сам в {at} · можно запустить сейчас"),
    ("macro.agentPartial.eu", "Обзор Европы собирается · мнение агента пишется"),
    ("macro.agentPartial.us", "Обзор США собирается · мнение агента пишется"),
    ("macro.agentPartial.review", "Итог дня собирается · разбор агента пишется"),
    ("macro.rerunCost", "Заново — несколько минут и токены агента"),
    (
        "macro.demoBanner",
        "Демо: придуманные дни 14–24 сентября · запуск обзоров и доводы выключены",
    ),
    ("macro.head.strong-short", "Всё указывает вниз · ищем шорт"),
    ("macro.head.short", "Скорее вниз · ищем шорт"),
    ("macro.head.neutral", "Перевеса нет · по правилам день пропускаем"),
    ("macro.head.long", "Скорее вверх · ищем лонг"),
    ("macro.head.strong-long", "Всё указывает вверх · ищем лонг"),
    (
        "macro.headRed.strong-short",
        "Всё указывает вниз · сегодня не торгуем, только наблюдаем",
    ),
    ("macro.headRed.short", "Скорее вниз · сегодня не торгуем, только наблюдаем"),
    ("macro.headRed.neutral", "Перевеса нет · сегодня не торгуем"),
    ("macro.headRed.long", "Скорее вверх · сегодня не торгуем, только наблюдаем"),
    (
        "macro.headRed.strong-long",
        "Всё указывает вверх · сегодня не торгуем, только наблюдаем",
    ),
    ("macro.more", "Подробнее: уровни, все факторы, группы, фон дня"),
    ("macro.moreReview", "Подробнее: попадания факторов, статистика, все уроки"),
    ("macro.noVault", "Укажи папку Trading Strategy Assistant · обзоры берутся оттуда"),
    ("macro.factorNoAbout", "Описания пока нет · см. заметку 22"),
    ("macro.factorWeight", "Вес {w} % · сработал на {s} % → дал {p} %"),
];

/// Strings the TS desk does not have. Few, so a linear scan is fine.
pub(crate) static DESK: &[(&str, &str)] = &[
    ("desk.soon", "Появится в следующих версиях"),
    ("desk.chat.loading", "Загружаю разговор…"),
    ("desk.chat.creating", "Создаю чат…"),
    (
        "desk.chat.macroEmpty",
        "Здесь будет разговор с агентом об обзоре этого дня: запусти обзор на экране «Обзор дня».",
    ),
    (
        "chat.error.no_claude",
        "Не нашли Claude Code на этом компьютере. Мастер настройки поможет его подключить — он появится в следующей версии Desk.",
    ),
    ("chat.error.failed", "Ответ прервался: {detail}"),
    ("desk.silent", "молчат:"),
    ("desk.hitShort", "{h} из {n}"),
    (
        "desk.hitTip",
        "этот довод был прав {h} раз из {n} за последние торговые дни",
    ),
    ("desk.newsCal", "Новости и календарь"),
    (
        "desk.headlines",
        "Заголовки: {n} · по делу {r} · тон {tone}",
    ),
    ("desk.newsMore", "Причины, риски, источники"),
    ("desk.similar.title", "Похожие дни"),
    ("desk.similar.matched", "совпало: {x}"),
    ("desk.similar.day", "день: {x}"),
    ("desk.similar.ib", "от IB: {x}"),
    (
        "desk.similar.args",
        "сегодняшние доводы в тот день: {h} из {n}",
    ),
    ("desk.similar.verdict", "разбор: {x}"),
    ("desk.ib.IB1", "до IB1"),
    ("desk.ib.IB2", "до IB2"),
    ("desk.ib.IB", "пробой без IB1"),
    ("desk.ib.none", "из IB не вышли"),
    ("desk.theme.title", "Тема"),
    ("desk.theme.system", "Как в системе"),
    ("desk.theme.light", "Светлая"),
    ("desk.theme.dark", "Тёмная"),
    ("desk.loading", "Загружаю день…"),
    ("desk.noReviewPast", "Итога дня не было"),
    (
        "desk.noVaultHint",
        "Пока папку задаёт запуск: --vault <папка> или DESK_VAULT · выбор папки — в настройках",
    ),
    ("desk.days", "Дни"),
    ("desk.markWindow", "есть обзор"),
    ("desk.markReview", "есть итог дня"),
    ("desk.close", "Закрыть"),
    (
        "desk.zoomHint",
        "колёсико — масштаб · тяни — сдвиг · ←/→ — другой снимок · Esc — закрыть",
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
    ("error.not_implemented", "Появится в следующей версии Desk"),
    ("error.vault.not_set", "Папка проекта не указана."),
    (
        "error.vault.read_failed",
        "Папка проекта не читается: {detail}",
    ),
    (
        "error.vault.watch_failed",
        "Не слежу за папкой проекта — обновляй день вручную · {detail}",
    ),
];

/// Looks a key up; an unknown key comes back as itself.
pub fn t(key: &str) -> &str {
    if let Some((_, v)) = THESIS.iter().find(|(k, _)| *k == key) {
        return v;
    }
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
    RU_TS.iter().chain(DESK).chain(THESIS).map(|(_, v)| *v)
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
        // An override must override something, and keep the placeholders.
        for (k, v) in THESIS {
            let i = RU_TS.binary_search_by(|(x, _)| (*x).cmp(k));
            let Ok(i) = i else {
                panic!("THESIS key not in ru.ts: {k}")
            };
            for var in ["{r}", "{w}", "{s}", "{p}", "{n}", "{x}"] {
                assert_eq!(RU_TS[i].1.contains(var), v.contains(var), "{k}: {var}");
            }
        }
    }
}
