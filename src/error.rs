use std::fmt;

#[derive(Debug)]
pub enum CacheError {
    Serde(String),
    Redis(String),
    Consistency(String),
    DurationOutOfRange,
}

impl From<redis::RedisError> for CacheError {
    fn from(value: redis::RedisError) -> Self {
        Self::Redis(value.to_string())
    }
}

impl From<serde_json::Error> for CacheError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serde(value.to_string())
    }
}

impl std::error::Error for CacheError {}

impl fmt::Display for CacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Serde(ref msg) => write!(f, "serde error: {msg}"),
            Self::Redis(ref msg) => write!(f, "redis error: {msg}"),
            Self::Consistency(ref msg) => write!(f, "consistency error: {msg}"),
            Self::DurationOutOfRange => write!(f, "duration is out of range"),
        }
    }
}
