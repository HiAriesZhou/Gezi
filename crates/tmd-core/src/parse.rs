//! Markdown events to a small block tree that layout can measure.

use std::ops::Range;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::html::{self, Piece};
use crate::line::Span;
use crate::style::Style;
use crate::theme::Theme;
use crate::width::is_wide;

/// Spaces a tab expands to in code and HTML blocks.
const TAB_WIDTH: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Block {
    /// Inline content; a span whose text is `"\n"` is a hard line break.
    Paragraph(Vec<Span>),
    Heading {
        level: u8,
        spans: Vec<Span>,
    },
    Code {
        lang: Option<String>,
        text: String,
    },
    List {
        start: Option<u64>,
        tight: bool,
        items: Vec<Item>,
    },
    Quote(Vec<Block>),
    Rule,
    Table(Table),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Align {
    Left,
    Center,
    Right,
}

/// Table cells as inline spans; rows may be shorter than `aligns`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Table {
    pub aligns: Vec<Align>,
    pub header: Vec<Vec<Span>>,
    pub rows: Vec<Vec<Vec<Span>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Item {
    pub task: Option<bool>,
    pub blocks: Vec<Block>,
}

/// A block at the document's top level with its source byte range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TopBlock {
    pub block: Block,
    pub src: Range<usize>,
}

pub(crate) fn parse(md: &str, theme: &Theme) -> Vec<TopBlock> {
    let options =
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS | Options::ENABLE_TABLES;
    let mut builder = Builder::new(md, theme);
    for (event, range) in Parser::new_ext(md, options).into_offset_iter() {
        builder.event(event, range);
    }
    builder.finish()
}

enum Frame {
    Quote(Vec<Block>),
    List {
        start: Option<u64>,
        tight: bool,
        items: Vec<Item>,
        /// Source offset where each item starts.
        item_starts: Vec<usize>,
    },
    Item(Item),
}

enum InlineKind {
    Paragraph,
    Heading(u8),
}

struct Inline {
    kind: InlineKind,
    spans: Vec<Span>,
    styles: Vec<Style>,
    /// Alt text being collected while inside an image.
    image_alt: Option<String>,
    /// Images nested inside the alt text being collected.
    nested_images: usize,
    /// A soft break waiting for the next character, with its style; it
    /// becomes a space unless both neighbors are wide characters.
    pending_soft_break: Option<Style>,
    /// Inside an inline `<script>` or `<style>`, whose content is hidden.
    hidden: bool,
}

impl Inline {
    fn new(kind: InlineKind, base: Style) -> Self {
        Self {
            kind,
            spans: Vec::new(),
            styles: vec![base],
            image_alt: None,
            nested_images: 0,
            pending_soft_break: None,
            hidden: false,
        }
    }

    fn style(&self) -> Style {
        self.styles.last().cloned().unwrap_or_default()
    }

    fn push_style(&mut self, top: &Style) {
        let next = self.style().patch(top);
        self.styles.push(next);
    }

    fn pop_style(&mut self) {
        if self.styles.len() > 1 {
            self.styles.pop();
        }
    }

    fn text(&mut self, text: &str, style: Style) {
        if self.hidden {
            return;
        }
        let text = clean_inline(text);
        let Some(first) = text.chars().next() else {
            return;
        };
        self.resolve_soft_break(first);
        self.append(&text, style);
    }

    /// Turns a pending soft break into a space, except between two wide
    /// characters, where Chinese text joins without one.
    fn resolve_soft_break(&mut self, next: char) {
        let Some(style) = self.pending_soft_break.take() else {
            return;
        };
        let prev = match &self.image_alt {
            Some(alt) => alt.chars().last(),
            None => self.spans.last().and_then(|span| span.text.chars().last()),
        };
        if !(prev.is_some_and(is_wide) && is_wide(next)) {
            self.append(" ", style);
        }
    }

    fn append(&mut self, text: &str, style: Style) {
        if let Some(alt) = &mut self.image_alt {
            alt.push_str(text);
            return;
        }
        match self.spans.last_mut() {
            Some(last) if last.style == style && last.text != "\n" => last.text.push_str(text),
            _ => self.spans.push(Span::new(text, style)),
        }
    }

    /// Adds HTML text in the current style, breaks as hard line breaks and
    /// images as placeholders in `muted`.
    fn html_pieces(&mut self, pieces: Vec<Piece>, muted: &Style) {
        for piece in pieces {
            match piece {
                Piece::Text(text) => {
                    let style = self.style();
                    self.text(&text, style);
                }
                Piece::Break if !self.hidden => {
                    let style = self.style();
                    self.spans.push(Span::new("\n", style));
                }
                Piece::Break => {}
                Piece::Image(alt) => {
                    let style = self.style().patch(muted);
                    self.text(&image_label(alt.as_deref().unwrap_or("")), style);
                }
                Piece::Hide(start) => self.hidden = start,
            }
        }
    }

    fn into_block(self) -> Block {
        match self.kind {
            InlineKind::Paragraph => Block::Paragraph(self.spans),
            InlineKind::Heading(level) => Block::Heading {
                level,
                spans: self.spans,
            },
        }
    }
}

struct Builder<'t> {
    md: &'t str,
    theme: &'t Theme,
    top: Vec<TopBlock>,
    /// Source range of the top-level block being built.
    top_src: Range<usize>,
    frames: Vec<Frame>,
    inline: Option<Inline>,
    code: Option<(Option<String>, String)>,
    html: Option<String>,
    table: Option<Table>,
    /// Cells of the table row being built.
    row: Vec<Vec<Span>>,
}

impl<'t> Builder<'t> {
    fn new(md: &'t str, theme: &'t Theme) -> Self {
        Self {
            md,
            theme,
            top: Vec::new(),
            top_src: 0..0,
            frames: Vec::new(),
            inline: None,
            code: None,
            html: None,
            table: None,
            row: Vec::new(),
        }
    }

    fn finish(mut self) -> Vec<TopBlock> {
        self.close_inline();
        self.top
    }

    fn at_top_level(&self) -> bool {
        self.frames.is_empty()
            && self.inline.is_none()
            && self.code.is_none()
            && self.html.is_none()
            && self.table.is_none()
    }

    fn event(&mut self, event: Event, range: Range<usize>) {
        let starts_block =
            matches!(event, Event::Rule) || matches!(&event, Event::Start(tag) if is_block(tag));
        if starts_block && self.at_top_level() {
            self.top_src = range.clone();
        }
        match event {
            Event::Start(tag) => self.start(tag, range.start),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.text(&text),
            Event::Code(code) => {
                let style = self.inline_mut().style().patch(&self.theme.inline_code);
                self.inline_mut().text(&code, style);
            }
            Event::Html(html) | Event::InlineHtml(html) => self.html_text(&html),
            Event::SoftBreak => {
                let inline = self.inline_mut();
                inline.pending_soft_break = Some(inline.style());
            }
            Event::HardBreak => {
                let inline = self.inline_mut();
                if inline.image_alt.is_none() {
                    let style = inline.style();
                    inline.spans.push(Span::new("\n", style));
                }
            }
            Event::Rule => self.push_block(Block::Rule),
            Event::TaskListMarker(checked) => {
                if let Some(Frame::Item(item)) = self
                    .frames
                    .iter_mut()
                    .rev()
                    .find(|frame| matches!(frame, Frame::Item(_)))
                {
                    item.task = Some(checked);
                }
            }
            // Not enabled in the parser options.
            Event::FootnoteReference(_) | Event::InlineMath(_) | Event::DisplayMath(_) => {}
        }
    }

    fn start(&mut self, tag: Tag, offset: usize) {
        let theme = self.theme;
        if is_block(&tag) {
            self.close_inline();
        }
        match tag {
            Tag::Paragraph => {
                if let Some(Frame::List { tight, .. }) = self.frames.iter_mut().rev().nth(1) {
                    *tight = false;
                }
                self.inline = Some(Inline::new(InlineKind::Paragraph, self.theme.text.clone()));
            }
            Tag::Heading { level, .. } => {
                let level = heading_level(level);
                let base = match level {
                    1 => &self.theme.h1,
                    2 => &self.theme.h2,
                    _ => &self.theme.h3,
                };
                self.inline = Some(Inline::new(InlineKind::Heading(level), base.clone()));
            }
            Tag::BlockQuote(_) => self.frames.push(Frame::Quote(Vec::new())),
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => info
                        .split_whitespace()
                        .next()
                        .map(clean_inline)
                        .filter(|lang| !lang.is_empty()),
                    CodeBlockKind::Indented => None,
                };
                self.code = Some((lang, String::new()));
            }
            Tag::HtmlBlock => self.html = Some(String::new()),
            Tag::List(start) => self.frames.push(Frame::List {
                start,
                tight: true,
                items: Vec::new(),
                item_starts: Vec::new(),
            }),
            Tag::Item => {
                if let Some(Frame::List { item_starts, .. }) = self.frames.last_mut() {
                    item_starts.push(offset);
                }
                self.frames.push(Frame::Item(Item::default()));
            }
            Tag::Emphasis => self.inline_mut().push_style(&theme.italic),
            Tag::Strong => self.inline_mut().push_style(&theme.bold),
            Tag::Strikethrough => self.inline_mut().push_style(&theme.strike),
            Tag::Link { dest_url, .. } => {
                let style = Style {
                    link: Some(clean_inline(&dest_url)),
                    ..self.theme.link.clone()
                };
                self.inline_mut().push_style(&style);
            }
            Tag::Table(aligns) => {
                self.table = Some(Table {
                    aligns: aligns.into_iter().map(align).collect(),
                    ..Table::default()
                });
            }
            Tag::TableHead | Tag::TableRow => self.row.clear(),
            Tag::TableCell => {
                self.inline = Some(Inline::new(InlineKind::Paragraph, theme.text.clone()));
            }
            Tag::Image { .. } => {
                let inline = self.inline_mut();
                if inline.image_alt.is_some() {
                    inline.nested_images += 1;
                } else {
                    // The placeholder starts with a narrow `[`.
                    inline.resolve_soft_break('[');
                    inline.image_alt = Some(String::new());
                }
            }
            // Not enabled in the parser options.
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) => self.close_inline(),
            TagEnd::BlockQuote(_) => {
                self.close_inline();
                if let Some(Frame::Quote(blocks)) = self.frames.pop() {
                    self.push_block(Block::Quote(blocks));
                }
            }
            TagEnd::CodeBlock => {
                if let Some((lang, text)) = self.code.take() {
                    self.push_block(Block::Code { lang, text });
                }
            }
            TagEnd::HtmlBlock => {
                if let Some(raw) = self.html.take() {
                    self.html_block(&raw);
                }
            }
            TagEnd::List(_) => {
                self.close_inline();
                if let Some(Frame::List {
                    start,
                    tight,
                    items,
                    item_starts,
                }) = self.frames.pop()
                {
                    let tight = tight && !items_separated_by_blank_line(self.md, &item_starts);
                    self.push_block(Block::List {
                        start,
                        tight,
                        items,
                    });
                }
            }
            TagEnd::Item => {
                self.close_inline();
                if let Some(Frame::Item(item)) = self.frames.pop()
                    && let Some(Frame::List { items, .. }) = self.frames.last_mut()
                {
                    items.push(item);
                }
            }
            TagEnd::TableCell => {
                let spans = self.inline.take().map(|inline| inline.spans);
                self.row.push(spans.unwrap_or_default());
            }
            TagEnd::TableHead => {
                let cells = std::mem::take(&mut self.row);
                if let Some(table) = &mut self.table {
                    table.header = cells;
                }
            }
            TagEnd::TableRow => {
                let cells = std::mem::take(&mut self.row);
                if let Some(table) = &mut self.table {
                    table.rows.push(cells);
                }
            }
            TagEnd::Table => {
                if let Some(table) = self.table.take() {
                    self.push_block(Block::Table(table));
                }
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                self.inline_mut().pop_style();
            }
            TagEnd::Image => {
                let style = self.inline_mut().style().patch(&self.theme.muted);
                let inline = self.inline_mut();
                if inline.nested_images > 0 {
                    inline.nested_images -= 1;
                } else if let Some(alt) = inline.image_alt.take() {
                    inline.text(&image_label(&alt), style);
                }
            }
            _ => {}
        }
    }

    fn text(&mut self, text: &str) {
        if let Some((_, code)) = &mut self.code {
            code.push_str(text);
        } else if let Some(html) = &mut self.html {
            html.push_str(text);
        } else {
            let style = self.inline_mut().style();
            self.inline_mut().text(text, style);
        }
    }

    fn html_text(&mut self, html: &str) {
        if let Some(block) = &mut self.html {
            block.push_str(html);
        } else {
            let muted = self.theme.muted.clone();
            let inline = self.inline_mut();
            if inline.image_alt.is_none() {
                inline.html_pieces(html::inline(html), &muted);
            }
        }
    }

    /// An HTML block shown as its text, like a paragraph; nothing at all
    /// when only tags and comments remain.
    fn html_block(&mut self, raw: &str) {
        let pieces = html::block(raw);
        if pieces.is_empty() {
            return;
        }
        let mut inline = Inline::new(InlineKind::Paragraph, self.theme.text.clone());
        inline.html_pieces(pieces, &self.theme.muted);
        self.push_block(inline.into_block());
    }

    /// The open inline run, opening an implicit paragraph for the bare text
    /// of tight list items.
    fn inline_mut(&mut self) -> &mut Inline {
        let base = self.theme.text.clone();
        self.inline
            .get_or_insert_with(|| Inline::new(InlineKind::Paragraph, base))
    }

    fn close_inline(&mut self) {
        if let Some(inline) = self.inline.take() {
            self.push_block(inline.into_block());
        }
    }

    fn push_block(&mut self, block: Block) {
        let block = match block {
            Block::Code { lang, text } => Block::Code {
                lang,
                text: clean_block(&text),
            },
            other => other,
        };
        match self.frames.last_mut() {
            Some(Frame::Quote(blocks)) => blocks.push(block),
            Some(Frame::Item(item)) => item.blocks.push(block),
            // A list only holds items; anything else belongs to the document.
            Some(Frame::List { .. }) | None => self.top.push(TopBlock {
                block,
                src: self.top_src.clone(),
            }),
        }
    }
}

fn is_block(tag: &Tag) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::Item
            | Tag::Table(_)
    )
}

/// Whether a blank line comes right before any item after the first, which
/// makes the list loose. pulldown-cmark only signals looseness by wrapping
/// item text in paragraphs, which misses items holding quotes or headings.
fn items_separated_by_blank_line(md: &str, item_starts: &[usize]) -> bool {
    item_starts.windows(2).any(|pair| {
        let before_next = &md[pair[0]..pair[1]];
        // Drop the partial line holding the next item's marker, then look at
        // the last full line. Quote markers count as blank space.
        let Some(cut) = before_next.rfind('\n') else {
            return false;
        };
        let last_line = before_next[..cut].rsplit('\n').next().unwrap_or("");
        last_line
            .chars()
            .all(|c| matches!(c, ' ' | '\t' | '\r' | '>'))
    })
}

/// Placeholder text for an image with alt text `alt`.
fn image_label(alt: &str) -> String {
    if alt.is_empty() {
        "[图片]".to_string()
    } else {
        format!("[图片: {alt}]")
    }
}

fn align(alignment: Alignment) -> Align {
    match alignment {
        Alignment::None | Alignment::Left => Align::Left,
        Alignment::Center => Align::Center,
        Alignment::Right => Align::Right,
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Inline text with tabs as spaces and control characters removed, so
/// markdown content cannot inject terminal escape sequences.
fn clean_inline(text: &str) -> String {
    text.chars()
        .filter_map(|c| match c {
            '\t' | '\n' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

/// Preformatted text with tabs expanded and control characters other than
/// newlines removed.
fn clean_block(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut column = 0;
    for c in text.chars() {
        match c {
            '\n' => {
                out.push('\n');
                column = 0;
            }
            '\t' => {
                let spaces = TAB_WIDTH - column % TAB_WIDTH;
                out.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            c if c.is_control() => {}
            c => {
                out.push(c);
                column += 1;
            }
        }
    }
    out
}
