use crossterm::event::{KeyCode, KeyModifiers};
use libgreetd_stub::SessionOptions;

use super::common::IntegrationRunner;
use crate::{
  power::PowerOption,
  ui::{common::menu::Menu, power::Power, sessions::Session, users::User},
};

#[tokio::test]
async fn menus_labels_default() {
  let opts = SessionOptions {
    username: "apognu".to_string(),
    password: "password".to_string(),
    mfa:      false,
  };

  let mut runner = IntegrationRunner::new(opts, None).await;

  let events = tokio::task::spawn({
    let mut runner = runner.clone();

    async move {
      runner.wait_until_buffer_contains("Username:").await;

      assert!(runner.output().await.contains("F2 Change command"));
      assert!(runner.output().await.contains("F3 Choose session"));
      assert!(runner.output().await.contains("F12 Power"));
    }
  });

  runner.join_until_end(events).await;
}

#[tokio::test]
async fn menus_labels_with_custom_bindings() {
  let opts = SessionOptions {
    username: "apognu".to_string(),
    password: "password".to_string(),
    mfa:      false,
  };

  let mut runner = IntegrationRunner::new(
    opts,
    Some(|greeter| {
      greeter.kb_command = 11;
      greeter.kb_sessions = 1;
      greeter.kb_power = 6;
    }),
  )
  .await;

  let events = tokio::task::spawn({
    let mut runner = runner.clone();

    async move {
      runner.wait_until_buffer_contains("Username:").await;

      assert!(runner.output().await.contains("F11 Change command"));
      assert!(runner.output().await.contains("F1 Choose session"));
      assert!(runner.output().await.contains("F6 Power"));
    }
  });

  runner.join_until_end(events).await;
}

#[tokio::test]
async fn change_command() {
  let opts = SessionOptions {
    username: "apognu".to_string(),
    password: "password".to_string(),
    mfa:      false,
  };

  let mut runner = IntegrationRunner::new(opts, None).await;

  let events = tokio::task::spawn({
    let mut runner = runner.clone();

    async move {
      runner.wait_until_buffer_contains("Username:").await;
      runner.send_key(KeyCode::F(3)).await;
      runner.wait_for_render().await;

      assert!(runner.output().await.contains("CMD uname"));

      runner.send_key(KeyCode::F(2)).await;
      runner.wait_for_render().await;

      assert!(runner.output().await.contains("Change session command"));
      assert!(runner.output().await.contains("New command: uname"));

      runner
        .send_modified_key(KeyCode::Char('u'), KeyModifiers::CONTROL)
        .await;
      runner.send_text("mynewcommand").await;
      runner.send_key(KeyCode::Enter).await;
      runner.wait_for_render().await;

      assert!(runner.output().await.contains("CMD mynewcommand"));
    }
  });

  runner.join_until_end(events).await;
}

#[tokio::test]
async fn session_menu() {
  let opts = SessionOptions {
    username: "apognu".to_string(),
    password: "password".to_string(),
    mfa:      false,
  };

  let mut runner = IntegrationRunner::new(
    opts,
    Some(|greeter| {
      greeter.sessions = Menu::<Session> {
        title:    "List of sessions".to_string(),
        options:  vec![
          Session {
            name: "My Session".to_string(),
            ..Default::default()
          },
          Session {
            name: "Second Session".to_string(),
            ..Default::default()
          },
        ],
        selected: 0,
      };
    }),
  )
  .await;

  let events = tokio::task::spawn({
    let mut runner = runner.clone();

    async move {
      runner.wait_until_buffer_contains("Username:").await;
      runner.send_key(KeyCode::F(3)).await;
      runner.wait_for_render().await;

      assert!(runner.output().await.contains("List of sessions"));
      assert!(runner.output().await.contains("My Session"));
      assert!(runner.output().await.contains("Second Session"));

      runner.send_key(KeyCode::Down).await;
      runner.send_key(KeyCode::Down).await;
      runner.send_key(KeyCode::Enter).await;
      runner.wait_for_render().await;

      assert!(runner.output().await.contains("SESS Second Session"));

      runner.send_key(KeyCode::F(3)).await;
      runner.wait_for_render().await;
      runner.send_key(KeyCode::Up).await;
      runner.send_key(KeyCode::Up).await;
      runner.send_key(KeyCode::Enter).await;
      runner.wait_for_render().await;

      assert!(runner.output().await.contains("SESS My Session"));
    }
  });

  runner.join_until_end(events).await;
}

#[tokio::test]
async fn power_menu() {
  let opts = SessionOptions {
    username: "apognu".to_string(),
    password: "password".to_string(),
    mfa:      false,
  };

  let mut runner = IntegrationRunner::new(
    opts,
    Some(|greeter| {
      greeter.powers = Menu::<Power> {
        title:    "What to do?".to_string(),
        options:  vec![
          Power {
            action: PowerOption::Shutdown,
            label: "Turn it off".to_string(),
            ..Default::default()
          },
          Power {
            action: PowerOption::Reboot,
            label: "And back on again".to_string(),
            ..Default::default()
          },
        ],
        selected: 0,
      };
    }),
  )
  .await;

  let events = tokio::task::spawn({
    let mut runner = runner.clone();

    async move {
      runner.wait_until_buffer_contains("Username:").await;
      runner.send_key(KeyCode::F(12)).await;
      runner.wait_for_render().await;

      assert!(runner.output().await.contains("What to do?"));
      assert!(runner.output().await.contains("Turn it off"));
      assert!(runner.output().await.contains("And back on again"));
    }
  });

  runner.join_until_end(events).await;
}

#[tokio::test]
async fn users_menu() {
  let opts = SessionOptions {
    username: "apognu".to_string(),
    password: "password".to_string(),
    mfa:      false,
  };

  let mut runner = IntegrationRunner::new(
    opts,
    Some(|greeter| {
      greeter.user_menu = true;
      greeter.users = Menu::<User> {
        title:    "The users".to_string(),
        options:  vec![
          User {
            username: "apognu".to_string(),
            name:     Some("Antoine POPINEAU".to_string()),
          },
          User {
            username: "bob".to_string(),
            name:     Some("Bob JOE".to_string()),
          },
        ],
        selected: 0,
      }
    }),
  )
  .await;

  let events = tokio::task::spawn({
    let mut runner = runner.clone();

    async move {
      runner.wait_until_buffer_contains("select a user").await;

      runner.send_key(KeyCode::Enter).await;
      runner.wait_for_render().await;

      assert!(runner.output().await.contains("Antoine POPINEAU"));
      assert!(runner.output().await.contains("Bob JOE"));

      runner.send_key(KeyCode::Down).await;
      runner.send_key(KeyCode::Enter).await;
      runner.wait_for_render().await;

      assert!(runner.output().await.contains("Username: Bob JOE"));
      assert!(runner.output().await.contains("Password:"));

      runner.send_key(KeyCode::Esc).await;
      runner.wait_for_render().await;

      runner.wait_until_buffer_contains("select a user").await;

      runner.send_text("otheruser").await;
      runner.wait_for_render().await;

      assert!(runner.output().await.contains("Username: otheruser"));
      assert!(runner.output().await.contains("Password:"));

      runner.send_key(KeyCode::Esc).await;
      runner.wait_for_render().await;
      runner.send_key(KeyCode::Enter).await;
      runner.send_key(KeyCode::Up).await;
      runner.send_key(KeyCode::Enter).await;
      runner.wait_for_render().await;
      runner.wait_for_render().await;

      assert!(runner.output().await.contains("Username: Antoine POPINEAU"));
      assert!(runner.output().await.contains("Password:"));

      runner.send_text("password").await;
    }
  });

  runner.join_until_client_exit(events).await;
}
