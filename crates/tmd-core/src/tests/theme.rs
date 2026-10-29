//! Theme files and built-in themes.

use crate::{Color, Theme};

#[test]
fn colors_parse_in_every_notation() {
    let theme = Theme::from_toml(
        r##"
[styles]
h1 = { fg = "ansi:4", bg = "bright:4" }
h2 = { fg = "#4DF2FF", bg = "default" }
"##,
    )
    .unwrap();
    assert_eq!(theme.h1.fg, Color::Ansi(4));
    assert_eq!(theme.h1.bg, Color::Ansi(12));
    assert_eq!(theme.h2.fg, Color::Rgb(0x4d, 0xf2, 0xff));
    assert_eq!(theme.h2.bg, Color::Reset);
}

#[test]
fn missing_fields_come_from_the_default_for_the_background() {
    let dark = Theme::from_toml("[styles]\nh1 = { fg = \"ansi:1\" }").unwrap();
    assert_eq!(dark.h1.fg, Color::Ansi(1));
    assert!(
        !dark.h1.bold,
        "a style given in the file replaces the base style"
    );
    assert_eq!(dark.h2, Theme::builtin("default-dark").unwrap().h2);

    let light = Theme::from_toml("dark = false").unwrap();
    assert_eq!(light, Theme::builtin("default-light").unwrap());
}

#[test]
fn errors_name_the_line() {
    let cases = [
        ("dark = true\n[styles]\nh7 = {}\n", 3, "h7"),
        ("[styles]\n\nh1 = { fg = \"blu\" }\n", 3, "invalid color"),
        ("[styles]\nh1 = { fg = \"ansi:8\" }\n", 2, "invalid color"),
        ("[styles]\nh1 = { fg = \"#12345\" }\n", 2, "invalid color"),
        (
            "[styles]\nh1 = { fg = \"ansi:1\", blink = true }\n",
            2,
            "blink",
        ),
        ("dark = \"yes\"\n", 1, ""),
        (
            "[symbols.table]\n\nvertical = \"││\"\n",
            3,
            "symbols.table.vertical",
        ),
        ("[symbols]\nbullets = []\n", 2, "symbols.bullets"),
    ];
    for (source, line, needle) in cases {
        let err = Theme::from_toml(source).expect_err(source);
        assert_eq!(err.line, Some(line), "{source:?}: {err}");
        assert!(err.to_string().contains(needle), "{source:?}: {err}");
    }
}

#[test]
fn built_in_themes_load() {
    let names: Vec<&str> = Theme::builtin_names().collect();
    assert!(names.contains(&"default-dark") && names.contains(&"default-light"));
    for name in names {
        assert!(Theme::builtin(name).is_some(), "{name}");
    }
    assert!(Theme::builtin("no-such-theme").is_none());
    assert_eq!(Theme::default(), Theme::builtin("default-dark").unwrap());
    assert!(!Theme::builtin("default-light").unwrap().dark);
}

#[test]
fn default_themes_use_only_the_16_ansi_colors() {
    for name in ["default-dark", "default-light"] {
        let t = Theme::builtin(name).unwrap();
        let styles = [
            &t.h1,
            &t.h2,
            &t.h3,
            &t.text,
            &t.bold,
            &t.italic,
            &t.strike,
            &t.inline_code,
            &t.code_block,
            &t.link,
            &t.quote_bar,
            &t.list_marker,
            &t.rule,
            &t.table_border,
            &t.table_header,
            &t.muted,
            &t.fold,
        ];
        for style in styles {
            for color in [style.fg, style.bg] {
                assert!(
                    matches!(color, Color::Reset | Color::Ansi(0..=15)),
                    "{name}: {color:?}"
                );
            }
        }
    }
}

#[test]
fn neon_tokyo_is_true_color() {
    let theme = Theme::builtin("neon-tokyo").expect("neon-tokyo is built in");
    assert!(theme.dark);
    assert!(matches!(theme.h1.fg, Color::Rgb(..)));
    assert!(matches!(theme.inline_code.fg, Color::Rgb(0x4d, 0xf2, 0xff)));
}

/// Chinese fonts often have no bold or italic face, so built-in themes
/// also color them.
#[test]
fn built_in_themes_color_bold_and_italic() {
    for name in Theme::builtin_names() {
        let theme = Theme::builtin(name).unwrap();
        assert!(theme.bold.bold && theme.bold.fg != Color::Reset, "{name}");
        assert!(
            theme.italic.italic && theme.italic.fg != Color::Reset,
            "{name}"
        );
        assert_ne!(theme.bold.fg, theme.italic.fg, "{name}");
    }
}

#[test]
fn heading_levels_are_distinguishable() {
    use crate::render;
    for name in Theme::builtin_names() {
        let theme = Theme::builtin(name).unwrap();
        let out = render("# one\n\n## two\n\n### three", 10, &theme);
        let rows: Vec<String> = out.iter().map(|l| l.plain_text()).collect();
        assert_eq!(rows[1], "━".repeat(10), "{name}: thick line under level 1");
        assert_eq!(rows[4], "─".repeat(10), "{name}: thin line under level 2");
        assert!(theme.h3.bold && theme.h3.fg != theme.h2.fg, "{name}");
    }
}

#[test]
fn control_characters_in_symbols_are_rejected() {
    let cases = [
        (
            "[symbols]\nbullets = [\"\\u001b]0;x\\u0007*\"]\n",
            2,
            "symbols.bullets",
        ),
        (
            "[symbols]\nquote_bar = \"\\u001b[2J|\"\n",
            2,
            "symbols.quote_bar",
        ),
        ("[symbols]\n\nrule = \"\\r\"\n", 3, "symbols.rule"),
        (
            "[symbols.table]\nvertical = \"\\u001b\"\n",
            2,
            "symbols.table.vertical",
        ),
        ("[symbols]\nh1_rule = \"a\\nb\"\n", 2, "symbols.h1_rule"),
    ];
    for (source, line, needle) in cases {
        let err = Theme::from_toml(source).expect_err(source);
        assert_eq!(err.line, Some(line), "{source:?}: {err}");
        assert!(err.to_string().contains(needle), "{source:?}: {err}");
    }
}

#[test]
fn symbol_errors_point_at_the_symbol_not_a_same_named_style() {
    let source = "[styles]\nrule = { fg = \"ansi:1\" }\n\n[symbols]\nrule = \"\"\n";
    let err = Theme::from_toml(source).unwrap_err();
    assert_eq!(err.line, Some(5), "{err}");
    let dotted = "dark = true\nsymbols.rule = \"\"\n";
    assert_eq!(Theme::from_toml(dotted).unwrap_err().line, Some(2));
}
