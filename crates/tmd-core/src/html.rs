//! A small HTML tag scanner: keeps the text, turns `<br>` and block tags
//! into line breaks and `<img>` into image placeholders, hides everything
//! else. Not a full HTML parser; markdown files only carry simple HTML.

/// Tags whose start and end break the line inside an HTML block.
const BLOCK_TAGS: &[&str] = &[
    "p", "div", "details", "summary", "h1", "h2", "h3", "h4", "h5", "h6", "li", "tr",
];
/// Tags whose content is hidden along with the tags.
const HIDDEN_CONTENT_TAGS: &[&str] = &["script", "style"];
/// Line breaks kept between two pieces of text; more collapse into one
/// blank line.
const MAX_BREAKS: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Piece {
    Text(String),
    Break,
    /// An `<img>`, with its alt text when present and non-empty.
    Image(crate::ImageSource),
    /// Start (`true`) or end of a `<script>` or `<style>` whose content
    /// arrives as separate inline events and must stay hidden.
    Hide(bool),
}

/// Pieces of inline HTML: tags hidden, `<br>` as a break, `<img>` as an
/// image. Whitespace is kept as is.
pub(crate) fn inline(html: &str) -> Vec<Piece> {
    scan(html)
        .into_iter()
        .filter_map(|raw| match raw {
            Raw::Text(text) => Some(Piece::Text(text)),
            Raw::LineBreak => Some(Piece::Break),
            Raw::BlockBreak => None,
            Raw::Image(alt) => Some(Piece::Image(alt)),
            Raw::Hide(start) => Some(Piece::Hide(start)),
        })
        .collect()
}

/// Pieces of an HTML block laid out like a browser would, roughly:
/// whitespace runs become one space, block tags and `<br>` break lines,
/// at most one blank line in a row, none at the start or end.
pub(crate) fn block(html: &str) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    let mut breaks = 0;
    let mut line_breaks = 0;
    for raw in scan(html) {
        match raw {
            Raw::BlockBreak => breaks += 1,
            Raw::LineBreak => {
                breaks += 1;
                line_breaks += 1;
            }
            Raw::Text(text) => {
                let text = collapse_whitespace(&text);
                let text = if breaks > 0 || out.is_empty() {
                    text.trim_start().to_string()
                } else {
                    text
                };
                if text.is_empty() {
                    continue;
                }
                flush_breaks(&mut out, breaks, line_breaks);
                (breaks, line_breaks) = (0, 0);
                match out.last_mut() {
                    Some(Piece::Text(last)) => last.push_str(&text),
                    _ => out.push(Piece::Text(text)),
                }
            }
            Raw::Image(alt) => {
                flush_breaks(&mut out, breaks, line_breaks);
                (breaks, line_breaks) = (0, 0);
                out.push(Piece::Image(alt));
            }
            // A block holds the whole element, so its content is already
            // skipped.
            Raw::Hide(_) => {}
        }
    }
    trim_line_ends(out)
}

/// Emits the breaks collected between two visible pieces: one line break
/// for any run of block tags, one per `<br>`, capped at one blank line.
fn flush_breaks(out: &mut Vec<Piece>, breaks: usize, line_breaks: usize) {
    if out.is_empty() || breaks == 0 {
        return;
    }
    let count = line_breaks.clamp(1, MAX_BREAKS);
    out.extend(std::iter::repeat_n(Piece::Break, count));
}

/// Drops spaces left before a break or at the end.
fn trim_line_ends(pieces: Vec<Piece>) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::with_capacity(pieces.len());
    for (i, piece) in pieces.iter().enumerate() {
        let next_is_break = matches!(pieces.get(i + 1), Some(Piece::Break) | None);
        match piece {
            Piece::Text(text) if next_is_break => {
                let trimmed = text.trim_end();
                if !trimmed.is_empty() {
                    out.push(Piece::Text(trimmed.to_string()));
                }
            }
            other => out.push(other.clone()),
        }
    }
    out
}

fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(c);
            in_space = false;
        }
    }
    out
}

#[derive(Debug, PartialEq, Eq)]
enum Raw {
    Text(String),
    /// `<br>`.
    LineBreak,
    /// Start or end of a block tag.
    BlockBreak,
    Image(crate::ImageSource),
    /// A script or style element opens (`true`) without closing in this
    /// text, or closes (`false`) without having opened in it.
    Hide(bool),
}

fn scan(html: &str) -> Vec<Raw> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut rest = html;
    // Markup other than a comment needs a `>` after its `<`; past the last
    // one, every `<` is text. Checking this up front keeps scanning linear
    // when `<!` or `<?` repeat without ever closing.
    let last_gt = html.rfind('>');
    while let Some(lt) = rest.find('<') {
        text.push_str(&rest[..lt]);
        rest = &rest[lt..];
        let at = html.len() - rest.len();
        let can_close = last_gt.is_some_and(|gt| gt > at) || rest.starts_with("<!--");
        let found = if can_close { markup(rest) } else { None };
        match found {
            Some((consumed, item)) => {
                if !text.is_empty() {
                    out.push(Raw::Text(decode_entities(&std::mem::take(&mut text))));
                }
                rest = &rest[consumed..];
                match item {
                    Markup::Hidden => {}
                    Markup::Tag(tag) if HIDDEN_CONTENT_TAGS.contains(&tag.name.as_str()) => {
                        if tag.closing {
                            out.push(Raw::Hide(false));
                        } else if let Some(after) = after_closing(rest, &tag.name) {
                            rest = after;
                        } else {
                            out.push(Raw::Hide(true));
                            rest = "";
                        }
                    }
                    Markup::Tag(tag) => out.extend(tag_piece(&tag)),
                }
            }
            None => {
                // Not markup: a literal `<`.
                text.push('<');
                rest = &rest[1..];
            }
        }
    }
    text.push_str(rest);
    if !text.is_empty() {
        out.push(Raw::Text(decode_entities(&text)));
    }
    out
}

fn tag_piece(tag: &Tag) -> Option<Raw> {
    match tag.name.as_str() {
        "br" => Some(Raw::LineBreak),
        "img" if !tag.closing => Some(Raw::Image(crate::ImageSource {
            destination: attribute(&tag.attrs, "src")
                .map(|s| decode_entities(&s))
                .unwrap_or_default(),
            alt: attribute(&tag.attrs, "alt")
                .map(|s| decode_entities(&s))
                .unwrap_or_default(),
            width: attribute(&tag.attrs, "width")
                .and_then(|s| s.parse().ok())
                .filter(|&n| n > 0),
            height: attribute(&tag.attrs, "height")
                .and_then(|s| s.parse().ok())
                .filter(|&n| n > 0),
            index: 0,
        })),
        name if BLOCK_TAGS.contains(&name) => Some(Raw::BlockBreak),
        "td" | "th" if tag.closing => Some(Raw::Text(" ".to_string())),
        _ => None,
    }
}

enum Markup {
    /// A comment, declaration or processing instruction.
    Hidden,
    Tag(Tag),
}

struct Tag {
    name: String,
    closing: bool,
    /// Everything between the name and the closing `>`.
    attrs: String,
}

/// Recognizes markup at the start of `text` (which starts with `<`) and
/// returns how many bytes it spans.
fn markup(text: &str) -> Option<(usize, Markup)> {
    if let Some(body) = text.strip_prefix("<!--") {
        // An unclosed comment hides the rest, as HTML does.
        let end = body.find("-->").map_or(text.len(), |i| 4 + i + 3);
        return Some((end, Markup::Hidden));
    }
    if text.starts_with("<!") || text.starts_with("<?") {
        return text.find('>').map(|i| (i + 1, Markup::Hidden));
    }
    let (closing, after) = match text.strip_prefix("</") {
        Some(after) => (true, after),
        None => (false, &text[1..]),
    };
    if !after.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let name_len = after
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .unwrap_or(after.len());
    let end = tag_end(&after[name_len..])?;
    let consumed = text.len() - after.len() + name_len + end + 1;
    Some((
        consumed,
        Markup::Tag(Tag {
            name: after[..name_len].to_ascii_lowercase(),
            closing,
            attrs: after[name_len..name_len + end].to_string(),
        }),
    ))
}

/// Byte offset of the `>` closing a tag, skipping quoted attribute values.
fn tag_end(text: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in text.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            (None, '>') => return Some(i),
            (None, '<') => return None,
            _ => {}
        }
    }
    None
}

/// The rest of `text` after `</name>` (case-insensitive), or `None` when
/// the element does not close in `text`. Compares in place so many script
/// tags in one block do not copy the rest of it each time.
fn after_closing<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.match_indices("</").find_map(|(start, _)| {
        let after = &text[start + 2..];
        let matches = after
            .as_bytes()
            .get(..name.len())
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name.as_bytes()));
        if !matches {
            return None;
        }
        Some(after.find('>').map_or("", |gt| &after[gt + 1..]))
    })
}

/// The value of attribute `name` (case-insensitive) in a tag's attribute
/// text, quoted or not.
fn attribute(attrs: &str, name: &str) -> Option<String> {
    let mut rest = attrs;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '/');
        if rest.is_empty() {
            return None;
        }
        let key_len = rest
            .find(|c: char| c.is_whitespace() || c == '=' || c == '/')
            .unwrap_or(rest.len());
        let key = &rest[..key_len];
        rest = rest[key_len..].trim_start();
        let value = match rest.strip_prefix('=') {
            Some(after) => {
                let after = after.trim_start();
                let (value, remaining) = match after.chars().next() {
                    Some(q @ ('"' | '\'')) => {
                        let body = &after[1..];
                        let end = body.find(q).unwrap_or(body.len());
                        (&body[..end], body.get(end + 1..).unwrap_or(""))
                    }
                    _ => {
                        let end = after.find(char::is_whitespace).unwrap_or(after.len());
                        (&after[..end], &after[end..])
                    }
                };
                rest = remaining;
                Some(value)
            }
            None => None,
        };
        if key.eq_ignore_ascii_case(name) {
            return Some(value.unwrap_or("").to_string());
        }
    }
}

/// Decodes the entities markdown authors commonly write inside HTML.
fn decode_entities(text: &str) -> String {
    const ENTITIES: [(&str, &str); 7] = [
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&apos;", "'"),
        ("&nbsp;", " "),
    ];
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    'outer: while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        for (entity, value) in ENTITIES {
            if let Some(after) = rest.strip_prefix(entity) {
                out.push_str(value);
                rest = after;
                continue 'outer;
            }
        }
        out.push('&');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Piece {
        Piece::Text(s.to_string())
    }

    fn image(src: &str, alt: &str, width: Option<u32>) -> Piece {
        Piece::Image(crate::ImageSource {
            destination: src.into(),
            alt: alt.into(),
            width,
            height: None,
            index: 0,
        })
    }

    #[test]
    fn inline_hides_tags_and_keeps_breaks_and_images() {
        assert_eq!(inline("<kbd>"), []);
        assert_eq!(inline("<br/>"), [Piece::Break]);
        assert_eq!(
            inline(r#"<img src="a.png" alt="猫 &amp; 狗">"#),
            [image("a.png", "猫 & 狗", None)]
        );
        assert_eq!(inline("<img src=a.png>"), [image("a.png", "", None)]);
        assert_eq!(inline("<!-- hidden -->"), []);
    }

    #[test]
    fn block_breaks_on_block_tags_and_collapses_whitespace() {
        let html = "<details>\n<summary>更多</summary>\n内容  &amp;\n  细节\n</details>";
        assert_eq!(
            block(html),
            [text("更多"), Piece::Break, text("内容 & 细节")]
        );
    }

    #[test]
    fn block_readme_header() {
        let html = r#"<p align="center">
  <img src="logo.png" alt="TMD logo" width="120">
</p>
<p align="center">
  <a href="x"><img src="ci.svg" alt="CI"></a>
  <a href="y"><img src='lic.svg' alt='License: MIT'></a>
</p>"#;
        assert_eq!(
            block(html),
            [
                image("logo.png", "TMD logo", Some(120)),
                Piece::Break,
                image("ci.svg", "CI", None),
                text(" "),
                image("lic.svg", "License: MIT", None),
            ]
        );
    }

    #[test]
    fn br_runs_keep_at_most_one_blank_line() {
        assert_eq!(
            block("<div>a<br><br><br>b</div>"),
            [text("a"), Piece::Break, Piece::Break, text("b")]
        );
        assert_eq!(
            block("<div>a<br>b</div>"),
            [text("a"), Piece::Break, text("b")]
        );
    }

    #[test]
    fn comments_scripts_and_declarations_are_hidden() {
        assert_eq!(block("<!-- only a comment -->"), []);
        assert_eq!(block("<script>alert(1)</script>"), []);
        assert_eq!(block("<!DOCTYPE html><div>x</div>"), [text("x")]);
        assert_eq!(block("<div>a<!-- unclosed"), [text("a")]);
    }

    #[test]
    fn stray_angle_brackets_stay_as_text() {
        assert_eq!(block("<div>1 < 2 and a<b</div>"), [text("1 < 2 and a<b")]);
        assert_eq!(block("<div>x > y</div>"), [text("x > y")]);
    }

    #[test]
    fn entities_are_decoded() {
        assert_eq!(
            block("<p>&lt;tag&gt; &quot;q&quot; &#39;s&#39; a&nbsp;b &unknown;</p>"),
            [text("<tag> \"q\" 's' a b &unknown;")]
        );
    }

    #[test]
    fn quoted_greater_than_does_not_end_a_tag() {
        assert_eq!(block(r#"<p title="a > b">text</p>"#), [text("text")]);
    }

    #[test]
    fn table_cells_are_separated() {
        assert_eq!(
            block("<table><tr><td>a</td><td>b</td></tr><tr><th>c</th><th>d</th></tr></table>"),
            [text("a b"), Piece::Break, text("c d")]
        );
    }

    #[test]
    fn inline_script_and_style_mark_hidden_ranges() {
        assert_eq!(inline("<script>"), [Piece::Hide(true)]);
        assert_eq!(inline("</SCRIPT>"), [Piece::Hide(false)]);
        assert_eq!(inline("<style>p{}</style>"), []);
    }

    /// Unclosed `<!`, `<?` and many script tags used to rescan the rest of
    /// the block for every `<`, taking quadratic time.
    #[test]
    fn scanning_stays_linear_on_pathological_markup() {
        let cases = [
            format!("<div>{}", "<!".repeat(400_000)),
            format!("<div>{}", "<?".repeat(400_000)),
            format!("<div>{}", "<script></script>".repeat(50_000)),
        ];
        for html in &cases {
            let start = std::time::Instant::now();
            let _ = block(html);
            let elapsed = start.elapsed();
            assert!(
                elapsed < std::time::Duration::from_secs(1),
                "{}… took {elapsed:?}",
                &html[..12]
            );
        }
    }
}
