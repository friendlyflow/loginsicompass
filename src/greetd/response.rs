//! What greetd answers, on its own so `examples/fake-greetd.rs` can be greetd without
//! linking the client.

use serde::{Deserialize, Serialize};

/// Responses received from greetd.
///
/// `Serialize` is not needed to talk to greetd, only to *be* greetd: the
/// scripted server in [`crate::fakegreetd`] writes these on the wire, in tests
/// and when running the greeter with no PAM behind it.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Success,
    Error {
        error_type: ErrorType,
        description: String,
    },
    AuthMessage {
        auth_message_type: AuthMessageType,
        auth_message: String,
    },
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ErrorType {
    AuthError,
    Error,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum AuthMessageType {
    Visible,
    Secret,
    Info,
    Error,
}
