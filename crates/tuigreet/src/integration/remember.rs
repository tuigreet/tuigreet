use crossterm::event::KeyCode;
use libgreetd_stub::SessionOptions;

use super::common::IntegrationRunner;
use crate::ui::common::masked::MaskedString;

#[tokio::test]
async fn remember_username() {
  let opts = SessionOptions {
    username: "apognu".to_string(),
    password: "password".to_string(),
    mfa:      false,
  };

  let mut runner = IntegrationRunner::new(
    opts,
    Some(|greeter| {
      greeter.remember = true;
      greeter.username = MaskedString::from("apognu".to_string(), None);
    }),
  )
  .await;

  let events = tokio::task::spawn({
    let mut runner = runner.clone();

    async move {
      runner.wait_until_buffer_contains("Username:").await;

      assert!(runner.output().await.contains("Username: apognu"));

      runner.wait_until_buffer_contains("Password:").await;
      runner.send_key(KeyCode::Esc).await;
      runner.wait_for_render().await;
      runner.wait_for_render().await;

      assert!(runner.output().await.contains("Username:       "));
      assert!(!runner.output().await.contains("Password:"));
    }
  });

  runner.join_until_end(events).await;
}
