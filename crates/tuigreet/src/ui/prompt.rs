use std::{borrow::Cow, error::Error};

use rand::{RngExt, SeedableRng, prelude::StdRng};
use tui::{
  layout::{Alignment, Constraint, Direction, Layout, Rect},
  style::Modifier,
  text::Span,
  widgets::{Block, BorderType, Borders, Clear, Paragraph},
};
use tuigreet_types::{GreetAlign, Mode, SecretDisplay};

use super::common::style::Themed;
use crate::{
  Greeter,
  info::get_hostname,
  ui::{
    Frame,
    prompt_value,
    util::{
      get_cursor_offset,
      get_greeting_height,
      get_input_width,
      get_message_height,
      get_rect_bounds,
      titleize,
    },
  },
};

const GREETING_INDEX: usize = 0;
const USERNAME_INDEX: usize = 1;
const ANSWER_INDEX: usize = 3;
const USERNAME_ROW: u16 = 1;
const ANSWER_ROW: u16 = 3;

pub fn draw_with_area(
  greeter: &mut Greeter,
  f: &mut Frame,
  area: Rect,
) -> Result<(u16, u16), Box<dyn Error>> {
  let theme = &greeter.theme;

  let size = area;
  let (x, y, width, height) = get_rect_bounds(greeter, size, 0);

  let container_padding = greeter.container_padding();
  let prompt_padding = greeter.prompt_padding();
  let greeting_alignment = match greeter.greet_align() {
    GreetAlign::Center => Alignment::Center,
    GreetAlign::Left => Alignment::Left,
    GreetAlign::Right => Alignment::Right,
  };

  let container = Rect::new(x, y, width, height);
  let frame = Rect::new(
    x + container_padding,
    y + container_padding,
    width - (2 * container_padding),
    height - (2 * container_padding),
  );

  // Wipe any animation pixels from the container area before drawing the
  // prompt; ratatui's Block patches style only and would leave the fire
  // glyphs visible underneath the form.
  if greeter.animation.is_some() {
    f.render_widget(Clear, container);
  }

  let mut block = Block::default()
    .title_style(theme.of(&[Themed::Title]))
    .style(theme.of(&[Themed::Container]))
    .borders(Borders::ALL)
    .border_type(BorderType::Plain)
    .border_style(theme.of(&[Themed::Border]));

  if greeter.title.enable {
    let title_text = greeter.title.custom.as_ref().map_or_else(
      || fl!("title_authenticate", hostname = get_hostname()),
      Clone::clone,
    );
    let hostname = Span::from(titleize(&title_text));
    block = block.title(hostname);
  }

  f.render_widget(block, container);

  let (message, message_height) =
    get_message_height(greeter, container_padding, 1);
  let (greeting, greeting_height) =
    get_greeting_height(greeter, container_padding, 0);

  let should_display_answer = greeter.mode == Mode::Password;

  let constraints = [
    Constraint::Length(greeting_height), // Greeting
    Constraint::Length(1),               // Username
    Constraint::Length(if should_display_answer {
      prompt_padding
    } else {
      0
    }), // Prompt padding
    Constraint::Length(u16::from(should_display_answer)), // Answer
  ];

  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints(constraints.as_ref())
    .split(frame);
  let cursor = chunks[USERNAME_INDEX];

  if let Some(greeting) = greeting {
    let greeting_label = greeting
      .alignment(greeting_alignment)
      .style(theme.of(&[Themed::Greet]));

    f.render_widget(greeting_label, chunks[GREETING_INDEX]);
  }

  let username_label_text = fl!("username");
  let username_label_width =
    u16::try_from(username_label_text.chars().count())?;
  let username_input_width =
    get_input_width(greeter, width, Some(&username_label_text));
  let prompt_width = u16::try_from(greeter.prompt_width())?;
  let username_label = if greeter.user_menu && greeter.username.value.is_empty()
  {
    let prompt_text = Span::from(fl!("select_user"));

    Paragraph::new(prompt_text).alignment(Alignment::Center)
  } else {
    let username_text = prompt_value(theme, Some(username_label_text));

    Paragraph::new(username_text)
  };

  let username = greeter.username.get();
  let username_value_text = Span::from(username);
  let username_value =
    Paragraph::new(username_value_text).style(theme.of(&[Themed::Input]));

  match greeter.mode {
    Mode::Username | Mode::Password | Mode::Action => {
      f.render_widget(username_label, chunks[USERNAME_INDEX]);

      if !greeter.user_menu || !greeter.username.value.is_empty() {
        f.render_widget(
          username_value,
          Rect::new(
            1 + chunks[USERNAME_INDEX].x + username_label_width,
            chunks[USERNAME_INDEX].y,
            username_input_width,
            1,
          ),
        );
      }

      let answer_text = if greeter.working {
        Span::from(fl!("wait"))
      } else {
        prompt_value(theme, greeter.prompt.as_ref())
      };

      let answer_label =
        Paragraph::new(answer_text).style(theme.of(&[Themed::Prompt]));

      if greeter.mode == Mode::Password
        || greeter.previous_mode == Mode::Password
      {
        f.render_widget(answer_label, chunks[ANSWER_INDEX]);

        if greeter.is_secret_revealed()
          || !greeter.asking_for_secret
          || greeter.secret_display.show()
        {
          let value = secret_value(greeter)?;

          let answer_value_text = Span::from(value);
          let answer_value =
            Paragraph::new(answer_value_text).style(theme.of(&[Themed::Input]));

          f.render_widget(
            answer_value,
            Rect::new(
              chunks[ANSWER_INDEX].x + prompt_width,
              chunks[ANSWER_INDEX].y,
              get_input_width(greeter, width, greeter.prompt.as_deref()),
              1,
            ),
          );

          if greeter.asking_for_secret && greeter.is_secret_revealed() {
            let warning = Paragraph::new(fl!("status_password_visible"))
              .alignment(Alignment::Right)
              .style(
                theme
                  .of(&[Themed::ActionButton])
                  .add_modifier(Modifier::BOLD | Modifier::REVERSED),
              );
            f.render_widget(warning, chunks[ANSWER_INDEX]);
          }
        }
      }

      if let Some(message) = message {
        let message = message
          .alignment(Alignment::Center)
          .style(greeter.theme.of(&[Themed::Text]));

        f.render_widget(
          message,
          Rect::new(x, y + height, width, message_height),
        );
      }
    },

    _ => {},
  }

  match greeter.mode {
    Mode::Username => {
      let username_length = greeter.username.get().chars().count();
      let offset = u16::try_from(get_cursor_offset(greeter, username_length)?)?;

      Ok((
        2 + cursor.x + username_label_width + offset,
        USERNAME_ROW + cursor.y,
      ))
    },

    Mode::Password => {
      let answer_length = greeter.buffer.chars().count();
      let offset = u16::try_from(get_cursor_offset(greeter, answer_length)?)?;

      if greeter.asking_for_secret && !greeter.secret_display.show() {
        Ok((
          1 + cursor.x + prompt_width,
          ANSWER_ROW + prompt_padding + cursor.y - 1,
        ))
      } else {
        Ok((
          1 + cursor.x + prompt_width + offset,
          ANSWER_ROW + prompt_padding + cursor.y - 1,
        ))
      }
    },

    _ => Ok((1, 1)),
  }
}

fn secret_value(greeter: &Greeter) -> Result<Cow<'_, str>, Box<dyn Error>> {
  if greeter.asking_for_secret && greeter.is_secret_revealed() {
    return Ok(Cow::Borrowed(greeter.buffer.as_str()));
  }
  let (true, SecretDisplay::Character(pool)) =
    (greeter.asking_for_secret, &greeter.secret_display)
  else {
    return Ok(Cow::Borrowed(greeter.buffer.as_str()));
  };

  let characters = pool.chars().collect::<Vec<_>>();
  if characters.is_empty() {
    return Err("secret display character pool must not be empty".into());
  }
  if characters.len() == 1 {
    return Ok(Cow::Owned(pool.repeat(greeter.buffer.chars().count())));
  }

  let mut rng = StdRng::seed_from_u64(0);
  let mut masked = String::with_capacity(greeter.buffer.len());
  for _ in greeter.buffer.chars() {
    masked.push(characters[rng.random_range(0..characters.len())]);
  }
  Ok(Cow::Owned(masked))
}
