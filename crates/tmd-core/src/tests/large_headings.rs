use crate::{Line, LineSize, RenderOptions, Theme, render, render_with_options};

fn large(md: &str, width: u16) -> Vec<Line> {
    render_with_options(
        md,
        width,
        &Theme::default(),
        RenderOptions {
            large_headings: true,
        },
    )
}

#[test]
fn both_h1_syntaxes_wrap_at_half_width_and_keep_full_width_rules() {
    for md in ["# 中文标题 English", "中文标题 English\n==="] {
        let lines = large(md, 9);
        assert_eq!(lines[0].plain_text(), "中文");
        assert_eq!(lines[1].plain_text(), "标题");
        assert!(
            lines[..lines.len() - 1]
                .iter()
                .all(|line| line.size == LineSize::Double
                    && line.width() <= 9
                    && line.physical_height() == 2)
        );
        assert_eq!(lines.last().unwrap().size, LineSize::Normal);
        assert_eq!(lines.last().unwrap().width(), 9);
        assert!(lines.iter().all(|line| line.src == lines[0].src));
    }
}

#[test]
fn only_top_level_h1_changes() {
    let md = "## Two\n\n### Three\n\n> # Quote\n\n- # List\n\nbody\n\n```\n# code\n```\n\n| A | B |\n|---|---|\n| c | d |";
    assert_eq!(large(md, 40), render(md, 40, &Theme::default()));
}

#[test]
fn defaults_and_tiny_widths_preserve_normal_layout() {
    for width in 1..4 {
        assert_eq!(
            large("# 中文 heading", width),
            render("# 中文 heading", width, &Theme::default())
        );
    }
    let normal = render("# heading", 20, &Theme::default());
    assert!(
        normal
            .iter()
            .all(|line| line.size == LineSize::Normal && line.physical_height() == 1)
    );
    assert_eq!(
        normal,
        render_with_options("# heading", 20, &Theme::default(), RenderOptions::default())
    );
}

#[test]
fn wide_and_combining_clusters_fit_without_losing_text() {
    let md = "# 中e\u{301}👩‍💻文";
    let lines = large(md, 8);
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.size == LineSize::Double)
            .map(Line::plain_text)
            .collect::<String>(),
        "中e\u{301}👩‍💻文"
    );
    assert!(lines.iter().all(|line| line.width() <= 8));
    assert_eq!(lines[0].spans[0].style, Theme::default().h1);
}
