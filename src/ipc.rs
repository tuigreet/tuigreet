use std::{borrow::Cow, error::Error, sync::Arc};

use greetd_ipc::{codec::TokioCodec, AuthMessageType, ErrorType, Request, Response};
use tokio::sync::{
  mpsc::{Receiver, Sender},
  Mutex, RwLock,
};

use crate::{
  event::Event,
  info::{delete_last_user_command, delete_last_user_session, write_last_user_command, write_last_user_session, write_last_username},
  macros::SafeDebug,
  ui::sessions::{Session, SessionSource, SessionType},
  AuthStatus, Greeter, Mode,
};

#[derive(Clone)]
pub struct Ipc(Arc<IpcHandle>);

pub struct IpcHandle {
  tx: RwLock<Sender<Request>>,
  rx: Mutex<Receiver<Request>>,
}

impl Ipc {
  pub fn new() -> Ipc {
    let (tx, rx) = tokio::sync::mpsc::channel::<Request>(10);

    Ipc(Arc::new(IpcHandle {
      tx: RwLock::new(tx),
      rx: Mutex::new(rx),
    }))
  }

  pub async fn send(&self, request: Request) {
    tracing::info!("sending request to greetd: {}", request.safe_repr());

    let _ = self.0.tx.read().await.send(request).await;
  }

  pub async fn next(&mut self) -> Option<Request> {
    self.0.rx.lock().await.recv().await
  }

  pub async fn handle(&mut self, greeter: Arc<RwLock<Greeter>>) -> Result<(), Box<dyn Error>> {
    let request = self.next().await;

    if let Some(request) = request {
      let stream = {
        let greeter = greeter.read().await;

        greeter.stream.as_ref().unwrap().clone()
      };

      let response = {
        request.write_to(&mut *stream.write().await).await?;

        let response = Response::read_from(&mut *stream.write().await).await?;

        greeter.write().await.working = false;

        response
      };

      self.parse_response(&mut *greeter.write().await, response).await?;
    }

    Ok(())
  }

  async fn parse_response(&mut self, greeter: &mut Greeter, response: Response) -> Result<(), Box<dyn Error>> {
    // Do not display actual message from greetd, which may contain entered information, sometimes passwords.
    match response {
      Response::Error { ref error_type, .. } => tracing::info!("received greetd error message: {error_type:?}"),
      ref response => tracing::info!("received greetd message: {:?}", response),
    }

    match response {
      Response::AuthMessage { auth_message_type, auth_message } => match auth_message_type {
        AuthMessageType::Secret => {
          greeter.mode = Mode::Password;
          greeter.working = false;
          greeter.asking_for_secret = true;
          greeter.set_prompt(&auth_message);
        }

        AuthMessageType::Visible => {
          greeter.mode = Mode::Password;
          greeter.working = false;
          greeter.asking_for_secret = false;
          greeter.set_prompt(&auth_message);
        }

        AuthMessageType::Error => {
          greeter.message = Some(auth_message);

          self.send(Request::PostAuthMessageResponse { response: None }).await;
        }

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

          self.send(Request::PostAuthMessageResponse { response: None }).await;
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
                }

                SessionSource::Session(index) => {
                  if let Some(Session { path: Some(session_path), .. }) = greeter.sessions.options.get(index) {
                    tracing::info!("caching last user session: {session_path:?}");

                    write_last_user_session(&greeter.username.value, session_path);
                    delete_last_user_command(&greeter.username.value);
                  }
                }

                _ => {}
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
              Ipc::cancel(greeter).await;

              greeter.message = Some(fl!("command_missing"));
              greeter.reset(false).await;
            }

            Some(command) if command.is_empty() => {
              Ipc::cancel(greeter).await;

              greeter.message = Some(fl!("command_missing"));
              greeter.reset(false).await;
            }

            Some(command) => {
              greeter.done = true;
              greeter.mode = Mode::Processing;

              let session = Session::get_selected(greeter);
              let default = DefaultCommand(&command, greeter.session_source.env());
              let (command, env) = wrap_session_command(greeter, session, &default);

              #[cfg(not(debug_assertions))]
              self.send(Request::StartSession { cmd: vec![command.to_string()], env }).await;

              #[cfg(debug_assertions)]
              {
                let _ = command;

                self.send(Request::StartSession { cmd: vec!["true".to_string()], env }).await;
              }
            }
          }
        }
      }

      Response::Error { error_type, .. } => {
        // Do not display actual message from greetd, which may contain entered information, sometimes passwords.
        tracing::info!("received an error from greetd: {error_type:?}");

        Ipc::cancel(greeter).await;

        match error_type {
          ErrorType::AuthError => {
            greeter.message = Some(fl!("failed"));
            self
              .send(Request::CreateSession {
                username: greeter.username.value.clone(),
              })
              .await;
            greeter.reset(true).await;
          }

          ErrorType::Error => {
            // Do not display actual message from greetd, which may contain entered information, sometimes passwords.
            greeter.message = Some("An error was received from greetd".to_string());
            greeter.reset(false).await;
          }
        }
      }
    }

    Ok(())
  }

  pub async fn cancel(greeter: &mut Greeter) {
    tracing::info!("cancelling session");

    let _ = Request::CancelSession.write_to(&mut *greeter.stream().await).await;
  }
}

fn desktop_names_to_xdg(names: &str) -> String {
  names.replace(';', ":").trim_end_matches(':').to_string()
}

struct DefaultCommand<'a>(&'a str, Option<Vec<String>>);

impl<'a> DefaultCommand<'a> {
  fn command(&'a self) -> &'a str {
    self.0
  }

  fn env(&'a self) -> Option<&'a Vec<String>> {
    self.1.as_ref()
  }
}

fn wrap_session_command<'a>(greeter: &Greeter, session: Option<&Session>, default: &'a DefaultCommand<'a>) -> (Cow<'a, str>, Vec<String>) {
  let mut env: Vec<String> = vec![];

  let mut command = Cow::Borrowed(default.command());

  match session {
    // If the target is a defined session, we should be able to deduce all the
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
        env.push(format!("XDG_SESSION_TYPE={}", session_type.as_xdg_session_type()));
      }
      if let Some(xdg_desktop_names) = xdg_desktop_names {
        env.push(format!("XDG_CURRENT_DESKTOP={}", desktop_names_to_xdg(xdg_desktop_names)));
      }

      if *session_type == SessionType::X11 {
        if let Some(ref wrap) = greeter.xsession_wrapper {
          command = Cow::Owned(format!("{} {}", wrap, default.command()));
        }
      } else if let Some(ref wrap) = greeter.session_wrapper {
        command = Cow::Owned(format!("{} {}", wrap, default.command()));
      }
    }

    _ => {
      // If a wrapper script is used, assume that it is able to set up the
      // required environment.
      if let Some(ref wrap) = greeter.session_wrapper {
        command = Cow::Owned(format!("{} {}", wrap, default.command()));
      }
      // Otherwise, set up the environment from the provided argument.
      if let Some(base_env) = default.env() {
        env.append(&mut base_env.clone());
      }
    }
  }

  if greeter.silent {
    (Cow::Owned(format!("{command} >/dev/null 2>&1")), env)
  } else {
    (command, env)
  }
}

#[cfg(test)]
mod test {
  use std::path::PathBuf;

  use crate::{
    ipc::{desktop_names_to_xdg, DefaultCommand},
    ui::sessions::{Session, SessionType},
    Greeter,
  };

  use super::wrap_session_command;

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
    let (command, env) = wrap_session_command(&greeter, Some(&session), &default);

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
    let (command, env) = wrap_session_command(&greeter, Some(&session), &default);

    assert_eq!(command.as_ref(), "/wrapper.sh Session1Cmd");
    assert_eq!(env, vec!["XDG_SESSION_TYPE=wayland"]);
  }

  #[test]
  fn x11_wrapper() {
    let mut greeter = Greeter::default();
    greeter.xsession_wrapper = Some("startx /usr/bin/env".into());

    let session = Session {
      slug: Some("thede".to_string()),
      name: "Session1".into(),
      session_type: SessionType::X11,
      command: "Session1Cmd".into(),
      path: Some(PathBuf::from("/Session1Path")),
      xdg_desktop_names: Some("one;two;three;".to_string()),
      ..Default::default()
    };

    let default = DefaultCommand(&session.command, None);
    let (command, env) = wrap_session_command(&greeter, Some(&session), &default);

    assert_eq!(command.as_ref(), "startx /usr/bin/env Session1Cmd");
    assert_eq!(
      env,
      vec!["XDG_SESSION_DESKTOP=thede", "DESKTOP_SESSION=thede", "XDG_SESSION_TYPE=x11", "XDG_CURRENT_DESKTOP=one:two:three"]
    );
  }

  #[test]
  fn xdg_current_desktop() {
    assert_eq!(desktop_names_to_xdg("one;two;three four"), "one:two:three four");
    assert_eq!(desktop_names_to_xdg("one;"), "one");
    assert_eq!(desktop_names_to_xdg(""), "");
    assert_eq!(desktop_names_to_xdg(";"), "");
  }
}
