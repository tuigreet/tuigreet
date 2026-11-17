use std::{fs, io};

use tempfile::tempdir;
use tokio::task::spawn;
use tuigreet_config::Config;

use super::common::{IntegrationRunner, SessionOptions};
use crate::ui::sessions::SessionSource;

#[tokio::test]
async fn authentication_ok() -> io::Result<()> {
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
      runner.send_text("apognu").await?;
      runner.wait_until_buffer_contains("Password:").await?;
      runner.send_text("password").await?;
      Ok(())
    }
  });

  runner.join_until_client_exit(events).await
}

#[tokio::test]
async fn authentication_bad_password() -> io::Result<()> {
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
      runner.send_text("apognu").await?;
      runner.wait_until_buffer_contains("Password:").await?;
      runner.send_text("password2").await?;
      runner.wait_for_render().await?;

      assert!(runner.output().await?.contains("Authentication failed"));
      runner.wait_until_buffer_contains("Password:").await?;
      runner.send_text("password").await
    }
  });

  runner.join_until_client_exit(events).await
}

#[tokio::test]
async fn authentication_ok_mfa() -> io::Result<()> {
  let opts = SessionOptions {
    username: "apognu".to_string(),
    password: "password".to_string(),
    mfa:      true,
  };

  let runner = IntegrationRunner::new(opts, None)?;

  let events = spawn({
    let runner = runner.clone();

    async move {
      runner.wait_until_buffer_contains("Username:").await?;
      runner.send_text("apognu").await?;
      runner.wait_until_buffer_contains("Password:").await?;
      runner.send_text("password").await?;
      runner.wait_until_buffer_contains("7 + 2 =").await?;
      runner.send_text("9").await?;
      Ok(())
    }
  });

  runner.join_until_client_exit(events).await
}

#[tokio::test]
async fn authentication_bad_mfa() -> io::Result<()> {
  let opts = SessionOptions {
    username: "apognu".to_string(),
    password: "password".to_string(),
    mfa:      true,
  };

  let runner = IntegrationRunner::new(opts, None)?;

  let events = spawn({
    let runner = runner.clone();

    async move {
      runner.wait_until_buffer_contains("Username:").await?;
      runner.send_text("apognu").await?;
      runner.wait_until_buffer_contains("Password:").await?;
      runner.send_text("password").await?;
      runner.wait_until_buffer_contains("7 + 2 =   ").await?;
      runner.send_text("10").await?;
      runner.wait_for_render().await?;

      assert!(runner.output().await?.contains("Authentication failed"));
      assert!(runner.output().await?.contains("Password:"));
      Ok(())
    }
  });

  runner.join_until_end(events).await
}

#[tokio::test]
#[expect(
  clippy::expect_used,
  reason = "The infallible runner setup callback must abort on fixture I/O \
            failure"
)]
async fn authentication_after_session_config_reload() -> io::Result<()> {
  let opts = SessionOptions {
    username: "apognu".to_string(),
    password: "password".to_string(),
    mfa:      false,
  };
  let runner = IntegrationRunner::new(
    opts,
    Some(|greeter| {
      let directory = tempdir().expect("create session directory");
      fs::write(
        directory.path().join("session.desktop"),
        "[Desktop Entry]\nName=Reload session\nExec=true\n",
      )
      .expect("write desktop session");
      let mut config = Config::default();
      config.session.sessions_dirs =
        vec![directory.path().to_string_lossy().into_owned()];
      config.session.xsessions_dirs.clear();
      greeter.session_source = SessionSource::None;
      greeter.apply_config(config.clone());
      greeter.reload_sessions();

      config.display.show_time = true;
      greeter.apply_config(config);
      greeter.reload_sessions();
    }),
  )?;
  let events = spawn({
    let runner = runner.clone();
    async move {
      runner.wait_until_buffer_contains("Username:").await?;
      runner.send_text("apognu").await?;
      runner.wait_until_buffer_contains("Password:").await?;
      runner.send_text("password").await
    }
  });

  runner.join_until_client_exit(events).await
}
