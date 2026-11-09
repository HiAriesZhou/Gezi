//! Block tree to rows of spans that fit a given width.

use crate::code;
use crate::line::Span;
use crate::parse::{Block, Item};
use crate::style::Style;
use crate::table;
use crate::theme::Theme;
use crate::width::{text_width, truncate};
use crate::wrap::{Row, push_text};

/// Columns a quote or list must leave for its content, enough for one wide
/// character; with less room the bar or marker is dropped instead.
const MIN_CONTENT: usize = 2;

pub(crate) struct Ctx<'t> {
    pub theme: &'t Theme,
    /// How many lists enclose the current block; picks the bullet.
    pub list_depth: usize,
    pub images: Option<&'t crate::ImageLayout<'t>>,
}

/// Lays out sibling blocks, with a blank row between them unless `tight`.
pub(crate) fn blocks(ctx: &Ctx, items: &[Block], width: usize, tight: bool) -> Vec<Row> {
    let mut rows = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 && !tight {
            rows.push(Row::new());
        }
        rows.extend(block(ctx, item, width));
    }
    rows
}

pub(crate) fn block(ctx: &Ctx, item: &Block, width: usize) -> Vec<Row> {
    let width = width.max(1);
    let theme = ctx.theme;
    match item {
        Block::Paragraph(spans) => crate::image::wrap(spans, width, ctx.images),
        Block::Heading { level, spans } => {
            let mut rows = crate::image::wrap(spans, width, ctx.images);
            let underline = match level {
                1 => Some((&theme.symbols.h1_rule, &theme.h1)),
                2 => Some((&theme.symbols.h2_rule, &theme.rule)),
                _ => None,
            };
            if let Some((symbol, style)) = underline.filter(|(symbol, _)| !symbol.is_empty()) {
                rows.push(rule_row(symbol, style, width));
            }
            rows
        }
        Block::Code(block) => code::rows(theme, block, width)
            .into_iter()
            .map(|(row, _)| row)
            .collect(),
        Block::List {
            start,
            tight,
            items,
        } => list(ctx, *start, *tight, items, width),
        Block::Quote(children) => quote(ctx, children, width),
        Block::Rule => vec![rule_row(&theme.symbols.rule, &theme.rule, width)],
        Block::Table(table) => table::layout(theme, table, width),
    }
}

/// `symbol` repeated across the row, at least once, cut to fit.
pub(crate) fn rule_row(symbol: &str, style: &Style, width: usize) -> Row {
    let repeated = symbol.repeat((width / text_width(symbol).max(1)).max(1));
    vec![Span::new(truncate(&repeated, width), style.clone())]
}

fn quote(ctx: &Ctx, children: &[Block], width: usize) -> Vec<Row> {
    let bar = &ctx.theme.symbols.quote_bar;
    let prefix = format!("{bar} ");
    if text_width(&prefix) + MIN_CONTENT > width {
        // No room left for content beside the bar; drop it so rows still fit.
        return blocks(ctx, children, width, false);
    }
    let inner = blocks(ctx, children, width - text_width(&prefix), false);
    inner
        .into_iter()
        .map(|row| {
            let mut out = Row::new();
            if row.is_empty() {
                push_text(&mut out, bar, &ctx.theme.quote_bar);
            } else {
                push_text(&mut out, &prefix, &ctx.theme.quote_bar);
                out.extend(row);
            }
            out
        })
        .collect()
}

fn list(ctx: &Ctx, start: Option<u64>, tight: bool, items: &[Item], width: usize) -> Vec<Row> {
    let theme = ctx.theme;
    let inner_ctx = Ctx {
        theme,
        list_depth: ctx.list_depth + 1,
        images: ctx.images,
    };
    let mut rows = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 && !tight {
            rows.push(Row::new());
        }
        let mut marker = marker(ctx, start.map(|n| n + i as u64), item.task);
        if text_width(&marker) + MIN_CONTENT > width {
            // No room left for content beside the marker; drop it so rows
            // still fit.
            marker.clear();
        }
        let indent = text_width(&marker);
        let mut inner = blocks(&inner_ctx, &item.blocks, width - indent, tight);
        if inner.is_empty() {
            inner.push(Row::new());
        }
        for (j, row) in inner.into_iter().enumerate() {
            let mut out = Row::new();
            if j == 0 {
                push_text(&mut out, &marker, &theme.list_marker);
            } else if !row.is_empty() {
                push_text(&mut out, &" ".repeat(indent), &theme.text);
            }
            out.extend(row);
            rows.push(out);
        }
    }
    rows
}

/// The marker before an item's first row, including its trailing space.
fn marker(ctx: &Ctx, number: Option<u64>, task: Option<bool>) -> String {
    let symbols = &ctx.theme.symbols;
    let task = task.map(|checked| {
        if checked {
            &symbols.task_checked
        } else {
            &symbols.task_unchecked
        }
    });
    match (number, task) {
        (Some(n), Some(task)) => format!("{n}. {task} "),
        (Some(n), None) => format!("{n}. "),
        (None, Some(task)) => format!("{task} "),
        (None, None) => {
            let bullets = &symbols.bullets;
            let bullet = bullets
                .get(ctx.list_depth % bullets.len().max(1))
                .map_or("-", String::as_str);
            format!("{bullet} ")
        }
    }
}
