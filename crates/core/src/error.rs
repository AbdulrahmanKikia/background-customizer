use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    Io(String),
    Parse(String),
    Invalid(String),
}

impl fmt::Display for CoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(formatter, "io: {message}"),
            Self::Parse(message) => write!(formatter, "parse: {message}"),
            Self::Invalid(message) => write!(formatter, "invalid: {message}"),
        }
    }
}

impl std::error::Error for CoreError {}
