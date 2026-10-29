//! Code highlighting with syntect's built-in syntaxes and themes, loaded
//! the first time a code block needs them.
//!
//! Streaming re-renders the whole buffer for every frame, and highlighting
//! is most of a render's cost, so results are cached per code block with
//! the parser state after each line: a block that grew, or kept its first
//! lines, only highlights what is new.

use std::sync::{Mutex, OnceLock};

use syntect::highlighting::{FontStyle, HighlightIterator, HighlightState, Highlighter, ThemeSet};
use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};

use crate::style::{Color, Style};

/// Lines longer than this (in bytes) turn highlighting off for the block:
/// some syntaxes (Perl, Ruby) take quadratic time on one long line, and
/// input is untrusted. Hand-written code rarely comes close.
const MAX_LINE_BYTES: usize = 1000;

/// Highlighted lines kept across all blocks; the least recently used
/// blocks go first. Bounded by lines, not blocks: each render visits every
/// block in order, so fewer slots than blocks would evict each block just
/// before it is needed again.
const MAX_CACHED_LINES: usize = 20_000;

const DARK_THEME: &str = "base16-ocean.dark";
const LIGHT_THEME: &str = "InspiredGitHub";

/// Fence names syntect's default set lacks or spells differently. It has
/// no TypeScript, so TypeScript borrows JavaScript, which colors everything
/// but the type annotations correctly.
const ALIASES: &[(&str, &str)] = &[
    ("typescript", "js"),
    ("ts", "js"),
    ("tsx", "js"),
    ("mts", "js"),
    ("cts", "js"),
    ("jsx", "js"),
    ("sh", "bash"),
    ("shell", "bash"),
    ("zsh", "bash"),
    ("console", "bash"),
];

static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
static THEMES: OnceLock<ThemeSet> = OnceLock::new();
static CACHE: Mutex<Cache> = Mutex::new(Cache {
    entries: Vec::new(),
});

type Pieces = Vec<(String, Style)>;

/// Highlighted blocks, most recently used last.
#[derive(Default)]
struct Cache {
    entries: Vec<Entry>,
}

struct Entry {
    syntax: String,
    dark: bool,
    lines: Vec<Checkpoint>,
}

/// One highlighted line and the parser state after it.
#[derive(Clone)]
struct Checkpoint {
    line: String,
    pieces: Pieces,
    parse: ParseState,
    highlight: HighlightState,
}

/// Styled pieces for each line of `lines`, or `None` when the language is
/// unknown. Styles carry only foreground color and font attributes; the
/// caller lays them over the code block background.
pub(crate) fn highlight(lang: &str, lines: &[&str], dark: bool) -> Option<Vec<Pieces>> {
    // A poisoned lock only means a panic elsewhere; the cache is still valid.
    let mut cache = CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    highlight_with(&mut cache, lang, lines, dark)
}

fn highlight_with(
    cache: &mut Cache,
    lang: &str,
    lines: &[&str],
    dark: bool,
) -> Option<Vec<Pieces>> {
    if lines.iter().any(|line| line.len() > MAX_LINE_BYTES) {
        return None;
    }
    let syntaxes = SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines);
    let lang = lang.to_ascii_lowercase();
    let token = ALIASES
        .iter()
        .find(|(alias, _)| *alias == lang)
        .map_or(lang.as_str(), |(_, target)| target);
    let syntax = syntaxes
        .find_syntax_by_token(token)
        // Plain text has nothing to color; keep the terminal's foreground.
        .filter(|syntax| syntax.name != syntaxes.find_syntax_plain_text().name)?;
    let themes = THEMES.get_or_init(ThemeSet::load_defaults);
    let theme = themes
        .themes
        .get(if dark { DARK_THEME } else { LIGHT_THEME })?;
    let highlighter = Highlighter::new(theme);

    let mut entry = cache.take_longest_prefix(&syntax.name, dark, lines);
    let (mut parse, mut state) = match entry.lines.last() {
        Some(last) => (last.parse.clone(), last.highlight.clone()),
        None => (
            ParseState::new(syntax),
            HighlightState::new(&highlighter, ScopeStack::new()),
        ),
    };
    for line in &lines[entry.lines.len()..] {
        // The newline-aware syntaxes need each line to end with `\n`.
        let source = format!("{line}\n");
        let ops = parse.parse_line(&source, syntaxes).ok()?;
        let pieces = HighlightIterator::new(&mut state, &ops, &source, &highlighter)
            .map(|(style, text)| (text.trim_end_matches('\n').to_string(), convert(style)))
            .filter(|(text, _)| !text.is_empty())
            .collect();
        entry.lines.push(Checkpoint {
            line: (*line).to_string(),
            pieces,
            parse: parse.clone(),
            highlight: state.clone(),
        });
    }
    let out = entry.lines.iter().map(|line| line.pieces.clone()).collect();
    cache.store(entry);
    Some(out)
}

impl Cache {
    /// The cached lines that start `lines`, as an entry to extend. An entry
    /// that `lines` fully extends is taken out; a partial match is copied,
    /// so two blocks sharing their first lines keep separate entries.
    fn take_longest_prefix(&mut self, syntax: &str, dark: bool, lines: &[&str]) -> Entry {
        let best = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.syntax == syntax && entry.dark == dark)
            .map(|(i, entry)| {
                let shared = entry
                    .lines
                    .iter()
                    .zip(lines)
                    .take_while(|(cached, line)| cached.line == **line)
                    .count();
                (i, shared)
            })
            .max_by_key(|&(_, shared)| shared);
        match best {
            Some((i, shared)) if shared == self.entries[i].lines.len() => self.entries.remove(i),
            Some((i, shared)) if shared > 0 => Entry {
                syntax: syntax.to_string(),
                dark,
                lines: self.entries[i].lines[..shared].to_vec(),
            },
            _ => Entry {
                syntax: syntax.to_string(),
                dark,
                lines: Vec::new(),
            },
        }
    }

    fn store(&mut self, entry: Entry) {
        self.entries.push(entry);
        let mut lines: usize = self.entries.iter().map(|e| e.lines.len()).sum();
        while lines > MAX_CACHED_LINES && self.entries.len() > 1 {
            lines -= self.entries.remove(0).lines.len();
        }
    }
}

fn convert(style: syntect::highlighting::Style) -> Style {
    let fg = style.foreground;
    Style {
        fg: Color::Rgb(fg.r, fg.g, fg.b),
        bold: style.font_style.contains(FontStyle::BOLD),
        italic: style.font_style.contains(FontStyle::ITALIC),
        underline: style.font_style.contains(FontStyle::UNDERLINE),
        ..Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUST: [&str; 8] = [
        "/* a block comment",
        "   still comment */",
        "fn main() {",
        "    let s = \"text\";",
        "    println!(\"{s}\");",
        "}",
        "",
        "struct X;",
    ];

    fn fresh(lines: &[&str]) -> Option<Vec<Vec<(String, Style)>>> {
        highlight_with(&mut Cache::default(), "rust", lines, true)
    }

    #[test]
    fn growing_block_reuses_and_matches_a_fresh_highlight() {
        let mut cache = Cache::default();
        for end in 1..=RUST.len() {
            let lines = &RUST[..end];
            assert_eq!(
                highlight_with(&mut cache, "rust", lines, true),
                fresh(lines),
                "{end} lines"
            );
        }
        assert_eq!(cache.entries.len(), 1, "one block, one entry");
    }

    #[test]
    fn edited_lines_and_shared_prefixes_match_a_fresh_highlight() {
        let mut cache = Cache::default();
        let mut edited = RUST.to_vec();
        edited[1] = "   no longer the end";
        for lines in [&RUST[..], &edited[..], &RUST[..], &edited[..]] {
            assert_eq!(
                highlight_with(&mut cache, "rust", lines, true),
                fresh(lines)
            );
        }
        assert_eq!(
            cache.entries.len(),
            2,
            "two blocks sharing a prefix keep separate entries"
        );
    }

    /// Rendering visits every block in order each frame; with more blocks
    /// than a fixed slot count, least-recently-used eviction dropped each
    /// block just before it was needed again.
    #[test]
    fn many_small_blocks_stay_cached() {
        let mut cache = Cache::default();
        let blocks: Vec<String> = (0..200).map(|n| format!("let x{n} = {n};")).collect();
        for _ in 0..2 {
            for block in &blocks {
                highlight_with(&mut cache, "rust", &[block.as_str()], true);
            }
        }
        assert_eq!(cache.entries.len(), blocks.len());
    }

    #[test]
    fn cache_is_bounded() {
        let mut cache = Cache::default();
        for n in 0..(MAX_CACHED_LINES + 10) {
            let line = format!("let x{n} = {n};");
            highlight_with(&mut cache, "rust", &[line.as_str()], true);
        }
        let lines: usize = cache.entries.iter().map(|e| e.lines.len()).sum();
        assert_eq!(lines, MAX_CACHED_LINES);
    }
}
