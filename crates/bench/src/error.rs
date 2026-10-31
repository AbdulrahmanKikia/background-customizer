use std::fmt;

#[derive(Debug)]
pub enum BenchError {
    Message(String),
    Io(String),
    Parse(String),
}

impl fmt::Display for BenchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(message) => formatter.write_str(message),
            Self::Io(message) => write!(formatter, "io: {message}"),
            Self::Parse(message) => write!(formatter, "parse: {message}"),
        }
    }
}

impl std::error::Error for BenchError {}

impl From<std::io::Error> for BenchError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl From<serde_json::Error> for BenchError {
    fn from(error: serde_json::Error) -> Self {
        Self::Parse(error.to_string())
    }
}
