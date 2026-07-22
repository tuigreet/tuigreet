use std::sync::Arc;

use tokio::sync::RwLock;
use tui::{
  Terminal,
  buffer::Buffer,
  layout::{Constraint, Direction, Layout, Rect},
  style::Color,
};
use tuigreet_config::{Config, WidgetPosition};
use tuigreet_types::Mode;

use crate::{Greeter, integration::common::backend::TestBackend, ui};

/// Create a test greeter with default configuration
fn test_greeter() -> Arc<RwLock<Greeter>> {
  let mut greeter = Greeter::default();
  greeter.working = false;
  // Initialize config to avoid unwrap panics
  greeter.config = Greeter::options().parse(&[""]).ok();
  Arc::new(RwLock::new(greeter))
}

/// Render UI and return the buffer
async fn render_ui(
  greeter: Arc<RwLock<Greeter>>,
  width: u16,
  height: u16,
) -> Buffer {
  let (backend, buffer, _rx) = TestBackend::new(width, height);
  let mut terminal = Terminal::new(backend).unwrap();

  ui::draw(greeter.clone(), &mut terminal).await.unwrap();

  let locked_buffer = buffer.lock().unwrap();
  locked_buffer.clone()
}

/// Get text from a line in the buffer
fn get_line(buffer: &Buffer, y: u16, width: u16) -> String {
  (0..width).map(|x| buffer[(x, y)].symbol()).collect()
}

#[tokio::test]
async fn test_layout_basic_structure() {
  // Test that basic layout splits work correctly
  let area = Rect::new(0, 0, 80, 24);
  let constraints = vec![
    Constraint::Length(1), // window padding
    Constraint::Min(1),    // main content
    Constraint::Length(1), // status bar
    Constraint::Length(1), // window padding
  ];

  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints(constraints)
    .split(area);

  assert_eq!(chunks.len(), 4);
  assert_eq!(chunks[0].height, 1); // top padding
  assert_eq!(chunks[1].y, 1); // main starts after padding
  assert_eq!(chunks[2].y, chunks[1].y + chunks[1].height); // status after main
  assert_eq!(chunks[3].y, 23); // bottom padding at end
}

#[tokio::test]
async fn test_status_bar_bottom_default() {
  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.mode = Mode::Username;
  }

  let buffer = render_ui(greeter, 80, 24).await;

  // Status bar should be near bottom (accounting for padding)
  // Check for status bar indicators in the lower portion
  let mut found_status = false;
  for y in 20..24 {
    let line = get_line(&buffer, y, 80);
    if line.contains("ESC") || line.contains("action") {
      found_status = true;
      break;
    }
  }

  assert!(found_status, "Status bar should be rendered at bottom");
}

#[tokio::test]
async fn test_status_bar_top_position() {
  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.mode = Mode::Username;
    let mut config = Config::default();
    config.layout.widgets.status_position = WidgetPosition::Top;

    // NOTE: loaded_config is what the UI actually reads
    g.loaded_config = Some(config.clone());
    g.apply_config(&config);
  }

  let buffer = render_ui(greeter, 80, 24).await;

  // Status bar has distinctive patterns: ESC, F2, F3, F12
  // When at top position, it should be at row 0 (absolute top, before padding)
  // When at bottom, it's near the bottom (row 22 typically, before bottom
  // padding)
  let top_line = get_line(&buffer, 0, 80);
  let bottom_line = get_line(&buffer, 22, 80);

  let top_has_status = (top_line.contains("F2")
    || top_line.contains("F3")
    || top_line.contains("F12"))
    && top_line.contains("ESC");
  let bottom_has_status = (bottom_line.contains("F2")
    || bottom_line.contains("F3")
    || bottom_line.contains("F12"))
    && bottom_line.contains("ESC");

  // When configured for top, status bar should be at row 0, not at bottom
  assert!(
    top_has_status && !bottom_has_status,
    "Status bar should be at row 0 when configured for WidgetPosition::Top. \
     Found at top (row 0): {top_has_status}, Found at bottom (row 22): \
     {bottom_has_status}"
  );
}

#[tokio::test]
async fn test_status_bar_hidden() {
  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.mode = Mode::Username;
    let mut config = Config::default();
    config.layout.widgets.status_position = WidgetPosition::Hidden;
    g.loaded_config = Some(config.clone());
    g.apply_config(&config);
  }

  let buffer = render_ui(greeter, 80, 24).await;

  // Status bar should not be rendered anywhere
  for y in 0..24 {
    let line = get_line(&buffer, y, 80);
    // ESC and action buttons should not appear together (status bar pattern)
    let has_esc = line.contains("ESC");
    let has_action = line.contains("action");
    assert!(
      !(has_esc && has_action),
      "Status bar should be hidden at line {y}: {line}"
    );
  }
}

#[tokio::test]
async fn test_time_widget_top_default() {
  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.time = true;
    g.mode = Mode::Username;
  }

  let buffer = render_ui(greeter, 80, 24).await;

  // Time should be at top - search first few rows
  let mut found_time = false;
  for y in 0..6 {
    let line = get_line(&buffer, y, 80);
    // Time widget should contain digits and colons (HH:MM pattern)
    if line.chars().filter(char::is_ascii_digit).count() >= 2
      && line.contains(':')
    {
      found_time = true;
      break;
    }
  }

  assert!(found_time, "Time widget should be rendered at top");
}

#[tokio::test]
async fn test_time_widget_bottom_position() {
  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.time = true;
    g.mode = Mode::Username;
    let mut config = Config::default();
    config.layout.widgets.time_position = WidgetPosition::Bottom;
    g.loaded_config = Some(config.clone());
    g.apply_config(&config);
  }

  let buffer = render_ui(greeter, 80, 24).await;

  // Time should be near bottom
  let mut found_time = false;
  for y in 20..24 {
    let line = get_line(&buffer, y, 80);
    if line.chars().any(|c| c.is_ascii_digit() || c == ':') {
      found_time = true;
      break;
    }
  }

  assert!(found_time, "Time widget should be at bottom");
}

#[tokio::test]
async fn test_username_prompt_renders() {
  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.mode = Mode::Username;
  }

  let buffer = render_ui(greeter, 80, 24).await;

  // Find "Username" prompt in buffer
  let mut found_username = false;
  for y in 0..24 {
    let line = get_line(&buffer, y, 80);
    if line.to_lowercase().contains("username") {
      found_username = true;
      break;
    }
  }

  assert!(found_username, "Username prompt should be rendered");
}

#[tokio::test]
async fn test_password_prompt_renders() {
  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.mode = Mode::Password;
    g.username.value = "testuser".to_string();
    g.prompt = Some("Password".to_string());
  }

  let buffer = render_ui(greeter, 80, 24).await;

  // Find password prompt
  let mut found_password = false;
  for y in 0..24 {
    let line = get_line(&buffer, y, 80);
    if line.to_lowercase().contains("password") {
      found_password = true;
      break;
    }
  }

  assert!(found_password, "Password prompt should be rendered");
}

#[tokio::test]
async fn test_greeting_renders() {
  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.mode = Mode::Username;
    g.greeting = Some("Welcome to Test System!".to_string());
  }

  let buffer = render_ui(greeter, 80, 24).await;

  // Find greeting text
  let mut found_greeting = false;
  for y in 0..24 {
    let line = get_line(&buffer, y, 80);
    if line.contains("Welcome") || line.contains("Test System") {
      found_greeting = true;
      break;
    }
  }

  assert!(found_greeting, "Greeting should be rendered");
}

#[tokio::test]
async fn test_container_renders() {
  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.mode = Mode::Username;
  }

  let buffer = render_ui(greeter, 80, 24).await;

  // Check for border characters across the entire buffer
  let mut has_borders = false;
  for y in 0..24 {
    for x in 0..80 {
      let char = buffer[(x, y)].symbol();
      if char == "─"
        || char == "│"
        || char == "┌"
        || char == "┐"
        || char == "└"
        || char == "┘"
      {
        has_borders = true;
        break;
      }
    }
    if has_borders {
      break;
    }
  }

  assert!(
    has_borders,
    "Container borders should be rendered somewhere in the buffer"
  );
}

#[tokio::test]
async fn test_window_padding() {
  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.mode = Mode::Username;
    let mut config = Config::default();
    config.layout.window_padding = Some(2);
    g.loaded_config = Some(config.clone());
    g.apply_config(&config);
  }

  let buffer = render_ui(greeter, 80, 24).await;

  // First two rows should be padding (empty or minimal content)
  let first_row = get_line(&buffer, 0, 80);
  let second_row = get_line(&buffer, 1, 80);

  assert!(
    first_row.chars().all(|c| c == ' '),
    "First row should be padding (all spaces)"
  );
  assert!(
    second_row.chars().all(|c| c == ' '),
    "Second row should be padding (all spaces)"
  );
}

#[tokio::test]
async fn test_multiple_layout_sizes() {
  // Test that layout works correctly at different terminal sizes
  let sizes = vec![(80, 24), (120, 40), (40, 12)];

  for (width, height) in sizes {
    let greeter = test_greeter();
    {
      let mut g = greeter.write().await;
      g.mode = Mode::Username;
    }

    let buffer = render_ui(greeter, width, height).await;

    // Basic sanity checks
    assert_eq!(buffer.area().width, width);
    assert_eq!(buffer.area().height, height);

    // Should still render username prompt
    let mut found_content = false;
    for y in 0..height {
      let line = get_line(&buffer, y, width);
      if !line.trim().is_empty() {
        found_content = true;
        break;
      }
    }

    assert!(
      found_content,
      "Should render content at size {width}x{height}"
    );
  }
}

#[tokio::test]
async fn test_combined_time_and_status_positions() {
  // Test all combinations of time and status positions
  let positions = vec![
    WidgetPosition::Top,
    WidgetPosition::Bottom,
    WidgetPosition::Default,
    WidgetPosition::Hidden,
  ];

  for time_pos in &positions {
    for status_pos in &positions {
      let greeter = test_greeter();
      {
        let mut g = greeter.write().await;
        g.mode = Mode::Username;
        g.time = true;
        let mut config = Config::default();
        config.layout.widgets.time_position = time_pos.clone();
        config.layout.widgets.status_position = status_pos.clone();
        g.loaded_config = Some(config.clone());
        g.apply_config(&config);
      }

      let buffer = render_ui(greeter, 80, 24).await;

      // Just verify it renders without panicking
      assert_eq!(buffer.area().width, 80);
      assert_eq!(buffer.area().height, 24);
    }
  }
}

#[tokio::test]
async fn test_theme_border_color_applied() {
  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.mode = Mode::Username;

    let mut config = Config::default();
    config.theme.border = Some("red".to_string());

    g.loaded_config = Some(config.clone());
    g.apply_config(&config);
    g.apply_theme_config(&config.theme, None);
  }

  let buffer = render_ui(greeter, 80, 24).await;

  // Check that border characters have red color
  let mut found_red_border = false;
  for y in 0..24 {
    for x in 0..80 {
      let cell = &buffer[(x, y)];
      let char = cell.symbol();
      if (char == "─"
        || char == "│"
        || char == "┌"
        || char == "┐"
        || char == "└"
        || char == "┘")
        && cell.fg == Color::Red
      {
        found_red_border = true;
        break;
      }
    }
    if found_red_border {
      break;
    }
  }

  assert!(
    found_red_border,
    "Border should be rendered with red color from theme config"
  );
}

#[tokio::test]
async fn test_theme_prompt_color_applied() {
  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.mode = Mode::Username;

    let mut config = Config::default();
    config.theme.prompt = Some("yellow".to_string());

    g.loaded_config = Some(config.clone());
    g.apply_config(&config);
    g.apply_theme_config(&config.theme, None);
  }

  let buffer = render_ui(greeter, 80, 24).await;

  // Look for yellow prompt text
  let mut found_yellow = false;
  for y in 0..24 {
    for x in 0..80 {
      let cell = &buffer[(x, y)];
      if cell.fg == Color::Yellow && !cell.symbol().trim().is_empty() {
        found_yellow = true;
        break;
      }
    }
    if found_yellow {
      break;
    }
  }

  assert!(
    found_yellow,
    "Prompt should be rendered with yellow color from theme config"
  );
}

#[tokio::test]
async fn test_theme_container_background_applied() {
  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.mode = Mode::Username;

    let mut config = Config::default();
    config.theme.container = Some("blue".to_string());

    g.loaded_config = Some(config.clone());
    g.apply_config(&config);
    g.apply_theme_config(&config.theme, None);
  }

  let buffer = render_ui(greeter, 80, 24).await;

  // Look for blue background in container area
  let mut found_blue_bg = false;
  for y in 0..24 {
    for x in 0..80 {
      let cell = &buffer[(x, y)];
      if cell.bg == Color::Blue {
        found_blue_bg = true;
        break;
      }
    }
    if found_blue_bg {
      break;
    }
  }

  assert!(
    found_blue_bg,
    "Container should be rendered with blue background from theme config"
  );
}

#[tokio::test]
async fn test_theme_greeting_color_applied() {
  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.mode = Mode::Username;

    let mut config = Config::default();
    config.display.greeting = Some("Welcome to the system!".to_string());
    config.theme.greet = Some("green".to_string());

    g.loaded_config = Some(config.clone());
    g.apply_config(&config);
    g.apply_theme_config(&config.theme, None);
  }

  let buffer = render_ui(greeter, 80, 24).await;

  // Look for green greeting text
  let mut found_green_greeting = false;
  for y in 0..24 {
    let line = get_line(&buffer, y, 80);
    if line.contains("Welcome") || line.contains("system") {
      // Check if any cell in this line has green foreground
      for x in 0..80 {
        if buffer[(x, y)].fg == Color::Green {
          found_green_greeting = true;
          break;
        }
      }
    }
    if found_green_greeting {
      break;
    }
  }

  assert!(
    found_green_greeting,
    "Greeting should be rendered with green color from theme config"
  );
}

#[tokio::test]
async fn test_theme_hex_colors_applied() {
  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.mode = Mode::Username;

    let mut config = Config::default();
    config.theme.border = Some("#ff0000".to_string()); // Red in hex

    g.loaded_config = Some(config.clone());
    g.apply_config(&config);
    g.apply_theme_config(&config.theme, None);
  }

  let buffer = render_ui(greeter, 80, 24).await;

  // Look for RGB(255,0,0) border
  let mut found_rgb_border = false;
  for y in 0..24 {
    for x in 0..80 {
      let cell = &buffer[(x, y)];
      let char = cell.symbol();
      if (char == "─"
        || char == "│"
        || char == "┌"
        || char == "┐"
        || char == "└"
        || char == "┘")
        && cell.fg == Color::Rgb(255, 0, 0)
      {
        found_rgb_border = true;
        break;
      }
    }
    if found_rgb_border {
      break;
    }
  }

  assert!(
    found_rgb_border,
    "Border should be rendered with hex color (#ff0000) from theme config"
  );
}

#[tokio::test]
async fn test_theme_multiple_colors_applied() {
  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.mode = Mode::Username;

    let mut config = Config::default();
    config.display.greeting = Some("Test".to_string());
    config.theme.border = Some("red".to_string());
    config.theme.text = Some("cyan".to_string());
    config.theme.greet = Some("green".to_string());

    g.loaded_config = Some(config.clone());
    g.apply_config(&config);
    g.apply_theme_config(&config.theme, None);
  }

  let buffer = render_ui(greeter, 80, 24).await;

  let mut found_red = false;
  let mut found_cyan = false;
  let mut found_green = false;

  for y in 0..24 {
    for x in 0..80 {
      let cell = &buffer[(x, y)];
      if cell.fg == Color::Red {
        found_red = true;
      }
      if cell.fg == Color::Cyan {
        found_cyan = true;
      }
      if cell.fg == Color::Green {
        found_green = true;
      }
    }
  }

  assert!(found_red, "Should have red elements from border theme");
  assert!(
    found_cyan || found_green,
    "Should have cyan or green elements from text/greet theme"
  );
}

#[tokio::test]
async fn test_cli_theme_overrides_config_theme() {
  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.mode = Mode::Username;

    let mut config = Config::default();
    config.theme.border = Some("red".to_string());

    // CLI theme overrides with blue
    g.loaded_config = Some(config.clone());
    g.apply_config(&config);
    g.apply_theme_config(&config.theme, Some("border=blue"));
  }

  let buffer = render_ui(greeter, 80, 24).await;

  // Should find blue border (CLI override), not red
  let mut found_blue_border = false;
  let mut found_red_border = false;

  for y in 0..24 {
    for x in 0..80 {
      let cell = &buffer[(x, y)];
      let char = cell.symbol();
      if char == "─"
        || char == "│"
        || char == "┌"
        || char == "┐"
        || char == "└"
        || char == "┘"
      {
        if cell.fg == Color::Blue {
          found_blue_border = true;
        }
        if cell.fg == Color::Red {
          found_red_border = true;
        }
      }
    }
  }

  assert!(
    found_blue_border,
    "CLI theme should override config theme with blue"
  );
  assert!(
    !found_red_border,
    "Config theme red should be overridden by CLI"
  );
}

#[tokio::test]
async fn test_theme_with_time_widget() {
  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.mode = Mode::Username;
    g.time = true;

    let mut config = Config::default();
    config.display.show_time = true;
    config.theme.time = Some("lightred".to_string());

    g.loaded_config = Some(config.clone());
    g.apply_config(&config);
    g.apply_theme_config(&config.theme, None);
  }

  let buffer = render_ui(greeter, 80, 24).await;

  // Verify time is displayed and theme is applied
  let mut found_time = false;
  let mut found_lightred = false;

  for y in 0..24 {
    for x in 0..80 {
      let cell = &buffer[(x, y)];
      let symbol = cell.symbol();

      // Check for time-like content (digits and colons)
      if symbol.chars().any(|c| c.is_ascii_digit() || c == ':') {
        found_time = true;
      }

      // Check for light red color anywhere
      if cell.fg == Color::LightRed {
        found_lightred = true;
      }
    }
  }

  assert!(found_time, "Time widget should be displayed");
  assert!(
    found_lightred,
    "Theme color (lightred) should be applied somewhere in the UI"
  );
}

#[tokio::test]
async fn test_doom_animation_renders_and_form_stays_legible() {
  use crate::ui::bg_animation::{self as animation, AnimationSpec, doom};

  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.mode = Mode::Username;
    g.animation = Some(animation::build(&AnimationSpec::Doom(
      doom::Options::default(),
    )));
  }

  // Render a few frames so the fire actually propagates upward.
  let mut buffer = render_ui(greeter.clone(), 80, 24).await;
  for _ in 0..40 {
    buffer = render_ui(greeter.clone(), 80, 24).await;
  }

  // Some cell on the bottom row should be a fire glyph — the bottom is
  // re-stamped to STEPS=12 every frame.
  let bottom = 23u16;
  let fire_glyphs = ['░', '▒', '▓', '█'];
  let bottom_has_fire = (0..80).any(|x| {
    buffer[(x, bottom)]
      .symbol()
      .chars()
      .next()
      .map(|c| fire_glyphs.contains(&c))
      .unwrap_or(false)
  });
  assert!(bottom_has_fire, "fire should render on the bottom row");

  // The login form sits in the middle of the screen; its inner cells must
  // not contain fire glyphs because Clear wipes them before the form draws.
  let form_y = 12u16;
  let form_has_fire = (20..60).any(|x| {
    buffer[(x, form_y)]
      .symbol()
      .chars()
      .next()
      .map(|c| fire_glyphs.contains(&c))
      .unwrap_or(false)
  });
  assert!(!form_has_fire, "fire must not bleed through the login form");
}

#[tokio::test]
async fn test_matrix_animation_renders_and_form_stays_legible() {
  use crate::ui::bg_animation::{self as animation, AnimationSpec, matrix};

  let greeter = test_greeter();
  {
    let mut g = greeter.write().await;
    g.mode = Mode::Username;
    g.animation = Some(animation::build(&AnimationSpec::Matrix(
      matrix::Options::default(),
    )));
  }

  // Render enough frames for streams to descend into the screen
  let mut buffer = render_ui(greeter.clone(), 80, 24).await;
  for _ in 0..60 {
    buffer = render_ui(greeter.clone(), 80, 24).await;
  }

  // At least one painted cell anywhere outside the login form area
  let painted_outside_form = (0..80).any(|x| {
    (0..6).chain(18..24).any(|y: u16| {
      let sym = buffer[(x, y)].symbol();
      sym != " " && !sym.is_empty()
    })
  });
  assert!(
    painted_outside_form,
    "matrix should paint something outside the form area after 60 frames"
  );

  // No matrix glyphs may bleed through the form
  // Only flag the katakana range since form labels can contain ASCII digits
  let form_y = 12u16;
  let katakana_in_form = (20..60).any(|x| {
    buffer[(x, form_y)]
      .symbol()
      .chars()
      .next()
      .is_some_and(|c| ('ｦ'..='ﾝ').contains(&c))
  });
  assert!(
    !katakana_in_form,
    "matrix glyphs must not bleed through the login form"
  );
}
