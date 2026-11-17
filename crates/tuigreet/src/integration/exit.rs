use std::io;

use crossterm::event::{KeyCode, KeyModifiers};
use tokio::task::spawn;

use super::common::{IntegrationRunner, SessionOptions};

#[tokio::test]
async fn exit() -> io::Result<()> {
  let opts = SessionOptions {
    username: "apognu".to_string(),
    password: "password".to_string(),
    mfa:      false,
  };

  let runner = IntegrationRunner::new(opts, None)?;

  let events = spawn({
    let runner = runner.clone();

    async move {
      runner
        .send_modified_key(KeyCode::Char('x'), KeyModifiers::CONTROL)
        .await?;
      Ok(())
    }
  });

  runner.join_until_client_exit(events).await
}
