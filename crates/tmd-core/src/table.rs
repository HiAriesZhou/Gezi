//! Table layout: column widths, borders, alignment and narrowing.

use crate::line::Span;
use crate::parse::{Align, Table};
use crate::style::Style;
use crate::theme::Theme;
use crate::width::{text_width, truncate};
use crate::wrap::{Row, push_text, wrap};

/// Columns are not narrowed below this many cells.
const MIN_COLUMN: usize = 4;
const ELLIPSIS: &str = "…";

pub(crate) fn layout(theme: &Theme, table: &Table, width: usize) -> Vec<Row> {
    let columns = column_count(table);
    if columns == 0 {
        return Vec::new();
    }
    let widths = fit_widths(natural_widths(table, columns), width);

    let header: Vec<Vec<Span>> = table
        .header
        .iter()
        .map(|cell| {
            cell.iter()
                .map(|span| Span::new(span.text.clone(), span.style.patch(&theme.table_header)))
                .collect()
        })
        .collect();

    let symbols = &theme.symbols.table;
    let mut rows = vec![border(
        theme,
        &widths,
        &symbols.top_left,
        &symbols.top_mid,
        &symbols.top_right,
    )];
    rows.extend(content_rows(theme, &header, &table.aligns, &widths));
    if !table.rows.is_empty() {
        rows.push(border(
            theme,
            &widths,
            &symbols.mid_left,
            &symbols.mid_mid,
            &symbols.mid_right,
        ));
        for cells in &table.rows {
            rows.extend(content_rows(theme, cells, &table.aligns, &widths));
        }
    }
    rows.push(border(
        theme,
        &widths,
        &symbols.bottom_left,
        &symbols.bottom_mid,
        &symbols.bottom_right,
    ));

    if table_width(&widths) > width {
        rows.into_iter().map(|row| cut(row, width)).collect()
    } else {
        rows
    }
}

fn column_count(table: &Table) -> usize {
    table
        .rows
        .iter()
        .map(Vec::len)
        .chain([table.aligns.len(), table.header.len()])
        .max()
        .unwrap_or(0)
}

fn spans_width(spans: &[Span]) -> usize {
    spans.iter().map(Span::width).sum()
}

/// The widest cell in each column, at least 1.
fn natural_widths(table: &Table, columns: usize) -> Vec<usize> {
    let mut widths = vec![1; columns];
    for cells in std::iter::once(&table.header).chain(&table.rows) {
        for (width, cell) in widths.iter_mut().zip(cells) {
            *width = (*width).max(spans_width(cell));
        }
    }
    widths
}

/// Rendered width: a border before each column and after the last, plus
/// one space of padding on each side of every cell.
fn table_width(widths: &[usize]) -> usize {
    1 + widths.iter().map(|w| w + 3).sum::<usize>()
}

/// Narrows columns as if the widest were cut one cell at a time (leftmost
/// first on ties) until the table fits or every column is down to
/// [`MIN_COLUMN`], but computes the result directly: find the largest cap
/// that fits, then hand the leftover cells to the rightmost capped columns.
fn fit_widths(widths: Vec<usize>, width: usize) -> Vec<usize> {
    if table_width(&widths) <= width {
        return widths;
    }
    let budget = width.saturating_sub(table_width(&vec![0; widths.len()]));
    let capped = |cap: usize| -> usize { widths.iter().map(|&w| w.min(cap)).sum() };
    if capped(MIN_COLUMN) > budget {
        return widths.iter().map(|&w| w.min(MIN_COLUMN)).collect();
    }
    // Largest cap whose total fits; `capped` grows with `cap`.
    let (mut low, mut high) = (
        MIN_COLUMN,
        widths.iter().copied().max().unwrap_or(MIN_COLUMN),
    );
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if capped(mid) <= budget {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    let cap = low;
    let mut extra = budget - capped(cap);
    let mut result: Vec<usize> = widths.iter().map(|&w| w.min(cap)).collect();
    for (fitted, &natural) in result.iter_mut().zip(&widths).rev() {
        if extra == 0 {
            break;
        }
        if natural > cap {
            *fitted += 1;
            extra -= 1;
        }
    }
    result
}

fn border(theme: &Theme, widths: &[usize], left: &str, mid: &str, right: &str) -> Row {
    let horizontal = &theme.symbols.table.horizontal;
    let inner: Vec<String> = widths.iter().map(|w| horizontal.repeat(w + 2)).collect();
    let text = format!("{left}{}{right}", inner.join(mid));
    vec![Span::new(text, theme.table_border.clone())]
}

/// One table row, wrapping each cell into its column; taller cells make
/// the others pad with blank lines.
fn content_rows(
    theme: &Theme,
    cells: &[Vec<Span>],
    aligns: &[Align],
    widths: &[usize],
) -> Vec<Row> {
    let wrapped: Vec<Vec<Row>> = widths
        .iter()
        .enumerate()
        .map(|(i, &w)| {
            cells
                .get(i)
                .map_or_else(|| vec![Row::new()], |cell| wrap(cell, w))
        })
        .collect();
    let height = wrapped.iter().map(Vec::len).max().unwrap_or(1);
    let bar = &theme.symbols.table.vertical;

    (0..height)
        .map(|line| {
            let mut row = Row::new();
            for (i, &w) in widths.iter().enumerate() {
                let content = wrapped[i].get(line).cloned().unwrap_or_default();
                let align = aligns.get(i).copied().unwrap_or(Align::Left);
                push_text(&mut row, bar, &theme.table_border);
                push_text(&mut row, " ", &theme.text);
                aligned(&mut row, content, w, align, &theme.text);
                push_text(&mut row, " ", &theme.text);
            }
            push_text(&mut row, bar, &theme.table_border);
            row
        })
        .collect()
}

fn aligned(row: &mut Row, content: Row, width: usize, align: Align, pad_style: &Style) {
    let pad = width.saturating_sub(spans_width(&content));
    let (left, right) = match align {
        Align::Left => (0, pad),
        Align::Right => (pad, 0),
        Align::Center => (pad / 2, pad - pad / 2),
    };
    push_text(row, &" ".repeat(left), pad_style);
    for span in content {
        push_text(row, &span.text, &span.style);
    }
    push_text(row, &" ".repeat(right), pad_style);
}

/// Cuts a row to `width - 1` columns, pads any gap a wide character left,
/// and ends it with an ellipsis, so every cut row is exactly `width` wide.
fn cut(row: Row, width: usize) -> Row {
    let room = width.saturating_sub(text_width(ELLIPSIS));
    let mut out = Row::new();
    let mut used = 0;
    for span in row {
        let part = truncate(&span.text, room - used);
        push_text(&mut out, part, &span.style);
        used += text_width(part);
        if part.len() < span.text.len() {
            break;
        }
    }
    push_text(&mut out, &" ".repeat(room - used), &Style::default());
    push_text(&mut out, ELLIPSIS, &Style::default());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The original one-cell-at-a-time narrowing, kept as the oracle.
    fn fit_widths_stepwise(mut widths: Vec<usize>, width: usize) -> Vec<usize> {
        while table_width(&widths) > width {
            let (index, &widest) = widths
                .iter()
                .enumerate()
                .max_by_key(|&(index, w)| (*w, std::cmp::Reverse(index)))
                .unwrap();
            if widest <= MIN_COLUMN {
                break;
            }
            widths[index] -= 1;
        }
        widths
    }

    #[test]
    fn fit_widths_matches_stepwise_narrowing() {
        let mut state = 7u64;
        let mut next = |bound: u64| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            (state >> 33) % bound
        };
        for _ in 0..2000 {
            let columns = 1 + next(12) as usize;
            let widths: Vec<usize> = (0..columns).map(|_| 1 + next(30) as usize).collect();
            let width = 1 + next(200) as usize;
            assert_eq!(
                fit_widths(widths.clone(), width),
                fit_widths_stepwise(widths.clone(), width),
                "{widths:?} at {width}"
            );
        }
    }
}
