use std::io;

use crossterm::event::KeyCode;
use tokio::task::spawn;

use super::common::{IntegrationRunner, SessionOptions};
use crate::ui::{
  common::{masked::MaskedString, menu::Menu},
  users::User,
};

#[tokio::test]
async fn remember_username() -> io::Result<()> {
  let opts = SessionOptions {
    username: "apognu".to_string(),
    password: "password".to_string(),
    mfa:      false,
  };

  let runner = IntegrationRunner::new(
    opts,
    Some(|greeter| {
      greeter.remember = true;
      greeter.username = MaskedString::from("apognu".to_string(), None);
    }),
  )?;

  let events = spawn({
    let runner = runner.clone();

    async move {
      runner.wait_until_buffer_contains("Username:").await?;

      assert!(runner.output().await?.contains("Username: apognu"));

      runner.wait_until_buffer_contains("Password:").await?;
      runner.send_key(KeyCode::Esc).await?;
      runner.wait_for_render().await?;

      assert!(runner.output().await?.contains("Username:       "));
      assert!(!runner.output().await?.contains("Password:"));
      Ok(())
    }
  });

  runner.join_until_end(events).await
}

#[tokio::test]
async fn remember_username_with_sole_user_in_menu() -> io::Result<()> {
  let opts = SessionOptions {
    username: "apognu".to_string(),
    password: "password".to_string(),
    mfa:      false,
  };

  let runner = IntegrationRunner::new(
    opts,
    Some(|greeter| {
      greeter.remember = true;
      greeter.username = MaskedString::from("apognu".to_string(), None);
      greeter.user_menu = true;
      greeter.users = Menu::<User> {
        title:    "The users".to_string(),
        options:  vec![User {
          username: "apognu".to_string(),
          name:     None,
        }],
        selected: 0,
      };
    }),
  )?;

  let events = spawn({
    let runner = runner.clone();

    async move {
      runner.wait_until_buffer_contains("Password:").await?;
      runner.wait_for_render().await?;

      assert!(!runner.output().await?.contains("Authentication failed"));

      runner.send_text("password").await
    }
  });

  runner.join_until_client_exit(events).await
}
