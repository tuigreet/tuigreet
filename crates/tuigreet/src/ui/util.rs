use std::num::TryFromIntError;

use ansi_to_tui::IntoText;
use tui::{
  prelude::Rect,
  text::Text,
  widgets::{Paragraph, Wrap},
};
use tuigreet_config::PromptAnchor;
use tuigreet_types::Mode;

use crate::Greeter;

pub fn titleize(message: &str) -> String {
  format!(" {message} ")
}

// Determines whether the cursor should be hidden. The cursor is visible only
// while the greeter expects text input.
pub const fn should_hide_cursor(greeter: &Greeter) -> bool {
  if greeter.is_idle() || greeter.working || greeter.done {
    return true;
  }
  match greeter.mode {
    Mode::Username => greeter.user_menu && greeter.username.value.is_empty(),
    Mode::Password => greeter.prompt.is_none(),
    Mode::Users
    | Mode::Sessions
    | Mode::Power
    | Mode::Processing
    | Mode::Action => true,
    Mode::Command | Mode::Background => false,
  }
}

// Computes the height of the main window where we display content, depending on
// the mode and spacing configuration.
//
// +------------------------+
// |                        | <- container padding
// |        Greeting        | <- greeting height
// |                        | <- auto-padding if greeting
// | Username:              | <- username
// | Password:              | <- password if prompt == Some(_)
// |                        | <- container padding
// +------------------------+
pub fn get_height(greeter: &Greeter) -> u16 {
  let (_, greeting_height) = get_greeting_height(greeter, 1, 0);
  let container_padding = greeter.container_padding();
  let prompt_padding = greeter.prompt_padding();

  let initial = match greeter.mode {
    Mode::Username | Mode::Action | Mode::Command => {
      container_padding.saturating_mul(2).saturating_add(1)
    },
    Mode::Password => {
      match greeter.prompt {
        Some(_) => {
          container_padding
            .saturating_mul(2)
            .saturating_add(prompt_padding)
            .saturating_add(2)
        },
        None => container_padding.saturating_mul(2).saturating_add(1),
      }
    },
    Mode::Users
    | Mode::Sessions
    | Mode::Power
    | Mode::Background
    | Mode::Processing => container_padding.saturating_mul(2),
  };

  match greeter.mode {
    Mode::Command
    | Mode::Sessions
    | Mode::Power
    | Mode::Background
    | Mode::Processing => initial,
    _ => initial.saturating_add(greeting_height),
  }
}

// Get the coordinates and size of the main window area, from the terminal size,
// and the content we need to display.
pub fn get_rect_bounds(
  greeter: &Greeter,
  area: Rect,
  items: usize,
) -> (u16, u16, u16, u16) {
  let prompt_layout = greeter
    .loaded_config
    .as_ref()
    .map(|config| &config.layout.prompt);
  let edge_margin = prompt_layout.map_or(0, |layout| layout.edge_margin);
  let margin_x = edge_margin.min(area.width.saturating_sub(1) / 2);
  let margin_y = edge_margin.min(area.height.saturating_sub(1) / 2);
  let usable = Rect::new(
    area.x.saturating_add(margin_x),
    area.y.saturating_add(margin_y),
    area.width.saturating_sub(margin_x.saturating_mul(2)),
    area.height.saturating_sub(margin_y.saturating_mul(2)),
  );

  let width = greeter.width().min(usable.width);
  let box_height = get_height(greeter)
    .saturating_add(u16::try_from(items).unwrap_or(u16::MAX))
    .min(usable.height);

  // Account for the message area rendered below the container so that the
  // combined block is positioned as one unit.
  let (_, message_height) =
    get_message_height(greeter, greeter.container_padding(), 0);
  let total_height =
    box_height.saturating_add(message_height).min(usable.height);
  let anchor =
    prompt_layout.map_or(PromptAnchor::Center, |layout| layout.anchor);

  let horizontal_space = usable.width.saturating_sub(width);
  let vertical_space = usable.height.saturating_sub(total_height);
  let anchored_x = match anchor {
    PromptAnchor::TopLeft
    | PromptAnchor::CenterLeft
    | PromptAnchor::BottomLeft => usable.x,
    PromptAnchor::TopCenter
    | PromptAnchor::Center
    | PromptAnchor::BottomCenter => {
      usable.x.saturating_add(horizontal_space / 2)
    },
    PromptAnchor::TopRight
    | PromptAnchor::CenterRight
    | PromptAnchor::BottomRight => usable.x.saturating_add(horizontal_space),
  };
  let anchored_y = match anchor {
    PromptAnchor::TopLeft
    | PromptAnchor::TopCenter
    | PromptAnchor::TopRight => usable.y,
    PromptAnchor::CenterLeft
    | PromptAnchor::Center
    | PromptAnchor::CenterRight => usable.y.saturating_add(vertical_space / 2),
    PromptAnchor::BottomLeft
    | PromptAnchor::BottomCenter
    | PromptAnchor::BottomRight => usable.y.saturating_add(vertical_space),
  };

  let offset_x = prompt_layout.map_or(0, |layout| layout.offset_x);
  let offset_y = prompt_layout.map_or(0, |layout| layout.offset_y);
  let max_x = usable.x.saturating_add(horizontal_space);
  let max_y = usable.y.saturating_add(vertical_space);
  let x = offset_coordinate(anchored_x, offset_x, usable.x, max_x);
  let y = offset_coordinate(anchored_y, offset_y, usable.y, max_y);

  (x, y, width, box_height)
}

fn offset_coordinate(base: u16, offset: i16, min: u16, max: u16) -> u16 {
  let shifted = i32::from(base) + i32::from(offset);
  u16::try_from(shifted.clamp(i32::from(min), i32::from(max))).unwrap_or(min)
}

// Computes the size of a text entry, from the container width and, if
// applicable, the prompt length.
pub fn get_input_width(
  greeter: &Greeter,
  width: u16,
  label: Option<&str>,
) -> u16 {
  let width = greeter.width().min(width);

  let label_width = label.map_or(0, |label| label.chars().count());

  width
    .saturating_sub(u16::try_from(label_width).unwrap_or(u16::MAX))
    .saturating_sub(5)
}

pub fn get_cursor_offset(
  greeter: &mut Greeter,
  length: usize,
) -> Result<usize, TryFromIntError> {
  let length = isize::try_from(length)?;
  let offset = length
    .saturating_add(greeter.cursor_offset)
    .clamp(0, length);
  greeter.cursor_offset = offset - length;
  usize::try_from(offset)
}

pub fn get_greeting_height(
  greeter: &Greeter,
  padding: u16,
  fallback: u16,
) -> (Option<Paragraph<'_>>, u16) {
  greeter.greeting.as_ref().map_or_else(
    || (None, fallback),
    |greeting| {
      let width = greeter.width();
      let text = greeting
        .clone()
        .into_text()
        .unwrap_or_else(|_| Text::raw(greeting));
      let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
      let height = paragraph
        .line_count(width.saturating_sub(padding.saturating_mul(2)).max(1))
        .saturating_add(1);

      // Layout dimensions saturate, just like message and container heights;
      // an oversized greeting must not wrap into a small visible rectangle.
      (Some(paragraph), u16::try_from(height).unwrap_or(u16::MAX))
    },
  )
}

pub fn get_message_height(
  greeter: &Greeter,
  padding: u16,
  fallback: u16,
) -> (Option<Paragraph<'_>>, u16) {
  greeter.message.as_ref().map_or_else(
    || (None, fallback),
    |message| {
      let width = greeter.width();
      let paragraph =
        Paragraph::new(message.trim_end()).wrap(Wrap { trim: true });
      let height = paragraph.line_count(width.saturating_sub(4).max(1));

      (
        Some(paragraph),
        u16::try_from(height)
          .unwrap_or(u16::MAX)
          .saturating_add(padding),
      )
    },
  )
}

#[cfg(test)]
mod test {
  use tui::{
    prelude::Rect,
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Paragraph, Wrap},
  };
  use tuigreet_config::Config;
  use tuigreet_types::Mode;

  use super::{get_input_width, get_rect_bounds};
  use crate::{
    Greeter,
    ui::util::{get_greeting_height, get_height},
  };

  // +-----------+
  // | Username: |
  // +-----------+
  #[test]
  fn test_container_height_username_padding_zero() {
    let mut greeter = Greeter::default();
    let mut config = Config::default();
    config.layout.container_padding = Some(0);
    greeter.apply_config(config);
    greeter.mode = Mode::Username;

    assert_eq!(get_height(&greeter), 3);
  }

  #[test]
  fn tiny_width_has_no_underflow() {
    let mut greeter = Greeter::default();
    greeter.apply_config(Config::default());
    assert_eq!(get_input_width(&greeter, 1, Some("Username")), 0);
    assert_eq!(
      get_rect_bounds(&greeter, Rect::new(0, 0, 1, 1), 0),
      (0, 0, 1, 1)
    );
  }

  // +-----------+
  // |           |
  // | Username: |
  // |           |
  // +-----------+
  #[test]
  fn test_container_height_username_padding_one() {
    let mut greeter = Greeter::default();
    let mut config = Config::default();
    config.layout.container_padding = Some(1);
    greeter.apply_config(config);
    greeter.mode = Mode::Username;

    assert_eq!(get_height(&greeter), 5);
  }

  // +-----------+
  // |           |
  // | Greeting  |
  // |           |
  // | Username: |
  // |           |
  // +-----------+
  #[test]
  fn test_container_height_username_greeting_padding_one() {
    let mut greeter = Greeter::default();
    let mut config = Config::default();
    config.layout.container_padding = Some(1);
    greeter.apply_config(config);
    greeter.greeting = Some("Hello".into());
    greeter.mode = Mode::Username;

    assert_eq!(get_height(&greeter), 7);
  }

  // +-----------+
  // |           |
  // | Greeting  |
  // |           |
  // | Username: |
  // |           |
  // | Password: |
  // |           |
  // +-----------+
  #[test]
  fn test_container_height_password_greeting_padding_one_prompt_padding_1() {
    let mut greeter = Greeter::default();
    let mut config = Config::default();
    config.layout.container_padding = Some(1);
    greeter.apply_config(config);
    greeter.greeting = Some("Hello".into());
    greeter.mode = Mode::Password;
    greeter.prompt = Some("Password:".into());

    assert_eq!(get_height(&greeter), 9);
  }

  // +-----------+
  // |           |
  // | Greeting  |
  // |           |
  // | Username: |
  // | Password: |
  // |           |
  // +-----------+
  #[test]
  fn test_container_height_password_greeting_padding_one_prompt_padding_0() {
    let mut greeter = Greeter::default();
    let mut config = Config::default();
    config.layout.container_padding = Some(1);
    config.layout.prompt_padding = Some(0);
    greeter.apply_config(config);
    greeter.greeting = Some("Hello".into());
    greeter.mode = Mode::Password;
    greeter.prompt = Some("Password:".into());

    assert_eq!(get_height(&greeter), 8);
  }

  #[test]
  fn test_rect_bounds() {
    let mut greeter = Greeter::default();
    let mut config = Config::default();
    config.layout.width = 50;
    greeter.apply_config(config);

    let (x, y, width, height) =
      get_rect_bounds(&greeter, Rect::new(0, 0, 100, 100), 1);

    assert_eq!(x, 25);
    assert_eq!(y, 47);
    assert_eq!(width, 50);
    assert_eq!(height, 6);
  }

  #[test]
  fn test_rect_bounds_with_message() {
    // With a message present, the combined block (box + message) should be
    // centered, not just the box. The box itself is unchanged; only `y` shifts
    // upward to leave room for the message below.
    //
    // Setup: default config (width=80, container_padding=2), Mode::Username,
    //        message="Wrong password" (fits on one line at width 76).
    //
    // box_height     = (2*2) + 1 = 5
    // message_height = line_count(76) + padding = 1 + 2 = 3
    // total_height   = 8
    // y = (100 - 8) / 2 = 46  (vs. (100 - 5) / 2 = 47 without the fix)
    let mut greeter = Greeter::default();
    greeter.apply_config(Config::default());
    greeter.message = Some("Wrong password".into());

    let (x, y, width, height) =
      get_rect_bounds(&greeter, Rect::new(0, 0, 100, 100), 0);

    assert_eq!(x, 10); // (100 - 80) / 2
    assert_eq!(y, 46); // (100 - 8) / 2, not (100 - 5) / 2 = 47
    assert_eq!(width, 80);
    assert_eq!(height, 5);
  }

  #[test]
  fn input_width() {
    let mut greeter = Greeter::default();
    let mut config = Config::default();
    config.layout.width = 40;
    config.layout.container_padding = Some(1);
    greeter.apply_config(config);

    let input_width = get_input_width(&greeter, 40, Some("Username:"));

    assert_eq!(input_width, 26);
  }

  #[test]
  fn greeting_height_one_line() {
    let mut greeter = Greeter::default();
    let mut config = Config::default();
    config.layout.width = 15;
    config.layout.container_padding = Some(1);
    greeter.apply_config(config);
    greeter.greeting = Some("Hello World".into());

    let (_, height) = get_greeting_height(&greeter, 1, 0);

    assert_eq!(height, 2);
  }

  #[test]
  fn greeting_height_two_lines() {
    let mut greeter = Greeter::default();
    let mut config = Config::default();
    config.layout.width = 8;
    config.layout.container_padding = Some(1);
    greeter.apply_config(config);
    greeter.greeting = Some("Hello World".into());

    let (_, height) = get_greeting_height(&greeter, 1, 0);

    assert_eq!(height, 3);
  }

  #[test]
  fn ansi_greeting_height_one_line() {
    let mut greeter = Greeter::default();
    let mut config = Config::default();
    config.layout.width = 15;
    config.layout.container_padding = Some(1);
    greeter.apply_config(config);
    greeter.greeting = Some("\x1b[31mHello\x1b[0m World".into());

    let (text, height) = get_greeting_height(&greeter, 1, 0);

    let expected = Paragraph::new(Text::from(vec![Line::from(vec![
      Span::styled("Hello", Style::default().fg(Color::Red)),
      Span::styled(" World", Style::reset()),
    ])]))
    .wrap(Wrap { trim: false });

    assert_eq!(text, Some(expected));
    assert_eq!(height, 2);
  }

  #[test]
  fn ansi_greeting_height_two_lines() {
    let mut greeter = Greeter::default();
    let mut config = Config::default();
    config.layout.width = 8;
    config.layout.container_padding = Some(1);
    greeter.apply_config(config);
    greeter.greeting = Some("\x1b[31mHello\x1b[0m World".into());

    let (text, height) = get_greeting_height(&greeter, 1, 0);

    let expected = Paragraph::new(Text::from(vec![Line::from(vec![
      Span::styled("Hello", Style::default().fg(Color::Red)),
      Span::styled(" World", Style::reset()),
    ])]))
    .wrap(Wrap { trim: false });

    assert_eq!(text, Some(expected));
    assert_eq!(height, 3);
  }

  #[test]
  fn greeting_preserves_trailing_whitespace() {
    let mut greeter = Greeter::default();
    let mut config = Config::default();
    config.layout.width = 30;
    config.layout.container_padding = Some(1);
    greeter.apply_config(config);
    // Simulate /etc/issue with trailing spaces for ASCII art alignment
    greeter.greeting = Some("Hello     \nWorld    ".into());

    let (text, height) = get_greeting_height(&greeter, 1, 0);

    let expected = Paragraph::new(Text::from(vec![
      Line::from("Hello     "),
      Line::from("World    "),
    ]))
    .wrap(Wrap { trim: false });

    assert_eq!(text, Some(expected));
    assert_eq!(height, 3);
  }
}
