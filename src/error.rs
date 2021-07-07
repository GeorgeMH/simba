use crate::script::lua::LuaEvent;
use reqwest::header::{InvalidHeaderName, InvalidHeaderValue};
use rlua::Error as LuaError;
use thiserror::Error;

pub type SimbaResult<T> = Result<T, SimbaError>;

#[derive(Error, Debug)]
pub enum SimbaError {
    #[error(transparent)]
    LuaError {
        #[from]
        source: LuaError,
    },

    #[error(transparent)]
    SerDeYamlError {
        #[from]
        source: serde_yaml::Error,
    },

    #[error(transparent)]
    SerDeJsonError {
        #[from]
        source: serde_json::Error,
    },

    #[error(transparent)]
    HttpError {
        #[from]
        source: reqwest::Error,
    },

    #[error(transparent)]
    InvalidHeaderName {
        #[from]
        source: InvalidHeaderName,
    },

    #[error(transparent)]
    InvalidHeaderValue {
        #[from]
        source: InvalidHeaderValue,
    },

    #[error(transparent)]
    IoError {
        #[from]
        source: std::io::Error,
    },

    #[error(transparent)]
    FromUtf8Error {
        #[from]
        source: std::string::FromUtf8Error,
    },

    #[error(transparent)]
    SetLoggerError {
        #[from]
        source: log::SetLoggerError,
    },

    #[error(transparent)]
    ConfigErrors {
        #[from]
        source: log4rs::config::runtime::ConfigErrors,
    },

    #[error(transparent)]
    TokioOneShotReceiveError {
        #[from]
        source: tokio::sync::oneshot::error::RecvError,
    },

    #[error(transparent)]
    StdMpscReceiveError {
        #[from]
        source: std::sync::mpsc::RecvError,
    },

    #[error(transparent)]
    StdMpscSendError {
        #[from]
        source: std::sync::mpsc::SendError<LuaEvent>,
    },

    #[error(transparent)]
    JoinError {
        #[from]
        source: tokio::task::JoinError,
    },

    #[error(transparent)]
    RenderError {
        #[from]
        source: handlebars::RenderError,
    },

    #[error("Other Error: {0}")]
    Other(String),
}
