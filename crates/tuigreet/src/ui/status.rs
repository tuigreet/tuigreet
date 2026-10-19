use std::borrow::Cow;

use chrono::prelude::*;
use tui::{
  layout::{Alignment, Constraint, Direction, Layout, Rect},
  style::Modifier,
  text::{Line, Span},
  widgets::Paragraph,
};
use tuigreet_config::{
  BatteryPosition,
  BindingAction,
  RegionConfig,
  RegionItem,
  WidgetPosition,
};
use tuigreet_types::Mode;
use unicode_width::UnicodeWidthStr;

use super::{
  Frame,
  common::style::{Theme, Themed},
};
use crate::{
  Greeter,
  info::{capslock_status, get_battery_info},
  ui::sessions::SessionSource,
};

const REGION_TOP_INDEX: usize = 1;
const REGION_MAIN_INDEX: usize = 2;
const REGION_BOTTOM_INDEX: usize = 3;
const LEGACY_INFO_TOP_INDEX: usize = 1;
const LEGACY_STATUS_TOP_INDEX: usize = 2;
const LEGACY_MAIN_INDEX: usize = 3;
const LEGACY_STATUS_BOTTOM_INDEX: usize = 4;
const LEGACY_TIME_BOTTOM_INDEX: usize = 5;
const PADDED_CONTENT_INDEX: usize = 1;
const STATUS_LEFT_INDEX: usize = 1;
const STATUS_RIGHT_INDEX: usize = 3;

#[derive(Clone, Copy)]
enum Button {
  Command,
  Session,
  Power,
  Background,
  Other,
}

pub(super) fn render_regions(
  f: &mut Frame,
  area: Rect,
  greeter: &Greeter,
  theme: &Theme,
  regions: &RegionConfig,
) -> Rect {
  let top_visible =
    !regions.top_left.is_empty() || !regions.top_right.is_empty();
  let bottom_visible =
    !regions.bottom_left.is_empty() || !regions.bottom_right.is_empty();
  let chunks = Layout::default()
    .constraints([
      Constraint::Length(greeter.window_padding()),
      Constraint::Length(u16::from(top_visible)),
      Constraint::Min(1),
      Constraint::Length(u16::from(bottom_visible)),
      Constraint::Length(greeter.window_padding()),
    ])
    .split(area);

  if top_visible {
    render_region_row(
      f,
      chunks[REGION_TOP_INDEX],
      &regions.top_left,
      &regions.top_right,
      greeter,
      theme,
    );
  }
  if bottom_visible {
    render_region_row(
      f,
      chunks[REGION_BOTTOM_INDEX],
      &regions.bottom_left,
      &regions.bottom_right,
      greeter,
      theme,
    );
  }
  chunks[REGION_MAIN_INDEX]
}

pub(super) fn render_legacy(
  f: &mut Frame,
  area: Rect,
  greeter: &Greeter,
  theme: &Theme,
  time_position: WidgetPosition,
  status_position: WidgetPosition,
  battery_position: BatteryPosition,
) -> Rect {
  let time_at_top = greeter.time
    && !matches!(
      time_position,
      WidgetPosition::Hidden | WidgetPosition::Bottom
    );
  let time_at_bottom =
    greeter.time && matches!(time_position, WidgetPosition::Bottom);
  let status_at_top = matches!(status_position, WidgetPosition::Top);
  let status_at_bottom = matches!(
    status_position,
    WidgetPosition::Default | WidgetPosition::Bottom
  );
  let info_at_top = greeter.battery || time_at_top;

  let chunks = Layout::default()
    .constraints([
      Constraint::Length(greeter.window_padding()),
      Constraint::Length(u16::from(info_at_top)),
      Constraint::Length(u16::from(status_at_top)),
      Constraint::Min(1),
      Constraint::Length(u16::from(status_at_bottom)),
      Constraint::Length(u16::from(time_at_bottom)),
      Constraint::Length(greeter.window_padding()),
    ])
    .split(area);

  if info_at_top {
    render_top_info(
      f,
      chunks[LEGACY_INFO_TOP_INDEX],
      greeter,
      theme,
      time_at_top,
      battery_position,
    );
  }
  if status_at_top {
    render_legacy_status(f, chunks[LEGACY_STATUS_TOP_INDEX], greeter, theme);
  }
  if status_at_bottom {
    render_legacy_status(f, chunks[LEGACY_STATUS_BOTTOM_INDEX], greeter, theme);
  }
  if time_at_bottom {
    render_bottom_time(f, chunks[LEGACY_TIME_BOTTOM_INDEX], greeter, theme);
  }
  chunks[LEGACY_MAIN_INDEX]
}

fn render_top_info(
  f: &mut Frame,
  area: Rect,
  greeter: &Greeter,
  theme: &Theme,
  show_time: bool,
  battery_position: BatteryPosition,
) {
  let padded = padded_row(area, greeter.window_padding());
  if show_time {
    let text = get_time(greeter);
    let chunks = Layout::default()
      .direction(Direction::Horizontal)
      .constraints([
        Constraint::Fill(1),
        Constraint::Length(layout_text_width(&text)),
        Constraint::Fill(1),
      ])
      .split(padded);
    f.render_widget(
      Paragraph::new(Span::from(text)).style(theme.of(&[Themed::Time])),
      chunks[1],
    );
  }

  if greeter.battery
    && let Some(text) = battery_text()
  {
    let width = Constraint::Length(layout_text_width(&text));
    let (slot, constraints) = match battery_position {
      BatteryPosition::Left => (0, [width, Constraint::Fill(1)]),
      BatteryPosition::Right => (1, [Constraint::Fill(1), width]),
    };
    let chunks = Layout::default()
      .direction(Direction::Horizontal)
      .constraints(constraints)
      .split(padded);
    f.render_widget(
      Paragraph::new(Span::from(text)).style(theme.of(&[Themed::Time])),
      chunks[slot],
    );
  }
}

fn render_bottom_time(
  f: &mut Frame,
  area: Rect,
  greeter: &Greeter,
  theme: &Theme,
) {
  let text = get_time(greeter);
  let chunks = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([
      Constraint::Length(greeter.window_padding()),
      Constraint::Fill(1),
      Constraint::Length(layout_text_width(&text)),
      Constraint::Fill(1),
      Constraint::Length(greeter.window_padding()),
    ])
    .split(area);
  f.render_widget(
    Paragraph::new(Span::from(text)).style(theme.of(&[Themed::Time])),
    chunks[2],
  );
}

fn render_legacy_status(
  f: &mut Frame,
  area: Rect,
  greeter: &Greeter,
  theme: &Theme,
) {
  let left = legacy_action_spans(greeter, theme);
  let left_width = spans_width(&left);
  let right = state_status_spans(greeter, theme);
  let right_width = spans_width(&right);
  let chunks = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([
      Constraint::Length(greeter.window_padding()),
      Constraint::Length(left_width),
      Constraint::Fill(1),
      Constraint::Length(right_width),
      Constraint::Length(greeter.window_padding()),
    ])
    .split(area);

  f.render_widget(Paragraph::new(Line::from(left)), chunks[STATUS_LEFT_INDEX]);
  if right_width > 0 {
    f.render_widget(
      Paragraph::new(Line::from(right)),
      chunks[STATUS_RIGHT_INDEX],
    );
  }
}

fn render_region_row(
  f: &mut Frame,
  area: Rect,
  left: &[RegionItem],
  right: &[RegionItem],
  greeter: &Greeter,
  theme: &Theme,
) {
  let halves = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
    .split(padded_row(area, greeter.window_padding()));
  f.render_widget(
    Paragraph::new(Line::from(region_spans(greeter, theme, left))),
    halves[0],
  );
  f.render_widget(
    Paragraph::new(Line::from(region_spans(greeter, theme, right)))
      .alignment(Alignment::Right),
    halves[1],
  );
}

fn padded_row(area: Rect, padding: u16) -> Rect {
  Layout::default()
    .direction(Direction::Horizontal)
    .constraints([
      Constraint::Length(padding),
      Constraint::Fill(1),
      Constraint::Length(padding),
    ])
    .split(area)[PADDED_CONTENT_INDEX]
}

fn region_spans<'a>(
  greeter: &'a Greeter,
  theme: &Theme,
  items: &[RegionItem],
) -> Vec<Span<'a>> {
  let mut spans = Vec::new();
  for item in items {
    let item_start = spans.len();
    if item_start > 0 {
      spans.push(Span::from("  "));
    }
    let content_start = spans.len();
    append_region_item(&mut spans, content_start, greeter, theme, *item);
    if spans.len() == content_start {
      spans.truncate(item_start);
    }
  }
  spans
}

fn append_region_item<'a>(
  spans: &mut Vec<Span<'a>>,
  start: usize,
  greeter: &'a Greeter,
  theme: &Theme,
  item: RegionItem,
) {
  match item {
    RegionItem::Time => append_region_time(spans, greeter, theme),
    RegionItem::Battery => append_region_battery(spans, greeter, theme),
    RegionItem::Actions => append_actions(spans, start, greeter, theme),
    RegionItem::Session => {
      if greeter.status_bar().show_session_status {
        append_session_status(spans, start, greeter, theme);
      }
    },
    RegionItem::CapsLock => append_region_caps_lock(spans, greeter, theme),
  }
}

fn append_region_time(
  spans: &mut Vec<Span<'_>>,
  greeter: &Greeter,
  theme: &Theme,
) {
  if greeter.time {
    spans.push(Span::styled(get_time(greeter), theme.of(&[Themed::Time])));
  }
}

fn append_region_battery(
  spans: &mut Vec<Span<'_>>,
  greeter: &Greeter,
  theme: &Theme,
) {
  if greeter.battery
    && let Some(text) = battery_text()
  {
    spans.push(Span::styled(text, theme.of(&[Themed::Time])));
  }
}

fn append_region_caps_lock(
  spans: &mut Vec<Span<'_>>,
  greeter: &Greeter,
  theme: &Theme,
) {
  if greeter.status_bar().show_caps_lock && capslock_status() {
    spans.push(status_label(theme, fl!("status_caps")));
  }
}

fn legacy_action_spans<'a>(
  greeter: &'a Greeter,
  theme: &Theme,
) -> Vec<Span<'a>> {
  let mut spans = Vec::new();
  append_actions(&mut spans, 0, greeter, theme);
  if greeter.status_bar().show_session_status {
    append_session_status(&mut spans, 0, greeter, theme);
  }
  spans
}

fn append_actions<'a>(
  spans: &mut Vec<Span<'a>>,
  start: usize,
  greeter: &'a Greeter,
  theme: &Theme,
) {
  if greeter.status_bar().show_reset {
    append_status_pair(
      spans,
      start,
      greeter,
      theme,
      Button::Other,
      Cow::Borrowed("ESC"),
      fl!("action_reset"),
    );
  }
  append_action_bindings(spans, start, greeter, theme);
}

fn append_action_bindings<'a>(
  spans: &mut Vec<Span<'a>>,
  start: usize,
  greeter: &'a Greeter,
  theme: &Theme,
) {
  let status = greeter.status_bar();
  for (visible, action, button) in [
    (status.show_command, BindingAction::Command, Button::Command),
    (
      status.show_session,
      BindingAction::Sessions,
      Button::Session,
    ),
    (status.show_power, BindingAction::Power, Button::Power),
    (
      status.show_background,
      BindingAction::Background,
      Button::Background,
    ),
    (
      status.show_password && greeter.asking_for_secret,
      BindingAction::TogglePassword,
      Button::Other,
    ),
  ] {
    if visible && let Some(key) = greeter.key_for_action(action) {
      append_status_pair(
        spans,
        start,
        greeter,
        theme,
        button,
        Cow::Borrowed(key),
        action_label(action),
      );
    }
  }
}

fn action_label(action: BindingAction) -> String {
  match action {
    BindingAction::Command => fl!("action_command"),
    BindingAction::Sessions => fl!("action_session"),
    BindingAction::Power => fl!("action_power"),
    BindingAction::Background => fl!("action_background"),
    BindingAction::TogglePassword => fl!("action_password"),
  }
}

fn append_session_status<'a>(
  spans: &mut Vec<Span<'a>>,
  start: usize,
  greeter: &'a Greeter,
  theme: &Theme,
) {
  let label = match greeter.session_source {
    SessionSource::Session(_) => fl!("status_session"),
    _ => fl!("status_command"),
  };
  let value = greeter.session_source.label(greeter).unwrap_or("-");
  append_status_pair(
    spans,
    start,
    greeter,
    theme,
    Button::Other,
    Cow::Owned(label),
    value,
  );
}

fn append_status_pair<'a>(
  spans: &mut Vec<Span<'a>>,
  start: usize,
  greeter: &Greeter,
  theme: &Theme,
  button: Button,
  label: Cow<'a, str>,
  value: impl AsRef<str>,
) {
  if spans.len() > start {
    spans.push(Span::from(" "));
  }
  spans.push(status_label(theme, label));
  spans.push(status_value(greeter, theme, button, value.as_ref()));
}

fn state_status_spans(greeter: &Greeter, theme: &Theme) -> Vec<Span<'static>> {
  let mut spans = Vec::new();
  if greeter.asking_for_secret && greeter.is_secret_revealed() {
    spans.push(status_label(theme, fl!("status_password_visible")));
  }
  if greeter.status_bar().show_caps_lock && capslock_status() {
    if !spans.is_empty() {
      spans.push(Span::from(" "));
    }
    spans.push(status_label(theme, fl!("status_caps")));
  }
  spans
}

fn spans_width(spans: &[Span<'_>]) -> u16 {
  spans.iter().fold(0, |width, span| {
    width.saturating_add(layout_text_width(span.content.as_ref()))
  })
}

fn layout_text_width(text: &str) -> u16 {
  u16::try_from(UnicodeWidthStr::width(text)).unwrap_or(u16::MAX)
}

fn get_time(greeter: &Greeter) -> String {
  let format = greeter
    .time_format
    .as_ref()
    .map_or_else(|| Cow::Owned(fl!("date")), Cow::Borrowed);
  Local::now()
    .format_localized(&format, greeter.locale)
    .to_string()
}

fn battery_text() -> Option<String> {
  get_battery_info().map(|info| {
    if info.charging {
      format!("{}%+", info.percentage)
    } else {
      format!("{}%", info.percentage)
    }
  })
}

fn status_label<'a>(theme: &Theme, text: impl Into<Cow<'a, str>>) -> Span<'a> {
  Span::styled(
    text,
    theme
      .of(&[Themed::ActionButton])
      .add_modifier(Modifier::REVERSED),
  )
}

fn status_value(
  greeter: &Greeter,
  theme: &Theme,
  button: Button,
  text: &str,
) -> Span<'static> {
  let relevant_mode = match button {
    Button::Command => Some(Mode::Command),
    Button::Session => Some(Mode::Sessions),
    Button::Power => Some(Mode::Power),
    Button::Background => Some(Mode::Background),
    Button::Other => None,
  };
  let style = if relevant_mode == Some(greeter.mode) {
    theme
      .of(&[Themed::ActionButton])
      .add_modifier(Modifier::REVERSED)
  } else {
    theme.of(&[Themed::Action])
  };
  let mut value = String::with_capacity(text.len() + 1);
  value.push(' ');
  value.push_str(text);
  Span::styled(value, style)
}

#[cfg(test)]
mod tests {
  use super::layout_text_width;

  #[test]
  fn layout_width_uses_terminal_column_width() {
    assert_eq!(layout_text_width("界"), 2);
    assert_eq!(layout_text_width("e\u{301}"), 1);
  }
}
