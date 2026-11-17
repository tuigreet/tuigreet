use std::io;

use crossterm::event::{KeyCode, KeyModifiers};
use tokio::task::spawn;

use super::common::{IntegrationRunner, SessionOptions};

#[tokio::test]
async fn keyboard_movement() -> io::Result<()> {
  let opts = SessionOptions {
    username: "apognu".to_string(),
    password: "password".to_string(),
    mfa:      false,
  };

  let runner = IntegrationRunner::new(opts, None)?;

  let events = spawn({
    let runner = runner.clone();

    async move {
      runner.wait_until_buffer_contains("Username:").await?;
      for char in "apognu".chars() {
        runner.send_key(KeyCode::Char(char)).await?;
      }
      runner.wait_for_render().await?;

      assert!(runner.output().await?.contains("Username: apognu"));

      runner.send_key(KeyCode::Left).await?;
      runner.send_key(KeyCode::Char('l')).await?;
      runner.send_key(KeyCode::Right).await?;
      runner.send_key(KeyCode::Char('r')).await?;
      runner
        .send_modified_key(KeyCode::Char('a'), KeyModifiers::CONTROL)
        .await?;
      runner.send_key(KeyCode::Char('a')).await?;
      runner
        .send_modified_key(KeyCode::Char('e'), KeyModifiers::CONTROL)
        .await?;
      runner.send_key(KeyCode::Char('e')).await?;
      runner.wait_for_render().await?;

      assert!(runner.output().await?.contains("Username: aapognlure"));

      runner.send_key(KeyCode::Left).await?;
      runner
        .send_modified_key(KeyCode::Char('u'), KeyModifiers::CONTROL)
        .await?;
      runner.wait_for_render().await?;

      assert!(runner.output().await?.contains("Username:      "));
      Ok(())
    }
  });

  runner.join_until_end(events).await
}
