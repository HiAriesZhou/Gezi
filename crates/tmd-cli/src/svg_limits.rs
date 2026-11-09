//! Validate SVG structure before entering recursive conversion/rendering.
use std::collections::HashMap;

use resvg::usvg::roxmltree::{Document, ParsingOptions};

const MAX_DEPTH: usize = 32;
const MAX_EXPANDED: usize = 20_000;

pub fn parse(text: &str) -> Result<Document<'_>, String> {
    // roxmltree's tokenizer is recursive too. Bound literal nesting with a
    // streaming tokenizer before constructing any XML tree.
    let mut depth = 0usize;
    for token in xmlparser::Tokenizer::from(text) {
        match token.map_err(|error| error.to_string())? {
            xmlparser::Token::ElementEnd {
                end: xmlparser::ElementEnd::Open,
                ..
            } => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err("SVG XML nesting exceeds limit".into());
                }
            }
            xmlparser::Token::ElementEnd {
                end: xmlparser::ElementEnd::Close(_, _),
                ..
            } => {
                depth = depth.saturating_sub(1);
            }
            xmlparser::Token::DtdStart { .. } | xmlparser::Token::EmptyDtd { .. } => {
                return Err("SVG DTD is unsupported".into());
            }
            _ => {}
        }
    }
    let doc = Document::parse_with_options(
        text,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: MAX_EXPANDED as u32,
        },
    )
    .map_err(|error| error.to_string())?;
    let nodes: Vec<_> = doc.descendants().collect();
    let mut edges = vec![Vec::new(); nodes.len()];
    let ids: HashMap<_, _> = nodes
        .iter()
        .filter_map(|node| node.attribute("id").map(|id| (id, node.id().get_usize())))
        .collect();
    for node in &nodes {
        let index = node.id().get_usize();
        edges[index].extend(node.children().map(|child| child.id().get_usize()));
        if node.is_element() && node.tag_name().name() == "style" {
            let css = node.text().unwrap_or_default().to_ascii_lowercase();
            // Stylesheet selectors can add reference edges to arbitrary nodes.
            // Keep these out of the bounded subset instead of guessing targets.
            if css.contains("url") || css.contains('\\') {
                return Err("SVG stylesheet references are unsupported".into());
            }
        }
        for attribute in node.attributes() {
            let value = attribute.value();
            if value.contains('\\') {
                return Err("SVG escaped references are unsupported".into());
            }
            // Conservatively account for href, paint servers, masks, clips,
            // filters and inline styles. XML character references are decoded
            // by roxmltree before this check.
            for fragment in value.split('#').skip(1) {
                let id = fragment
                    .split(|c: char| {
                        c.is_whitespace() || matches!(c, ')' | '\'' | '"' | ';' | ',' | '#')
                    })
                    .next()
                    .unwrap_or_default();
                if let Some(&target) = ids.get(id) {
                    edges[index].push(target);
                }
            }
        }
    }
    let mut active = vec![false; nodes.len()];
    let mut memo = vec![None; nodes.len()];
    measure(0, 0, &edges, &mut active, &mut memo)?;
    Ok(doc)
}

fn measure(
    node: usize,
    depth: usize,
    edges: &[Vec<usize>],
    active: &mut [bool],
    memo: &mut [Option<(usize, usize)>],
) -> Result<(usize, usize), String> {
    if depth > MAX_DEPTH || active[node] {
        return Err("SVG nesting or reference cycle exceeds limit".into());
    }
    if let Some((height, count)) = memo[node] {
        if depth + height > MAX_DEPTH {
            return Err("SVG expanded depth exceeds limit".into());
        }
        return Ok((height, count));
    }
    active[node] = true;
    let mut height = 0;
    let mut count = 1;
    for &child in &edges[node] {
        let (child_height, child_count) = measure(child, depth + 1, edges, active, memo)?;
        height = height.max(child_height + 1);
        count += child_count;
        if count > MAX_EXPANDED {
            return Err("SVG expanded structure exceeds limit".into());
        }
    }
    active[node] = false;
    memo[node] = Some((height, count));
    Ok((height, count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_literal_and_reference_expanded_depth() {
        let deep = format!(
            "<svg>{}<path/>{}</svg>",
            "<g>".repeat(400),
            "</g>".repeat(400)
        );
        assert!(parse(&deep).is_err());
        let mut uses = String::from("<svg>");
        for i in 0..40 {
            uses.push_str(&format!("<g id='g{i}'><use href='#g{}'/></g>", i + 1));
        }
        uses.push_str("<g id='g40'><path/></g></svg>");
        assert!(parse(&uses).is_err());
        assert!(
            parse("<svg><g id='a'><use href='#b'/></g><g id='b'><use href='#a'/></g></svg>")
                .is_err()
        );
    }

    #[test]
    fn supports_bounded_badge_paint_servers_and_use_but_rejects_hidden_edges() {
        assert!(parse("<svg><defs><linearGradient id='s'><stop/></linearGradient><path id='p'/></defs><rect fill='url(#s)'/><use href='#p'/></svg>").is_ok());
        assert!(parse("<svg><style>.a {fill:#fff}</style><path class='a'/></svg>").is_ok());
        assert!(parse("<svg><style>.a {fill:url(#p)}</style><path id='p'/></svg>").is_err());
        assert!(parse("<svg><path style='fill:u&#92;72l(#p)'/></svg>").is_err());
    }

    #[test]
    fn rejects_exponential_reference_expansion() {
        let mut svg = String::from("<svg><g id='g0'><path/></g>");
        for i in 1..16 {
            svg.push_str(&format!(
                "<g id='g{i}'><use href='#g{p}'/><use href='#g{p}'/></g>",
                p = i - 1
            ));
        }
        svg.push_str("</svg>");
        assert!(parse(&svg).is_err());
    }
}
