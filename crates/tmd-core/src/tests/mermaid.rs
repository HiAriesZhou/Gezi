use crate::{Document, ImageLayout, RenderOptions, Theme};

#[test]
fn previews_preserve_code_search_copy_and_containers() {
    for source in [
        "```mermaid\nflowchart LR\nA[开始] --> B[结束]\n```",
        "> ```mermaid\n> flowchart LR\n> A[开始] --> B[结束]\n> ```",
        "- item\n\n  ```mermaid\n  flowchart LR\n  A[开始] --> B[结束]\n  ```",
    ] {
        let doc = Document::new(source, Theme::default());
        assert_eq!(doc.images().len(), 1);
        assert_eq!(doc.code_blocks()[0].preview, Some(0));
        assert!(
            doc.mermaid_source(0)
                .unwrap()
                .contains("A[开始] --> B[结束]")
        );
        let matches = doc.find("开始");
        assert_eq!(matches.len(), 1);
        let mut media = ImageLayout {
            source_previews: &[],
            cell_width: 8,
            cell_height: 16,
            sizes: &[Some((320, 160))],
            failed: &[],
        };
        let lines = doc.layout_with_images(40, RenderOptions::default(), &media);
        assert!(
            lines
                .iter()
                .flat_map(|line| &line.spans)
                .any(|span| span.image_cell.is_some())
        );
        assert!(Document::match_line(&lines, &matches[0]).is_some());
        assert!(lines.iter().all(|line| line.width() <= 40));
        media.source_previews = &[0];
        let lines = doc.layout_with_images(40, RenderOptions::default(), &media);
        assert!(
            lines
                .iter()
                .flat_map(|line| &line.spans)
                .all(|span| span.image_cell.is_none())
        );
        let row = Document::match_line(&lines, &matches[0]).unwrap();
        assert!(lines[row].plain_text().contains("开始"));
        media.source_previews = &[];
        media.sizes = &[None];
        media.failed = &[true];
        let lines = doc.layout_with_images(80, RenderOptions::default(), &media);
        assert!(
            lines
                .iter()
                .any(|line| line.plain_text().contains("preview unavailable"))
        );
        assert!(lines.iter().any(|line| line.plain_text().contains("开始")));
        assert!(
            doc.layout(80, RenderOptions::default())
                .iter()
                .any(|line| line.plain_text().contains("开始"))
        );
    }
}

#[test]
fn image_urls_cannot_become_diagrams() {
    let doc = Document::new(
        "![x](mermaid:flowchart)\n\n```rust\nflowchart LR\n```",
        Theme::default(),
    );
    assert!(doc.mermaid_source(0).is_none());
    assert_eq!(doc.code_blocks()[0].preview, None);
}
