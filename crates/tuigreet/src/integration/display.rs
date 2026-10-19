use std::io;

use jiff::civil::DateTime;
use libgreetd_stub::SessionOptions;
use tokio::task::spawn;

use super::common::IntegrationRunner;

#[tokio::test]
async fn show_greet() -> io::Result<()> {
  let opts = SessionOptions {
    username: "apognu".to_string(),
    password: "password".to_string(),
    mfa:      false,
  };

  let runner = IntegrationRunner::new(
    opts,
    Some(|greeter| {
      greeter.greeting = Some("Lorem ipsum dolor sit amet".to_string());
    }),
  )?;

  let events = spawn({
    let runner = runner.clone();

    async move {
      runner.wait_for_render().await?;

      assert!(
        runner
          .output()
          .await?
          .contains("Lorem ipsum dolor sit amet")
      );
      Ok(())
    }
  });

  runner.join_until_end(events).await
}

#[tokio::test]
async fn show_wrapped_greet() -> io::Result<()> {
  let opts = SessionOptions {
    username: "apognu".to_string(),
    password: "password".to_string(),
    mfa:      false,
  };

  let runner = IntegrationRunner::new_with_size(
    opts,
    Some(|greeter| {
      greeter.greeting = Some("Lorem \x1b[31mipsum dolor sit amet".to_string());
    }),
    (20, 20),
  )?;

  let events = spawn({
    let runner = runner.clone();

    async move {
      runner.wait_for_render().await?;

      let output = runner.output().await?;

      assert!(output.contains("┌ Authenticate into┐"), "{:?}", output.0);
      assert!(output.contains("│    Lorem ipsum   │"));
      assert!(output.contains("│  dolor sit amet  │"));
      assert!(output.contains("└──────────────────┘"));
      Ok(())
    }
  });

  runner.join_until_end(events).await
}

const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";

#[tokio::test]
async fn show_time() -> io::Result<()> {
  let opts = SessionOptions {
    username: "apognu".to_string(),
    password: "password".to_string(),
    mfa:      false,
  };

  let runner = IntegrationRunner::new(
    opts,
    Some(|greeter| {
      greeter.time = true;
      greeter.time_format = Some(TIME_FORMAT.to_string());
    }),
  )?;

  let events = spawn({
    let runner = runner.clone();

    async move {
      runner.wait_for_render().await?;

      let initial = runner.output().await?;
      let timestamp = initial
        .split_whitespace()
        .find(|word| DateTime::strptime(TIME_FORMAT, word).is_ok())
        .ok_or_else(|| {
          io::Error::other("rendered clock did not contain a timestamp")
        })?;

      loop {
        runner.wait_for_render().await?;
        let rendered = runner.output().await?;
        if !rendered.contains(timestamp) {
          assert!(
            rendered
              .split_whitespace()
              .any(|word| DateTime::strptime(TIME_FORMAT, word).is_ok())
          );
          break;
        }
      }
      Ok(())
    }
  });

  runner.join_until_end(events).await
}

#[tokio::test]
async fn show_custom_title_overrides_localization() -> io::Result<()> {
  let opts = SessionOptions {
    username: "apognu".to_string(),
    password: "password".to_string(),
    mfa:      false,
  };

  let runner = IntegrationRunner::new_with_size(
    opts,
    Some(|greeter| {
      greeter.title.enable = true;
      greeter.title.custom = Some("Welcome to My System".to_string());
    }),
    (30, 10),
  )?;

  let events = spawn({
    let runner = runner.clone();

    async move {
      runner.wait_for_render().await?;

      let output = runner.output().await?;

      // Custom title should appear, not the localized default
      assert!(
        output.contains("Welcome to My System"),
        "Custom title should override localization. Output:\n{}",
        output.0
      );

      // Default localized title should NOT appear
      assert!(
        !output.contains("Authenticate into"),
        "Default localized title should not appear. Output:\n{}",
        output.0
      );
      Ok(())
    }
  });

  runner.join_until_end(events).await
}
