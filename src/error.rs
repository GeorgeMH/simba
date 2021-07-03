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

    #[error("SerDe Yaml Error {source}")]
    SerDeYamlError {
        #[from]
        source: serde_yaml::Error,
    },

    #[error("SerDe JSON Error {source}")]
    SerDeJsonError {
        #[from]
        source: serde_json::Error,
    },

    #[error("HTTP Error {source}")]
    HttpError {
        #[from]
        source: reqwest::Error,
    },

    #[error("Invalid header name: {source}")]
    InvalidHeaderName {
        #[from]
        source: InvalidHeaderName,
    },

    #[error("Invalid header value: {source}")]
    InvalidHeaderValue {
        #[from]
        source: InvalidHeaderValue,
    },

    #[error("IO Error {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },

    #[error("From Utf8 Error {source}")]
    FromUtf8Error {
        #[from]
        source: std::string::FromUtf8Error,
    },

    #[error("Error setting logger: {source}")]
    SetLoggerError {
        #[from]
        source: log::SetLoggerError,
    },

    #[error("Log configuration error: {source}")]
    ConfigErrors {
        #[from]
        source: log4rs::config::runtime::ConfigErrors,
    },

    #[error("Other Error: {0}")]
    Other(String),
}
