//! The single wire contract shared by the Saccade Extension, Runtime, and MCP mode.

#![deny(unsafe_code)]

mod action;
mod observation;
mod transport;

pub use action::*;
pub use observation::*;
pub use transport::*;

pub const OBSERVATION_SCHEMA: &str = "saccade.observation/1";
pub const HOST_PROTOCOL: &str = "saccade-extension-host/1";
pub const SESSION_CAPABILITY_SCHEME: &str = "saccade_session_bearer_v1";
