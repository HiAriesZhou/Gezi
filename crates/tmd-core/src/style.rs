/// A terminal color, independent of any terminal library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    /// The terminal's default color.
    #[default]
    Reset,
    /// One of the 16 ANSI palette colors, `0..=15`.
    Ansi(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub dim: bool,
    /// Hyperlink target; backends that support it make the text clickable.
    pub link: Option<String>,
}

impl Style {
    pub fn fg(color: Color) -> Self {
        Self {
            fg: color,
            ..Self::default()
        }
    }

    pub fn bold(self) -> Self {
        Self { bold: true, ..self }
    }

    pub fn italic(self) -> Self {
        Self {
            italic: true,
            ..self
        }
    }

    pub fn underline(self) -> Self {
        Self {
            underline: true,
            ..self
        }
    }

    pub fn strike(self) -> Self {
        Self {
            strike: true,
            ..self
        }
    }

    pub fn on(self, bg: Color) -> Self {
        Self { bg, ..self }
    }

    /// Returns `self` with `top` layered over it: colors and link set in
    /// `top` win, attributes accumulate.
    pub fn patch(&self, top: &Style) -> Style {
        Style {
            fg: if top.fg == Color::Reset {
                self.fg
            } else {
                top.fg
            },
            bg: if top.bg == Color::Reset {
                self.bg
            } else {
                top.bg
            },
            bold: self.bold || top.bold,
            italic: self.italic || top.italic,
            underline: self.underline || top.underline,
            strike: self.strike || top.strike,
            dim: self.dim || top.dim,
            link: top.link.clone().or_else(|| self.link.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_layers_colors_and_accumulates_attributes() {
        let base = Style::fg(Color::Ansi(1)).bold().on(Color::Ansi(8));
        let top = Style::fg(Color::Ansi(4)).italic();
        let merged = base.patch(&top);
        assert_eq!(merged.fg, Color::Ansi(4));
        assert_eq!(merged.bg, Color::Ansi(8));
        assert!(merged.bold && merged.italic);
    }

    #[test]
    fn patch_keeps_link_unless_overridden() {
        let base = Style {
            link: Some("a".into()),
            ..Style::default()
        };
        assert_eq!(base.patch(&Style::default()).link.as_deref(), Some("a"));
        let top = Style {
            link: Some("b".into()),
            ..Style::default()
        };
        assert_eq!(base.patch(&top).link.as_deref(), Some("b"));
    }
}
