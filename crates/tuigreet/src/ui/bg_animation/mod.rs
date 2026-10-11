//! Background animations rendered behind the login UI.

pub mod conway;
pub mod doom;
pub mod matrix;

use std::str::FromStr;

use tui::{buffer::Buffer, layout::Rect, style::Color};

/// A background animation drawn beneath the login UI.
pub trait Animation: Send + Sync {
  /// React to the current terminal size.
  fn resize(&mut self, area: Rect);

  /// Advance the animation by one frame.
  fn step(&mut self);

  /// Paint the current frame.
  fn render(&self, area: Rect, buf: &mut Buffer);
}

/// Which animation to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  Doom,
  Matrix,
  Conway,
}

/// Catalog entry for a registered animation kind.
#[allow(dead_code)]
pub struct KindInfo {
  pub kind:  Kind,
  pub name:  &'static str,
  pub label: &'static str,
}

/// Every registered animation kind, in menu display order.
pub const KINDS: &[KindInfo] = &[
  KindInfo {
    kind:  Kind::Doom,
    name:  "doom",
    label: "DOOM Fire",
  },
  KindInfo {
    kind:  Kind::Matrix,
    name:  "matrix",
    label: "Matrix",
  },
  KindInfo {
    kind:  Kind::Conway,
    name:  "conway",
    label: "Conway",
  },
];

impl Kind {
  /// Resolve a config string to a [`Kind`], or `None` for unknown / disabled.
  pub fn from_name(name: &str) -> Option<Self> {
    match name.trim().to_ascii_lowercase().as_str() {
      "doom" | "fire" => Some(Self::Doom),
      "matrix" | "cmatrix" => Some(Self::Matrix),
      "conway"  => Some(Self::Conway),
      _ => None,
    }
  }
}

/// Fully-resolved configuration for one animation.
#[derive(Debug, Clone)]
pub enum AnimationSpec {
  Doom(doom::Options),
  Matrix(matrix::Options),
  Conway(conway::Options),
}

/// Construct an animation matching `spec`'s variant.
pub fn build(spec: &AnimationSpec) -> Box<dyn Animation> {
  match spec {
    AnimationSpec::Doom(opts) => Box::new(doom::Doom::new(opts.clone())),
    AnimationSpec::Matrix(opts) => Box::new(matrix::Matrix::new(opts.clone())),
    AnimationSpec::Conway(opts) => Box::new(conway::Conway::new(opts.clone())),
  }
}

impl Kind {
  /// Build a spec for this kind using its `Options::default()`.
  #[must_use]
  pub fn default_spec(self) -> AnimationSpec {
    match self {
      Self::Doom => AnimationSpec::Doom(doom::Options::default()),
      Self::Matrix => AnimationSpec::Matrix(matrix::Options::default()),
      Self::Conway => AnimationSpec::Conway(conway::Options::default()),
    }
  }
}

/// Build an animation of the given kind using its default options.
pub fn build_default(kind: Kind) -> Box<dyn Animation> {
  build(&kind.default_spec())
}

/// Parse a color from `#RRGGBB`, `0xRRGGBB`, or any string accepted by
/// ratatui's [`Color::from_str`].
pub fn parse_color(s: &str) -> Option<Color> {
  let trimmed = s.trim();
  let hex = trimmed
    .strip_prefix('#')
    .or_else(|| trimmed.strip_prefix("0x"))
    .or_else(|| trimmed.strip_prefix("0X"));
  if let Some(hex) = hex
    && hex.len() == 6
    && let Ok(r) = u8::from_str_radix(&hex[0..2], 16)
    && let Ok(g) = u8::from_str_radix(&hex[2..4], 16)
    && let Ok(b) = u8::from_str_radix(&hex[4..6], 16)
  {
    return Some(Color::Rgb(r, g, b));
  }
  Color::from_str(trimmed).ok()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_hex_colors() {
    assert_eq!(parse_color("#9F2707"), Some(Color::Rgb(0x9F, 0x27, 0x07)));
    assert_eq!(parse_color("0xFFFFFF"), Some(Color::Rgb(255, 255, 255)));
    assert_eq!(parse_color("0X000000"), Some(Color::Rgb(0, 0, 0)));
  }

  #[test]
  fn falls_back_to_named_colors() {
    assert!(parse_color("red").is_some());
  }

  #[test]
  fn rejects_garbage() {
    assert_eq!(parse_color("not-a-color"), None);
    assert_eq!(parse_color("#ZZZZZZ"), None);
  }

  #[test]
  fn kind_from_name() {
    assert_eq!(Kind::from_name("doom"), Some(Kind::Doom));
    assert_eq!(Kind::from_name("DOOM"), Some(Kind::Doom));
    assert_eq!(Kind::from_name("fire"), Some(Kind::Doom));
    assert_eq!(Kind::from_name("none"), None);
    assert_eq!(Kind::from_name(""), None);
    assert_eq!(Kind::from_name("matrix"), Some(Kind::Matrix));
    assert_eq!(Kind::from_name("CMATRIX"), Some(Kind::Matrix));
    assert_eq!(Kind::from_name("conway"), Some(Kind::Conway));
    assert_eq!(Kind::from_name("CONWAY"), Some(Kind::Conway));
  }
}
