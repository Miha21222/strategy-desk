//! The chat's Markdown: `pulldown-cmark` parses into a small block model
//! once per message ([`Cache`]), the desk draws it with its own fonts and
//! palette. Only the live message is re-parsed while it streams.
//!
//! Why not `egui_commonmark` (0.26, egui 0.37): measured 2026-11-26 on the
//! release binary (Linux x86_64, glow, stripped). The parser costs 0.39 MB
//! either way (`pulldown-cmark`, no default features); egui_commonmark's
//! renderer adds another 0.24 MB on top of this module even without its
//! image loading, and it draws in its own styles, which the desk would have
//! to fight. What an agent's answer needs — paragraphs, headings, lists,
//! code, quotes, tables, links — fits in the few hundred lines below.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use eframe::egui::{
    self, text::LayoutJob, Align, Color32, CornerRadius, FontFamily, FontId, Frame, Margin, Stroke,
    TextFormat, Ui,
};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::i18n::t;
use crate::theme::{radius, space, text, Palette};

// ---------------------------------------------------------------- the model

/// Inline style bits.
pub const BOLD: u8 = 1;
pub const ITALIC: u8 = 2;
pub const CODE: u8 = 4;
pub const STRIKE: u8 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: u8,
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Para(Vec<Span>),
    Heading(u8, Vec<Span>),
    /// `start` = the first number of an ordered list.
    List {
        start: Option<u64>,
        items: Vec<Vec<Block>>,
    },
    Code {
        lang: String,
        text: String,
    },
    Quote(Vec<Block>),
    Rule,
    Table {
        head: Vec<Vec<Span>>,
        rows: Vec<Vec<Vec<Span>>>,
    },
}

/// One open container while parsing.
enum Open {
    Blocks(Vec<Block>),
    Quote(Vec<Block>),
    List {
        start: Option<u64>,
        items: Vec<Vec<Block>>,
    },
    Item(Vec<Block>),
    /// `implicit`: the text of a tight list item, no `Paragraph` around it.
    Para {
        spans: Vec<Span>,
        implicit: bool,
    },
    Heading(u8, Vec<Span>),
    Code {
        lang: String,
        text: String,
    },
    Table {
        head: Vec<Vec<Span>>,
        rows: Vec<Vec<Vec<Span>>>,
        row: Vec<Vec<Span>>,
    },
    Cell(Vec<Span>),
}

struct Builder {
    stack: Vec<Open>,
    style: u8,
    bold: u32,
    italic: u32,
    strike: u32,
    links: Vec<String>,
}

impl Builder {
    fn new() -> Self {
        Self {
            stack: vec![Open::Blocks(Vec::new())],
            style: 0,
            bold: 0,
            italic: 0,
            strike: 0,
            links: Vec::new(),
        }
    }

    fn restyle(&mut self) {
        self.style = (if self.bold > 0 { BOLD } else { 0 })
            | (if self.italic > 0 { ITALIC } else { 0 })
            | (if self.strike > 0 { STRIKE } else { 0 });
    }

    /// Hand a finished block to the container under it.
    fn push_block(&mut self, b: Block) {
        match self.stack.last_mut() {
            Some(Open::Blocks(v) | Open::Quote(v) | Open::Item(v)) => v.push(b),
            // A block where none belongs (not produced by pulldown): keep the
            // text rather than lose it.
            _ => {
                if let Some(Open::Blocks(v)) = self.stack.first_mut() {
                    v.push(b);
                }
            }
        }
    }

    fn close_implicit(&mut self) {
        if matches!(self.stack.last(), Some(Open::Para { implicit: true, .. })) {
            if let Some(Open::Para { spans, .. }) = self.stack.pop() {
                self.push_block(Block::Para(spans));
            }
        }
    }

    /// Where inline text goes; opens an implicit paragraph in a block container.
    fn spans(&mut self) -> Option<&mut Vec<Span>> {
        if matches!(
            self.stack.last(),
            Some(Open::Blocks(_) | Open::Quote(_) | Open::Item(_))
        ) {
            self.stack.push(Open::Para {
                spans: Vec::new(),
                implicit: true,
            });
        }
        match self.stack.last_mut() {
            Some(Open::Para { spans, .. } | Open::Heading(_, spans) | Open::Cell(spans)) => {
                Some(spans)
            }
            _ => None,
        }
    }

    fn inline(&mut self, s: &str, extra: u8) {
        if let Some(Open::Code { text, .. }) = self.stack.last_mut() {
            text.push_str(s);
            return;
        }
        let style = self.style | extra;
        let link = self.links.last().cloned();
        if let Some(spans) = self.spans() {
            match spans.last_mut() {
                Some(last) if last.style == style && last.link == link => last.text.push_str(s),
                _ => spans.push(Span {
                    text: s.to_owned(),
                    style,
                    link,
                }),
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Emphasis => {
                self.italic += 1;
                self.restyle();
            }
            Tag::Strong => {
                self.bold += 1;
                self.restyle();
            }
            Tag::Strikethrough => {
                self.strike += 1;
                self.restyle();
            }
            Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. } => {
                self.links.push(dest_url.into_string());
            }
            Tag::TableCell => self.stack.push(Open::Cell(Vec::new())),
            Tag::TableHead | Tag::TableRow => {}
            block => {
                self.close_implicit();
                let open = match block {
                    Tag::Paragraph => Open::Para {
                        spans: Vec::new(),
                        implicit: false,
                    },
                    Tag::Heading { level, .. } => Open::Heading(level as u8, Vec::new()),
                    Tag::BlockQuote(_) => Open::Quote(Vec::new()),
                    Tag::CodeBlock(kind) => Open::Code {
                        lang: match kind {
                            CodeBlockKind::Fenced(l) => {
                                l.split_whitespace().next().unwrap_or("").to_owned()
                            }
                            CodeBlockKind::Indented => String::new(),
                        },
                        text: String::new(),
                    },
                    Tag::List(start) => Open::List {
                        start,
                        items: Vec::new(),
                    },
                    Tag::Item => Open::Item(Vec::new()),
                    Tag::Table(_) => Open::Table {
                        head: Vec::new(),
                        rows: Vec::new(),
                        row: Vec::new(),
                    },
                    // Html blocks, footnotes, definition lists, metadata: their
                    // text lands in a plain container.
                    _ => Open::Quote(Vec::new()),
                };
                self.stack.push(open);
            }
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Emphasis => {
                self.italic = self.italic.saturating_sub(1);
                self.restyle();
            }
            TagEnd::Strong => {
                self.bold = self.bold.saturating_sub(1);
                self.restyle();
            }
            TagEnd::Strikethrough => {
                self.strike = self.strike.saturating_sub(1);
                self.restyle();
            }
            TagEnd::Link | TagEnd::Image => {
                self.links.pop();
            }
            TagEnd::TableCell => {
                if let Some(Open::Cell(spans)) = self.stack.pop() {
                    if let Some(Open::Table { row, .. }) = self.stack.last_mut() {
                        row.push(spans);
                    }
                }
            }
            TagEnd::TableHead => {
                if let Some(Open::Table { head, row, .. }) = self.stack.last_mut() {
                    *head = std::mem::take(row);
                }
            }
            TagEnd::TableRow => {
                if let Some(Open::Table { rows, row, .. }) = self.stack.last_mut() {
                    rows.push(std::mem::take(row));
                }
            }
            _ => {
                self.close_implicit();
                let Some(open) = self.stack.pop() else { return };
                match open {
                    Open::Blocks(v) => {
                        // Never popped by a well-formed stream; put it back.
                        self.stack.push(Open::Blocks(v));
                    }
                    Open::Para { spans, .. } => self.push_block(Block::Para(spans)),
                    Open::Heading(l, spans) => self.push_block(Block::Heading(l, spans)),
                    Open::Quote(v) => self.push_block(Block::Quote(v)),
                    Open::Code { lang, text } => self.push_block(Block::Code {
                        lang,
                        text: text.trim_end_matches('\n').to_owned(),
                    }),
                    Open::List { start, items } => self.push_block(Block::List { start, items }),
                    Open::Item(v) => {
                        if let Some(Open::List { items, .. }) = self.stack.last_mut() {
                            items.push(v);
                        }
                    }
                    Open::Table { head, rows, .. } => self.push_block(Block::Table { head, rows }),
                    Open::Cell(spans) => self.push_block(Block::Para(spans)),
                }
            }
        }
    }

    fn finish(mut self) -> Vec<Block> {
        // A live message stops anywhere: close whatever is still open.
        while self.stack.len() > 1 {
            self.end(TagEnd::Paragraph);
        }
        match self.stack.pop() {
            Some(Open::Blocks(v)) => v,
            _ => Vec::new(),
        }
    }
}

/// Markdown → blocks. Never fails: whatever pulldown reads is kept as text.
pub fn parse(src: &str) -> Vec<Block> {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut b = Builder::new();
    for ev in Parser::new_ext(src, opts) {
        match ev {
            Event::Start(tag) => b.start(tag),
            Event::End(tag) => b.end(tag),
            Event::Text(s) => b.inline(&s, 0),
            Event::Code(s) => b.inline(&s, CODE),
            Event::InlineMath(s) | Event::DisplayMath(s) => b.inline(&s, CODE),
            Event::Html(s) | Event::InlineHtml(s) => b.inline(&s, 0),
            Event::SoftBreak => b.inline(" ", 0),
            Event::HardBreak => b.inline("\n", 0),
            Event::Rule => {
                b.close_implicit();
                b.push_block(Block::Rule);
            }
            Event::TaskListMarker(done) => b.inline(if done { "[x] " } else { "[ ] " }, 0),
            Event::FootnoteReference(s) => b.inline(&format!("[{s}]"), 0),
        }
    }
    b.finish()
}

// ---------------------------------------------------------------- the cache

/// Parsed messages by the hash of their text. A message that stopped
/// streaming is parsed once; the live one is re-parsed when its text moved.
#[derive(Debug, Default)]
pub struct Cache {
    map: HashMap<u64, Arc<Vec<Block>>>,
    live: Option<(u64, Arc<Vec<Block>>)>,
    /// Parses done (tests).
    pub parses: u64,
}

/// Finished messages kept parsed; past this the cache starts over.
const CACHE_MAX: usize = 512;

fn hash(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

impl Cache {
    pub fn get(&mut self, src: &str, live: bool) -> Arc<Vec<Block>> {
        let k = hash(src);
        if live {
            if let Some((lk, b)) = &self.live {
                if *lk == k {
                    return b.clone();
                }
            }
            self.parses += 1;
            let b = Arc::new(parse(src));
            self.live = Some((k, b.clone()));
            return b;
        }
        if let Some(b) = self.map.get(&k) {
            return b.clone();
        }
        if self.map.len() >= CACHE_MAX {
            self.map.clear();
        }
        self.parses += 1;
        let b = Arc::new(parse(src));
        self.map.insert(k, b.clone());
        b
    }
}

// ---------------------------------------------------------------- drawing

fn format(p: &Palette, style: u8, size: f32, link: bool) -> TextFormat {
    let family = if style & CODE != 0 {
        FontFamily::Monospace
    } else if style & BOLD != 0 {
        crate::fonts::bold()
    } else {
        FontFamily::Proportional
    };
    let size = if style & CODE != 0 { size - 1.0 } else { size };
    let color = if link { p.accent } else { p.fg };
    TextFormat {
        font_id: FontId::new(size, family),
        color,
        italics: style & ITALIC != 0,
        background: if style & CODE != 0 {
            p.code_bg
        } else {
            Color32::TRANSPARENT
        },
        strikethrough: if style & STRIKE != 0 {
            Stroke::new(1.0, p.fg_muted)
        } else {
            Stroke::NONE
        },
        underline: if link {
            Stroke::new(1.0, p.accent)
        } else {
            Stroke::NONE
        },
        ..TextFormat::default()
    }
}

fn job(p: &Palette, spans: &[Span], size: f32, extra: u8) -> LayoutJob {
    let mut j = LayoutJob::default();
    for s in spans {
        j.append(
            &s.text,
            0.0,
            format(p, s.style | extra, size, s.link.is_some()),
        );
    }
    j
}

/// A run of spans: one selectable label, or (with links) one label per span
/// in a wrapped row, so a link can be clicked.
fn spans_ui(ui: &mut Ui, p: &Palette, spans: &[Span], size: f32, extra: u8) {
    if spans.iter().all(|s| s.link.is_none()) {
        ui.add(egui::Label::new(job(p, spans, size, extra)).wrap());
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for s in spans {
            let one = job(p, std::slice::from_ref(s), size, extra);
            match &s.link {
                Some(url) => {
                    let r = ui
                        .add(egui::Label::new(one).wrap().sense(egui::Sense::click()))
                        .on_hover_text(url.as_str())
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if r.clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                    }
                }
                None => {
                    ui.add(egui::Label::new(one).wrap());
                }
            }
        }
    });
}

/// Draw parsed blocks at the body size.
pub fn show(ui: &mut Ui, p: &Palette, blocks: &[Block]) {
    ui.spacing_mut().item_spacing.y = space::S4;
    for b in blocks {
        block(ui, p, b);
    }
}

fn block(ui: &mut Ui, p: &Palette, b: &Block) {
    match b {
        Block::Para(spans) => spans_ui(ui, p, spans, text::BASE, 0),
        Block::Heading(level, spans) => {
            let size = match level {
                1 => text::HEADING,
                2 => text::LG + 1.0,
                _ => text::LG,
            };
            ui.add_space(space::S2);
            spans_ui(ui, p, spans, size, BOLD);
        }
        Block::List { start, items } => {
            for (i, item) in items.iter().enumerate() {
                let marker = match start {
                    Some(n) => format!("{}.", n + i as u64),
                    None => "•".to_owned(),
                };
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = space::S3;
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(18.0, text::BASE + 4.0),
                        egui::Sense::hover(),
                    );
                    ui.painter().text(
                        rect.right_top() + egui::vec2(0.0, 1.0),
                        egui::Align2::RIGHT_TOP,
                        marker,
                        FontId::proportional(text::BASE),
                        p.fg_muted,
                    );
                    ui.vertical(|ui| show(ui, p, item));
                });
            }
        }
        Block::Code { lang, text: code } => code_block(ui, p, lang, code),
        Block::Quote(inner) => {
            let r = Frame::new()
                .inner_margin(Margin {
                    left: space::S8 as i8,
                    right: 0,
                    top: 0,
                    bottom: 0,
                })
                .show(ui, |ui| show(ui, p, inner))
                .response;
            ui.painter().vline(
                r.rect.left() + 2.0,
                r.rect.y_range(),
                Stroke::new(3.0, p.border),
            );
        }
        Block::Rule => {
            ui.separator();
        }
        Block::Table { head, rows } => table(ui, p, head, rows),
    }
}

fn code_block(ui: &mut Ui, p: &Palette, lang: &str, code: &str) {
    Frame::new()
        .fill(p.code_bg)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::SM))
        .inner_margin(Margin::symmetric(space::S5 as i8, space::S4 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(crate::screen::kit::txt(lang, text::XS, p.fg_muted));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    let id = ui.scope_id().with("copied");
                    let now = ui.input(|i| i.time);
                    let copied = ui
                        .ctx()
                        .data(|d| d.get_temp::<f64>(id))
                        .is_some_and(|t0| now - t0 < 1.5);
                    let label = if copied { t("md.copied") } else { t("md.copy") };
                    let b = egui::Button::new(crate::screen::kit::txt(label, text::XS, p.accent))
                        .frame(false);
                    if ui.add(b).clicked() {
                        ui.ctx().copy_text(code.to_owned());
                        ui.ctx().data_mut(|d| d.insert_temp(id, now));
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_millis(1600));
                    }
                });
            });
            egui::ScrollArea::horizontal()
                .id_salt(ui.scope_id().with("code"))
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    let mut j = LayoutJob::default();
                    j.append(code, 0.0, format(p, CODE, text::BASE, false));
                    j.wrap.max_width = f32::INFINITY;
                    ui.add(egui::Label::new(j).extend());
                });
        });
}

fn table(ui: &mut Ui, p: &Palette, head: &[Vec<Span>], rows: &[Vec<Vec<Span>>]) {
    egui::ScrollArea::horizontal()
        .id_salt(ui.scope_id().with("table"))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new(ui.scope_id().with("grid"))
                .striped(true)
                .spacing(egui::vec2(space::S8, space::S3))
                .show(ui, |ui| {
                    for cell in head {
                        ui.add(egui::Label::new(job(p, cell, text::MD, BOLD)));
                    }
                    ui.end_row();
                    for row in rows {
                        for cell in row {
                            ui.add(egui::Label::new(job(p, cell, text::MD, 0)));
                        }
                        ui.end_row();
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(s: &str) -> Span {
        Span {
            text: s.into(),
            style: 0,
            link: None,
        }
    }

    #[test]
    fn paragraphs_styles_and_links() {
        let b = parse("Уровень **15 230** и `ADR`, см. [заметку](https://x.y/22).\n\nВторой.");
        assert_eq!(b.len(), 2);
        let Block::Para(s) = &b[0] else {
            panic!("{b:?}")
        };
        assert_eq!(s[0], plain("Уровень "));
        assert_eq!(s[1].style, BOLD);
        assert_eq!(s[3].style, CODE);
        assert_eq!(s[5].link.as_deref(), Some("https://x.y/22"));
        assert_eq!(b[1], Block::Para(vec![plain("Второй.")]));
    }

    #[test]
    fn tight_and_nested_lists_keep_their_text() {
        let b = parse("1. раз\n2. два\n   - вложенный\n\n- конец");
        let Block::List { start, items } = &b[0] else {
            panic!("{b:?}")
        };
        assert_eq!(*start, Some(1));
        assert_eq!(items[0], vec![Block::Para(vec![plain("раз")])]);
        assert!(matches!(&items[1][1], Block::List { start: None, .. }));
        assert!(matches!(&b[1], Block::List { start: None, .. }));
    }

    #[test]
    fn code_tables_quotes_rules() {
        let src = "```python\nprint(1)\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n> цитата\n\n---";
        let b = parse(src);
        assert_eq!(
            b[0],
            Block::Code {
                lang: "python".into(),
                text: "print(1)".into()
            }
        );
        let Block::Table { head, rows } = &b[1] else {
            panic!("{b:?}")
        };
        assert_eq!(head.len(), 2);
        assert_eq!(rows, &vec![vec![vec![plain("1")], vec![plain("2")]]]);
        assert_eq!(b[2], Block::Quote(vec![Block::Para(vec![plain("цитата")])]));
        assert_eq!(b[3], Block::Rule);
    }

    #[test]
    fn a_message_cut_mid_stream_still_parses() {
        for cut in [
            "**жир",
            "```rust\nfn x",
            "| a |\n|---|\n| 1",
            "- раз\n- дв",
            "> ци",
        ] {
            let b = parse(cut);
            assert!(!b.is_empty(), "{cut:?}");
        }
    }

    #[test]
    fn finished_messages_are_parsed_once_the_live_one_on_change() {
        let mut c = Cache::default();
        c.get("один", false);
        c.get("один", false);
        assert_eq!(c.parses, 1);
        c.get("жив", true);
        c.get("жив", true);
        assert_eq!(c.parses, 2);
        c.get("живой", true);
        assert_eq!(c.parses, 3);
    }
}
