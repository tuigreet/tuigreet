//! UI rendering and component modules

pub mod background;
pub mod bg_animation;
mod command;
pub mod common;
mod i18n;
pub mod power;
mod processing;
mod prompt;
pub mod sessions;
#[cfg(test)] mod tests;
pub mod users;
mod util;

use std::{
  borrow::Cow,
  error::Error,
  io::{self, Write},
  sync::Arc,
};

use chrono::prelude::*;
use sessions::SessionSource;
use tokio::sync::RwLock;
use tui::{
  Frame as CrosstermFrame,
  Terminal,
  layout::{Constraint, Direction, Layout},
  style::Modifier,
  text::{Line, Span},
  widgets::Paragraph,
};
use tuigreet_config::{BatteryPosition, WidgetPosition};
use tuigreet_types::Mode;
use util::buttonize;

use self::common::style::{Theme, Themed};
pub use self::i18n::MESSAGES;
use crate::{
  Greeter,
  info::{capslock_status, get_battery_info},
  ui::util::should_hide_cursor,
};

const INFOBAR_CONTENT_INDEX: usize = 1;
const STATUSBAR_LEFT_INDEX: usize = 1;
const STATUSBAR_RIGHT_INDEX: usize = 3;

pub type Frame<'a> = CrosstermFrame<'a>;

enum Button {
  Command,
  Session,
  Power,
  Background,
  Other,
}

/// Get widget position from config
fn get_widget_position(greeter: &Greeter, widget_name: &str) -> WidgetPosition {
  if let Some(ref config) = greeter.loaded_config {
    match widget_name {
      "time" => config.layout.widgets.time_position.clone(),
      "status" => config.layout.widgets.status_position.clone(),
      _ => WidgetPosition::Default,
    }
  } else {
    WidgetPosition::Default
  }
}

pub async fn draw<B>(
  greeter: Arc<RwLock<Greeter>>,
  terminal: &mut Terminal<B>,
) -> Result<(), Box<dyn Error>>
where
  B: tui::backend::Backend,
  B::Error: 'static,
{
  let mut greeter = greeter.write().await;
  let hide_cursor = should_hide_cursor(&greeter);

  if greeter.needs_clear {
    greeter.needs_clear = false;
    terminal.draw(|f| {
      let area = f.area();
      let buf = f.buffer_mut();
      for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
          if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_char('█');
            cell.set_fg(tui::style::Color::DarkGray);
          }
        }
      }
    })?;
    return Ok(());
  }

  terminal.draw(|f| {
    let area = f.area();
    if let Some(anim) = greeter.animation.as_mut() {
      anim.resize(area);
      anim.step();
      anim.render(area, f.buffer_mut());
    }

    let theme = &greeter.theme;
    let size = area;
    let time_position = get_widget_position(&greeter, "time");
    let status_position = get_widget_position(&greeter, "status");

    let time_at_top = greeter.time
      && !matches!(
        time_position,
        WidgetPosition::Hidden | WidgetPosition::Bottom
      );
    let time_at_bottom =
      greeter.time && matches!(time_position, WidgetPosition::Bottom);

    // Dynamic layout
    let mut constraints = vec![];
    let mut info_top_slot = None;
    let mut time_bottom_slot = None;
    let mut status_slot = None;

    // Top padding
    constraints.push(Constraint::Length(greeter.window_padding()));

    // Top info row: time centered, battery overlaid on left or right.
    if greeter.battery || time_at_top {
      info_top_slot = Some(constraints.len());
      constraints.push(Constraint::Length(1));
    }

    // Status at top (if configured)
    if matches!(status_position, WidgetPosition::Top) {
      status_slot = Some(constraints.len());
      constraints.push(Constraint::Length(1));
    }

    // Main content area
    let main_slot = constraints.len();
    constraints.push(Constraint::Min(1));

    // Status at bottom (default behavior)
    if matches!(
      status_position,
      WidgetPosition::Default | WidgetPosition::Bottom
    ) {
      status_slot = Some(constraints.len());
      constraints.push(Constraint::Length(1));
    }

    // Time at bottom (if configured)
    if time_at_bottom {
      time_bottom_slot = Some(constraints.len());
      constraints.push(Constraint::Length(1));
    }

    // Bottom padding
    constraints.push(Constraint::Length(greeter.window_padding()));

    let chunks = Layout::default().constraints(constraints).split(size);

    // Render top info row: time centered, battery overlaid
    if let Some(slot) = info_top_slot {
      let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
          [
            Constraint::Length(greeter.window_padding()),
            Constraint::Fill(1),
            Constraint::Length(greeter.window_padding()),
          ]
          .as_ref(),
        )
        .split(chunks[slot]);

      if time_at_top {
        let time_text = get_time(&greeter);

        let center_chunks = Layout::default()
          .direction(Direction::Horizontal)
          .constraints(
            [
              Constraint::Fill(1),
              Constraint::Length(time_text.chars().count() as u16),
              Constraint::Fill(1),
            ]
            .as_ref(),
          )
          .split(top_chunks[INFOBAR_CONTENT_INDEX]);

        f.render_widget(
          Paragraph::new(Span::from(time_text))
            .style(theme.of(&[Themed::Time])),
          center_chunks[1],
        );
      }

      if greeter.battery
        && let Some(info) = get_battery_info()
      {
        let battery_text = if info.charging {
          format!("{}%+", info.percentage)
        } else {
          format!("{}%", info.percentage)
        };

        let battery_pos = greeter
          .loaded_config
          .as_ref()
          .map(|c| c.layout.widgets.battery_position.clone())
          .unwrap_or_default();

        let (bat_slot, cons) = match battery_pos {
          BatteryPosition::Left => {
            (0, vec![
              Constraint::Length(battery_text.chars().count() as u16),
              Constraint::Fill(1),
            ])
          },
          BatteryPosition::Right => {
            (1, vec![
              Constraint::Fill(1),
              Constraint::Length(battery_text.chars().count() as u16),
            ])
          },
        };

        let center_chunks = Layout::default()
          .direction(Direction::Horizontal)
          .constraints(cons)
          .split(top_chunks[INFOBAR_CONTENT_INDEX]);

        f.render_widget(
          Paragraph::new(Span::from(battery_text))
            .style(theme.of(&[Themed::Time])),
          center_chunks[bat_slot],
        );
      }
    }

    // Render time at bottom
    if let Some(slot) = time_bottom_slot {
      let time_text = get_time(&greeter);

      let time_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
          [
            Constraint::Length(greeter.window_padding()),
            Constraint::Fill(1),
            Constraint::Length(time_text.chars().count() as u16),
            Constraint::Fill(1),
            Constraint::Length(greeter.window_padding()),
          ]
          .as_ref(),
        )
        .split(chunks[slot]);

      f.render_widget(
        Paragraph::new(Span::from(time_text)).style(theme.of(&[Themed::Time])),
        time_chunks[2],
      );
    }

    // Render status bar if not hidden
    if let Some(slot) = status_slot {
      let status_block_size_right = fl!("status_caps").chars().count() as u16;

      let status_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
          [
            Constraint::Length(greeter.window_padding()),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(status_block_size_right),
            Constraint::Length(greeter.window_padding()),
          ]
          .as_ref(),
        )
        .split(chunks[slot]);

      let session_source_label = match greeter.session_source {
        SessionSource::Session(_) => fl!("status_session"),
        _ => fl!("status_command"),
      };

      let session_source =
        greeter.session_source.label(&greeter).unwrap_or("-");

      let mut status_spans: Vec<Span> = vec![];

      if greeter.status_show_reset {
        status_spans.push(status_label(theme, "ESC"));
        status_spans.push(status_value(
          &greeter,
          theme,
          Button::Other,
          fl!("action_reset"),
        ));
        status_spans.push(Span::from(" "));
      }
      if greeter.status_show_command {
        status_spans
          .push(status_label(theme, format!("F{}", greeter.kb_command)));
        status_spans.push(status_value(
          &greeter,
          theme,
          Button::Command,
          fl!("action_command"),
        ));
        status_spans.push(Span::from(" "));
      }
      if greeter.status_show_session {
        status_spans
          .push(status_label(theme, format!("F{}", greeter.kb_sessions)));
        status_spans.push(status_value(
          &greeter,
          theme,
          Button::Session,
          fl!("action_session"),
        ));
        status_spans.push(Span::from(" "));
      }
      if greeter.status_show_power {
        status_spans
          .push(status_label(theme, format!("F{}", greeter.kb_power)));
        status_spans.push(status_value(
          &greeter,
          theme,
          Button::Power,
          fl!("action_power"),
        ));
        status_spans.push(Span::from(" "));
      }
      if greeter.status_show_background {
        status_spans
          .push(status_label(theme, format!("F{}", greeter.kb_background)));
        status_spans.push(status_value(
          &greeter,
          theme,
          Button::Background,
          fl!("action_background"),
        ));
        status_spans.push(Span::from(" "));
      }
      if greeter.status_show_session_status {
        status_spans.push(status_label(theme, session_source_label));
        status_spans.push(status_value(
          &greeter,
          theme,
          Button::Other,
          session_source,
        ));
      }

      let status_left_text = Line::from(status_spans);
      let status_left = Paragraph::new(status_left_text);

      f.render_widget(status_left, status_chunks[STATUSBAR_LEFT_INDEX]);

      if greeter.status_show_caps_lock && capslock_status() {
        let status_right_text = status_label(theme, fl!("status_caps"));
        let status_right = Paragraph::new(status_right_text);

        f.render_widget(status_right, status_chunks[STATUSBAR_RIGHT_INDEX]);
      }
    }

    // Get the main content area and create a sub-frame for content drawing
    let main_area = chunks[main_slot];

    let cursor = match greeter.mode {
      Mode::Command => {
        self::command::draw_with_area(&mut greeter, f, main_area).ok()
      },
      Mode::Sessions => {
        greeter.sessions.draw_with_area(&greeter, f, main_area).ok()
      },
      Mode::Power => greeter.powers.draw_with_area(&greeter, f, main_area).ok(),
      Mode::Background => {
        greeter
          .backgrounds
          .draw_with_area(&greeter, f, main_area)
          .ok()
      },
      Mode::Users => greeter.users.draw_with_area(&greeter, f, main_area).ok(),
      Mode::Processing => {
        self::processing::draw_with_area(&mut greeter, f, main_area).ok()
      },
      _ => self::prompt::draw_with_area(&mut greeter, f, main_area).ok(),
    };

    if !hide_cursor && let Some(cursor) = cursor {
      f.set_cursor_position((cursor.0 - 1, cursor.1 - 1));
    }
  })?;

  io::stdout().flush()?;

  Ok(())
}

fn get_time(greeter: &Greeter) -> String {
  let format = match &greeter.time_format {
    Some(format) => Cow::Borrowed(format),
    None => Cow::Owned(fl!("date")),
  };

  Local::now()
    .format_localized(&format, greeter.locale)
    .to_string()
}

fn status_label<'s, S>(theme: &Theme, text: S) -> Span<'s>
where
  S: Into<String>,
{
  Span::styled(
    text.into(),
    theme
      .of(&[Themed::ActionButton])
      .add_modifier(Modifier::REVERSED),
  )
}

fn status_value<'s, S>(
  greeter: &Greeter,
  theme: &Theme,
  button: Button,
  text: S,
) -> Span<'s>
where
  S: Into<String>,
{
  let relevant_mode = match button {
    Button::Command => Mode::Command,
    Button::Session => Mode::Sessions,
    Button::Power => Mode::Power,
    Button::Background => Mode::Background,

    _ => {
      return Span::from(buttonize(&text.into()))
        .style(theme.of(&[Themed::Action]));
    },
  };

  let style = if greeter.mode == relevant_mode {
    theme
      .of(&[Themed::ActionButton])
      .add_modifier(Modifier::REVERSED)
  } else {
    theme.of(&[Themed::Action])
  };

  Span::from(buttonize(&text.into())).style(style)
}

fn prompt_value<'s, S>(theme: &Theme, text: Option<S>) -> Span<'s>
where
  S: Into<String>,
{
  match text {
    Some(text) => {
      Span::styled(
        text.into(),
        theme.of(&[Themed::Prompt]).add_modifier(Modifier::BOLD),
      )
    },
    None => Span::from(""),
  }
}
