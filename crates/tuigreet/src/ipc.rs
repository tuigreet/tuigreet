use std::{borrow::Cow, error::Error, sync::Arc};

use greetd_ipc::{
  AuthMessageType,
  ErrorType,
  Request,
  Response,
  codec::TokioCodec,
};
use tokio::sync::{
  Mutex,
  RwLock,
  mpsc::{Receiver, Sender},
};
use tuigreet_types::{AuthStatus, Mode};

use crate::{
  Greeter,
  event::Event,
  info::{
    delete_last_user_command,
    delete_last_user_session,
    write_last_user_command,
    write_last_user_session,
    write_last_username,
  },
  macros::SafeDebug,
  ui::sessions::{Session, SessionSource, SessionType},
};

/// IPC client for communicating with greetd.
#[derive(Clone)]
pub struct Ipc(Arc<IpcHandle>);

/// Internal IPC channel handle.
pub struct IpcHandle {
  tx: RwLock<Sender<Request>>,
  rx: Mutex<Receiver<Request>>,
}

impl Ipc {
  /// Create a new IPC client.
  pub fn new() -> Self {
    let (tx, rx) = tokio::sync::mpsc::channel::<Request>(10);

    Self(Arc::new(IpcHandle {
      tx: RwLock::new(tx),
      rx: Mutex::new(rx),
    }))
  }

  /// Send a request to greetd.
  pub async fn send(&self, request: Request) {
    tracing::info!("sending request to greetd: {}", request.safe_repr());

    if let Err(e) = self.0.tx.read().await.send(request).await {
      tracing::warn!(
        "failed to send request to greetd IPC channel: {} (request: {})",
        e,
        e.0.safe_repr()
      );
    }
  }

  /// Receive next request from the queue.
  pub async fn next(&mut self) -> Option<Request> {
    self.0.rx.lock().await.recv().await
  }

  /// Handle IPC communication with greetd daemon.
  ///
  /// Processes authentication flow and session management.
  pub async fn handle(
    &mut self,
    greeter: Arc<RwLock<Greeter>>,
  ) -> Result<(), Box<dyn Error>> {
    let request = self.next().await;

    if let Some(request) = request {
      let stream = {
        let greeter = greeter.read().await;
        greeter.stream.as_ref().map(Arc::clone)
      };

      let response = if let Some(stream) = stream {
        let mut stream_guard = stream.write().await;
        request.write_to(&mut *stream_guard).await?;
        let response = Response::read_from(&mut *stream_guard).await?;
        drop(stream_guard);

        greeter.write().await.working = false;

        response
      } else {
        // Mock mode is on, emulate a plausible response
        greeter.write().await.working = false;
        mock_response(&request)
      };

      self
        .parse_response(&mut *greeter.write().await, response)
        .await?;
    }

    Ok(())
  }

  async fn parse_response(
    &mut self,
    greeter: &mut Greeter,
    response: Response,
  ) -> Result<(), Box<dyn Error>> {
    // XXX: Do not display actual message from greetd, which may contain entered
    // information, sometimes passwords.
    match response {
      Response::Error { ref error_type, .. } => {
        tracing::info!("received greetd error message: {error_type:?}");
      },
      ref response => tracing::info!("received greetd message: {:?}", response),
    }

    match response {
      Response::AuthMessage {
        auth_message_type,
        auth_message,
      } => {
        match auth_message_type {
          AuthMessageType::Secret => {
            greeter.mode = Mode::Password;
            greeter.working = false;
            greeter.asking_for_secret = true;
            greeter.set_prompt(&auth_message);
          },

          AuthMessageType::Visible => {
            greeter.mode = Mode::Password;
            greeter.working = false;
            greeter.asking_for_secret = false;
            greeter.set_prompt(&auth_message);
          },

          AuthMessageType::Error => {
            greeter.message = Some(auth_message);

            self
              .send(Request::PostAuthMessageResponse { response: None })
              .await;
          },

          AuthMessageType::Info => {
            greeter.remove_prompt();

            greeter.previous_mode = greeter.mode;
            greeter.mode = Mode::Action;

            if let Some(message) = &mut greeter.message {
              message.push('\n');
              message.push_str(auth_message.trim_end());
            } else {
              greeter.message = Some(auth_message.trim_end().to_string());
            }

            self
              .send(Request::PostAuthMessageResponse { response: None })
              .await;
          },
        }
      },

      Response::Success => {
        if greeter.done {
          tracing::info!("greetd acknowledged session start, exiting");

          if greeter.remember {
            tracing::info!("caching last successful username");

            write_last_username(&greeter.username);

            if greeter.remember_user_session {
              match greeter.session_source {
                SessionSource::Command(ref command) => {
                  tracing::info!("caching last user command: {command}");

                  write_last_user_command(&greeter.username.value, command);
                  delete_last_user_session(&greeter.username.value);
                },

                SessionSource::Session(index) => {
                  if let Some(Session {
                    path: Some(session_path),
                    ..
                  }) = greeter.sessions.options.get(index)
                  {
                    tracing::info!(
                      "caching last user session: {session_path:?}"
                    );

                    write_last_user_session(
                      &greeter.username.value,
                      session_path,
                    );
                    delete_last_user_command(&greeter.username.value);
                  }
                },

                _ => {},
              }
            }
          }

          if let Some(ref sender) = greeter.events {
            let _ = sender.send(Event::Exit(AuthStatus::Success)).await;
          }
        } else {
          tracing::info!("authentication successful, starting session");

          match greeter.session_source.command(greeter).map(str::to_string) {
            None => {
              Self::cancel(greeter).await;

              greeter.message = Some(fl!("command_missing"));
              greeter.reset(false).await;
            },

            Some(command) if command.is_empty() => {
              Self::cancel(greeter).await;

              greeter.message = Some(fl!("command_missing"));
              greeter.reset(false).await;
            },

            Some(command) => {
              greeter.done = true;
              greeter.mode = Mode::Processing;

              let session = Session::get_selected(greeter);
              let default =
                DefaultCommand(&command, greeter.session_source.env());
              let (command, env) =
                wrap_session_command(greeter, session, &default);

              #[cfg(not(debug_assertions))]
              self
                .send(Request::StartSession {
                  cmd: vec![command.to_string()],
                  env,
                })
                .await;

              #[cfg(debug_assertions)]
              {
                let _ = command;

                self
                  .send(Request::StartSession {
                    cmd: vec!["true".to_string()],
                    env,
                  })
                  .await;
              }
            },
          }
        }
      },

      Response::Error { error_type, .. } => {
        // XXX: Do not display actual message from greetd, which may contain
        // entered information, sometimes passwords.
        tracing::info!("received an error from greetd: {error_type:?}");

        Self::cancel(greeter).await;

        match error_type {
          ErrorType::AuthError => {
            greeter.message = Some(fl!("failed"));
            self
              .send(Request::CreateSession {
                username: greeter.username.value.clone(),
              })
              .await;
            greeter.reset(true).await;
          },

          ErrorType::Error => {
            // XXX: Do not display actual message from greetd, which may contain
            // entered information, sometimes passwords.
            greeter.message =
              Some("An error was received from greetd".to_string());
            greeter.reset(false).await;
          },
        }
      },
    }

    Ok(())
  }

  pub async fn cancel(greeter: &mut Greeter) {
    tracing::info!("cancelling session");

    if greeter.stream.is_none() {
      return;
    }

    let _ = Request::CancelSession
      .write_to(&mut *greeter.stream().await)
      .await;
  }
}

// Mocks regular greetd behaviour for UI tests
fn mock_response(request: &Request) -> Response {
  match request {
    Request::CreateSession { .. } => {
      Response::AuthMessage {
        auth_message_type: AuthMessageType::Secret,
        auth_message:      "Password: ".to_string(),
      }
    },
    Request::PostAuthMessageResponse { .. } | Request::StartSession { .. } => {
      Response::Success
    },
    Request::CancelSession => Response::Success,
  }
}

fn desktop_names_to_xdg(names: &str) -> String {
  names.replace(';', ":").trim_end_matches(':').to_string()
}

struct DefaultCommand<'a>(&'a str, Option<Vec<String>>);

impl<'a> DefaultCommand<'a> {
  const fn command(&'a self) -> &'a str {
    self.0
  }

  const fn env(&'a self) -> Option<&'a Vec<String>> {
    self.1.as_ref()
  }
}

fn wrap_session_command<'a>(
  greeter: &Greeter,
  session: Option<&Session>,
  default: &'a DefaultCommand<'a>,
) -> (Cow<'a, str>, Vec<String>) {
  let mut env: Vec<String> = vec![];

  match session {
    // If the target is a defined session, we *should* be able to deduce all the
    // environment we need from the desktop file.
    Some(Session {
      slug,
      session_type,
      xdg_desktop_names,
      ..
    }) => {
      if let Some(slug) = slug {
        env.push(format!("XDG_SESSION_DESKTOP={slug}"));
        env.push(format!("DESKTOP_SESSION={slug}"));
      }
      if *session_type != SessionType::None {
        env.push(format!(
          "XDG_SESSION_TYPE={}",
          session_type.as_xdg_session_type()
        ));
      }
      if let Some(xdg_desktop_names) = xdg_desktop_names {
        env.push(format!(
          "XDG_CURRENT_DESKTOP={}",
          desktop_names_to_xdg(xdg_desktop_names)
        ));
      }

      if *session_type == SessionType::X11 {
        if let Some(ref wrap) = greeter.xsession_wrapper {
          return (Cow::Owned(format!("{} {}", wrap, default.command())), env);
        }
      } else if let Some(ref wrap) = greeter.session_wrapper {
        return (Cow::Owned(format!("{} {}", wrap, default.command())), env);
      }
    },

    _ => {
      // If a wrapper script is used, assume that it is able to set up the
      // required environment.
      if let Some(ref wrap) = greeter.session_wrapper {
        return (Cow::Owned(format!("{} {}", wrap, default.command())), env);
      }
      // Otherwise, set up the environment from the provided argument.
      if let Some(base_env) = default.env() {
        env.append(&mut base_env.clone());
      }
    },
  }

  (Cow::Borrowed(default.command()), env)
}

#[cfg(test)]
mod test {
  use std::path::PathBuf;

  use super::wrap_session_command;
  use crate::{
    Greeter,
    ipc::{DefaultCommand, desktop_names_to_xdg},
    ui::sessions::{Session, SessionType},
  };

  #[test]
  fn wayland_no_wrapper() {
    let greeter = Greeter::default();

    let session = Session {
      name: "Session1".into(),
      session_type: SessionType::Wayland,
      command: "Session1Cmd".into(),
      path: Some(PathBuf::from("/Session1Path")),
      ..Default::default()
    };

    let default = DefaultCommand(&session.command, None);
    let (command, env) =
      wrap_session_command(&greeter, Some(&session), &default);

    assert_eq!(command.as_ref(), "Session1Cmd");
    assert_eq!(env, vec!["XDG_SESSION_TYPE=wayland"]);
  }

  #[test]
  fn wayland_wrapper() {
    let mut greeter = Greeter::default();
    greeter.session_wrapper = Some("/wrapper.sh".into());

    let session = Session {
      name: "Session1".into(),
      session_type: SessionType::Wayland,
      command: "Session1Cmd".into(),
      path: Some(PathBuf::from("/Session1Path")),
      ..Default::default()
    };

    let default = DefaultCommand(&session.command, None);
    let (command, env) =
      wrap_session_command(&greeter, Some(&session), &default);

    assert_eq!(command.as_ref(), "/wrapper.sh Session1Cmd");
    assert_eq!(env, vec!["XDG_SESSION_TYPE=wayland"]);
  }

  #[test]
  fn x11_wrapper() {
    let mut greeter = Greeter::default();
    greeter.xsession_wrapper = Some("startx".into());

    let session = Session {
      slug:              Some("thede".to_string()),
      name:              "Session1".into(),
      session_type:      SessionType::X11,
      command:           "Session1Cmd".into(),
      path:              Some(PathBuf::from("/Session1Path")),
      xdg_desktop_names: Some("one;two;three;".to_string()),
    };

    let default = DefaultCommand(&session.command, None);
    let (command, env) =
      wrap_session_command(&greeter, Some(&session), &default);

    assert_eq!(command.as_ref(), "startx Session1Cmd");
    assert_eq!(env, vec![
      "XDG_SESSION_DESKTOP=thede",
      "DESKTOP_SESSION=thede",
      "XDG_SESSION_TYPE=x11",
      "XDG_CURRENT_DESKTOP=one:two:three"
    ]);
  }

  #[test]
  fn xdg_current_desktop() {
    assert_eq!(
      desktop_names_to_xdg("one;two;three four"),
      "one:two:three four"
    );
    assert_eq!(desktop_names_to_xdg("one;"), "one");
    assert_eq!(desktop_names_to_xdg(""), "");
    assert_eq!(desktop_names_to_xdg(";"), "");
  }
}
