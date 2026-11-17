use std::{io, path::Path};

use greetd_ipc::{
  AuthMessageType,
  ErrorType,
  Request,
  Response,
  codec::{Error as CodecError, TokioCodec},
};
use tokio::net::UnixListener;

pub struct SessionOptions {
  pub username: String,
  pub password: String,
  pub mfa:      bool,
}

enum State {
  Idle,
  Password { known_user: bool },
  Mfa,
  Authenticated,
}

pub async fn serve(socket: &Path, options: SessionOptions) -> io::Result<()> {
  let listener = UnixListener::bind(socket)?;
  loop {
    let (mut stream, _) = listener.accept().await?;
    let mut state = State::Idle;
    loop {
      let request = match Request::read_from(&mut stream).await {
        Ok(request) => request,
        Err(CodecError::Eof | CodecError::Io(_)) => break,
        Err(error) => return Err(io::Error::other(error.to_string())),
      };
      let response = respond(&mut state, request, &options);
      match response.write_to(&mut stream).await {
        Ok(()) => {},
        Err(CodecError::Eof | CodecError::Io(_)) => break,
        Err(error) => return Err(io::Error::other(error.to_string())),
      }
    }
  }
}

fn respond(
  state: &mut State,
  request: Request,
  options: &SessionOptions,
) -> Response {
  match request {
    Request::CreateSession { username } if matches!(state, State::Idle) => {
      *state = State::Password {
        known_user: username == options.username,
      };
      prompt(AuthMessageType::Secret, "Password:")
    },
    Request::PostAuthMessageResponse {
      response: Some(answer),
    } => {
      match state {
        State::Password { known_user }
          if *known_user && answer == options.password =>
        {
          if options.mfa {
            *state = State::Mfa;
            prompt(AuthMessageType::Visible, "7 + 2 =")
          } else {
            *state = State::Authenticated;
            Response::Success
          }
        },
        State::Mfa if answer == "9" => {
          *state = State::Authenticated;
          Response::Success
        },
        State::Password { .. } | State::Mfa => {
          *state = State::Idle;
          Response::Error {
            error_type:  ErrorType::AuthError,
            description: "Authentication rejected".to_string(),
          }
        },
        _ => protocol_error(state),
      }
    },
    Request::StartSession { .. } if matches!(state, State::Authenticated) => {
      Response::Success
    },
    Request::CancelSession => {
      *state = State::Idle;
      Response::Success
    },
    _ => protocol_error(state),
  }
}

fn prompt(auth_message_type: AuthMessageType, message: &str) -> Response {
  Response::AuthMessage {
    auth_message_type,
    auth_message: message.to_string(),
  }
}

fn protocol_error(state: &mut State) -> Response {
  *state = State::Idle;
  Response::Error {
    error_type:  ErrorType::Error,
    description: "Unexpected request for the current authentication state"
      .to_string(),
  }
}
