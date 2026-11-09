//! Image references and cell geometry; independent of terminal protocols and IO.
use crate::wrap::{self, Row};
use crate::{Span, Style};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageSource {
    pub destination: String,
    pub alt: String,
    /// Author's pixel hints, before fitting within the document pane.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Index in Document::images(); assigned by the static document.
    pub index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageCell {
    pub index: usize,
    pub columns: u16,
    pub rows: u16,
    /// Row within the complete image, allowing a viewport to start inside it.
    pub row: u16,
    /// Exact bitmap size within the enclosing cells; padding stays transparent.
    pub pixel_width: u32,
    pub pixel_height: u32,
}

pub struct ImageLayout<'a> {
    pub cell_width: u16,
    pub cell_height: u16,
    /// Decoded intrinsic sizes. Missing/failed images retain their alt labels.
    pub sizes: &'a [Option<(u32, u32)>],
    pub failed: &'a [bool],
}

impl ImageLayout<'_> {
    fn rectangle(&self, image: &ImageSource, width: usize) -> Option<ImageCell> {
        let (w, h) = self.sizes.get(image.index).copied().flatten()?;
        if w == 0 || h == 0 {
            return None;
        }
        let cw = f64::from(self.cell_width.max(1));
        let ch = f64::from(self.cell_height.max(1));
        let mut scale = 1.0_f64;
        if let Some(value) = image.width {
            scale = scale.min(f64::from(value) / f64::from(w));
        }
        if let Some(value) = image.height {
            scale = scale.min(f64::from(value) / f64::from(h));
        }
        scale = scale.min(width as f64 * cw / f64::from(w));
        // Keep pathological aspect ratios bounded in layout as well as decoding.
        scale = scale.min(4096.0 * ch / f64::from(h));
        Some(ImageCell {
            index: image.index,
            columns: ((f64::from(w) * scale / cw).ceil() as u16)
                .max(1)
                .min(width as u16),
            rows: ((f64::from(h) * scale / ch).ceil() as u16).max(1),
            row: 0,
            pixel_width: (f64::from(w) * scale).round().max(1.0) as u32,
            pixel_height: (f64::from(h) * scale).round().max(1.0) as u32,
        })
    }
}

/// Images occupy atomic rectangles. Adjacent badges share a row; surrounding
/// prose wraps normally on its own rows. Container prefixes remain in layout.
pub(crate) fn wrap(spans: &[Span], width: usize, images: Option<&ImageLayout<'_>>) -> Vec<Row> {
    let Some(images) = images else {
        return wrap::wrap(spans, width);
    };
    let mut out = Vec::new();
    let mut text = Vec::new();
    let mut group: Vec<(Span, ImageCell)> = Vec::new();
    let mut used = 0;
    for span in spans {
        let rect = span
            .image
            .as_ref()
            .and_then(|image| images.rectangle(image, width));
        if let Some(rect) = rect {
            flush_text(&mut out, &mut text, width);
            if used + usize::from(rect.columns) + usize::from(!group.is_empty()) > width {
                flush_group(&mut out, &mut group);
                used = 0;
            }
            used += usize::from(rect.columns) + usize::from(!group.is_empty());
            group.push((span.clone(), rect));
        } else if !group.is_empty() && span.text.trim().is_empty() && span.text != "\n" {
            // HTML links often put whitespace between adjacent badge images.
        } else {
            flush_group(&mut out, &mut group);
            used = 0;
            let mut span = span.clone();
            if let Some(image) = &span.image
                && images.failed.get(image.index) == Some(&true)
            {
                span.text = format!("[图片: {} · 无法加载]", image.alt);
            }
            text.push(span);
        }
    }
    flush_group(&mut out, &mut group);
    flush_text(&mut out, &mut text, width);
    if out.is_empty() {
        out.push(Vec::new());
    }
    out
}

fn flush_text(out: &mut Vec<Row>, text: &mut Vec<Span>, width: usize) {
    if text.iter().any(|span| !span.text.trim().is_empty()) {
        out.extend(wrap::wrap(text, width));
    }
    text.clear();
}

fn flush_group(out: &mut Vec<Row>, group: &mut Vec<(Span, ImageCell)>) {
    let height = group.iter().map(|(_, cell)| cell.rows).max().unwrap_or(0);
    for row in 0..height {
        let mut spans = Vec::new();
        for (i, (original, cell)) in group.iter().enumerate() {
            if i > 0 {
                spans.push(Span::new(" ", Style::default()));
            }
            let mut span = Span::new(
                " ".repeat(usize::from(cell.columns)),
                original.style.clone(),
            );
            span.text_ranges = original.text_ranges.clone();
            if row < cell.rows {
                span.image_cell = Some(ImageCell { row, ..*cell });
            }
            spans.push(span);
        }
        out.push(spans);
    }
    group.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, RenderOptions, Theme};

    #[test]
    fn preserves_markdown_and_html_references_without_loading_anything() {
        let md = "![猫](assets/a.png)\n\n<picture><source srcset='wrong.svg'><img src='https://example.test/badge?v=2&amp;x=1' alt='CI' width='120' height='20'></picture>\n\n```\n![not an image](secret.png)\n```\n";
        let doc = Document::new(md, Theme::default());
        assert_eq!(doc.images().len(), 2);
        assert_eq!(doc.images()[0].destination, "assets/a.png");
        assert_eq!(
            doc.images()[1].destination,
            "https://example.test/badge?v=2&x=1"
        );
        assert_eq!(doc.images()[1].width, Some(120));
        assert_eq!(doc.images()[1].height, Some(20));
        assert_eq!(doc.images()[1].index, 1);
        let plain = doc.layout(80, RenderOptions::default());
        assert!(plain.iter().any(|l| l.plain_text().contains("[图片: 猫]")));
        assert!(
            plain
                .iter()
                .flat_map(|l| &l.spans)
                .all(|s| s.image_cell.is_none())
        );
        assert_eq!(doc.find("猫").len(), 1);
    }

    #[test]
    fn html_alt_controls_never_become_terminal_escapes() {
        let doc = Document::new("<img src='a.png' alt='safe&#27;[31m'>", Theme::default());
        let text: String = doc
            .layout(80, RenderOptions::default())
            .iter()
            .map(|line| line.plain_text())
            .collect();
        assert!(!text.contains('\x1b'));
        assert!(!doc.images()[0].alt.contains('\x1b'));
    }

    #[test]
    fn badges_share_rows_and_images_fit_nested_and_narrow_layouts() {
        let md = "<img src=a alt=A> <img src=b alt=B>\n\n> ![C](c)\n\n# after\n";
        let doc = Document::new(md, Theme::default());
        let sizes = [Some((40, 20)), Some((40, 20)), Some((200, 200))];
        let images = ImageLayout {
            cell_width: 10,
            cell_height: 20,
            sizes: &sizes,
            failed: &[],
        };
        let lines = doc.layout_with_images(12, RenderOptions::default(), &images);
        assert_eq!(
            lines[0]
                .spans
                .iter()
                .filter(|s| s.image_cell.is_some())
                .count(),
            2
        );
        for width in [1, 4, 12, 80] {
            let lines = doc.layout_with_images(width, RenderOptions::default(), &images);
            assert!(lines.iter().all(|line| line.width() <= usize::from(width)));
            for cell in lines
                .iter()
                .flat_map(|l| &l.spans)
                .filter_map(|s| s.image_cell)
            {
                assert!(cell.columns > 0 && cell.rows > 0 && cell.row < cell.rows);
            }
            assert!(Document::heading_line(&lines, &doc.headings()[0]).is_some());
        }
    }
}
