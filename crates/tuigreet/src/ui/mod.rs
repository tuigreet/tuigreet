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
mod status;
#[cfg(test)] mod tests;
pub mod users;
mod util;

use std::{
  error::Error,
  io::{self, Write},
  sync::Arc,
};

use tokio::sync::RwLock;
use tui::{
  Frame as CrosstermFrame,
  Terminal,
  backend::Backend,
  layout::Rect,
  style::Modifier,
  text::Span,
  widgets::Clear,
};
use tuigreet_config::IdleAction;
use tuigreet_types::Mode;

use self::common::style::{Theme, Themed};
pub use self::i18n::MESSAGES;
use crate::{Greeter, ui::util::should_hide_cursor};

pub type Frame<'a> = CrosstermFrame<'a>;

pub async fn draw<B>(
  greeter: Arc<RwLock<Greeter>>,
  terminal: &mut Terminal<B>,
) -> Result<(), Box<dyn Error>>
where
  B: Backend,
  B::Error: 'static,
{
  let mut greeter = greeter.write().await;
  let hide_cursor = should_hide_cursor(&greeter);

  terminal.draw(|f| {
    let area = f.area();
    if greeter.is_idle() && greeter.idle_action() == IdleAction::Blank {
      f.render_widget(Clear, area);
      return;
    }

    let pause_animation =
      greeter.is_idle() && greeter.idle_action() == IdleAction::PauseAnimation;
    if let Some(animation) = greeter.animation.as_mut() {
      animation.resize(area);
      if !pause_animation {
        animation.step();
      }
      animation.render(area, f.buffer_mut());
    }

    let theme = &greeter.theme;
    let main_area = {
      let widgets = greeter
        .loaded_config
        .as_ref()
        .map(|config| &config.layout.widgets);
      let regions = widgets
        .map(|widgets| &widgets.regions)
        .filter(|regions| regions.is_configured());
      if let Some(regions) = regions {
        status::render_regions(f, area, &greeter, theme, regions)
      } else {
        status::render_legacy(
          f,
          area,
          &greeter,
          theme,
          widgets
            .map_or_else(Default::default, |widgets| widgets.time_position),
          widgets
            .map_or_else(Default::default, |widgets| widgets.status_position),
          widgets
            .map_or_else(Default::default, |widgets| widgets.battery_position),
        )
      }
    };
    draw_main_content(&mut greeter, f, main_area, hide_cursor);
  })?;
  drop(greeter);
  io::stdout().flush()?;
  Ok(())
}

fn draw_main_content(
  greeter: &mut Greeter,
  f: &mut Frame,
  area: Rect,
  hide_cursor: bool,
) {
  let cursor = match greeter.mode {
    Mode::Command => self::command::draw_with_area(greeter, f, area).ok(),
    Mode::Sessions => greeter.sessions.draw_with_area(greeter, f, area).ok(),
    Mode::Power => greeter.powers.draw_with_area(greeter, f, area).ok(),
    Mode::Background => {
      greeter.backgrounds.draw_with_area(greeter, f, area).ok()
    },
    Mode::Users => greeter.users.draw_with_area(greeter, f, area).ok(),
    Mode::Processing => {
      Some(self::processing::draw_with_area(greeter, f, area))
    },
    _ => self::prompt::draw_with_area(greeter, f, area).ok(),
  };

  if !hide_cursor && let Some(cursor) = cursor {
    f.set_cursor_position((cursor.0 - 1, cursor.1 - 1));
  }
}

fn prompt_value<'s, S>(theme: &Theme, text: Option<S>) -> Span<'s>
where
  S: Into<String>,
{
  text.map_or_else(
    || Span::from(""),
    |text| {
      Span::styled(
        text.into(),
        theme.of(&[Themed::Prompt]).add_modifier(Modifier::BOLD),
      )
    },
  )
}
