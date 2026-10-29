//! Block tree to rows of spans that fit a given width.

use crate::line::Span;
use crate::parse::{Block, Item};
use crate::style::Style;
use crate::table;
use crate::theme::Theme;
use crate::width::{text_width, truncate};
use crate::wrap::{Row, hard_wrap_lines, push_text, wrap};

pub(crate) struct Ctx<'t> {
    pub theme: &'t Theme,
    /// How many lists enclose the current block; picks the bullet.
    pub list_depth: usize,
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
        Block::Paragraph(spans) => wrap(spans, width),
        Block::Heading { level, spans } => {
            let mut rows = wrap(spans, width);
            if *level == 2 {
                rows.push(rule_row(theme, width));
            }
            rows
        }
        Block::Code { lang, text } => code(theme, lang.as_deref(), text, width),
        Block::List {
            start,
            tight,
            items,
        } => list(ctx, *start, *tight, items, width),
        Block::Quote(children) => quote(ctx, children, width),
        Block::Rule => vec![rule_row(theme, width)],
        Block::Table(table) => table::layout(theme, table, width),
        Block::Html(text) => hard_wrap_lines(text, width)
            .into_iter()
            .map(|line| vec![Span::new(line, theme.muted.clone())])
            .collect(),
    }
}

fn rule_row(theme: &Theme, width: usize) -> Row {
    vec![Span::new(
        theme.symbols.rule.repeat(width),
        theme.rule.clone(),
    )]
}

fn code(theme: &Theme, lang: Option<&str>, text: &str, width: usize) -> Vec<Row> {
    let style = &theme.code_block;
    let mut rows = Vec::new();
    if let Some(lang) = lang {
        let label = truncate(lang, width);
        let pad = width - text_width(label);
        let label_style = Style {
            dim: true,
            ..style.clone()
        };
        let mut row = Row::new();
        push_text(&mut row, &" ".repeat(pad), style);
        push_text(&mut row, label, &label_style);
        rows.push(row);
    }
    let mut lines = hard_wrap_lines(text, width);
    if lines.is_empty() {
        lines.push(String::new());
    }
    for line in lines {
        let pad = width - text_width(&line);
        rows.push(vec![Span::new(line + &" ".repeat(pad), style.clone())]);
    }
    rows
}

fn quote(ctx: &Ctx, children: &[Block], width: usize) -> Vec<Row> {
    let bar = &ctx.theme.symbols.quote_bar;
    let prefix = format!("{bar} ");
    if text_width(&prefix) >= width {
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
    };
    let mut rows = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 && !tight {
            rows.push(Row::new());
        }
        let mut marker = marker(ctx, start.map(|n| n + i as u64), item.task);
        if text_width(&marker) >= width {
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
