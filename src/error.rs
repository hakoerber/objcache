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

impl<T: fmt::Debug> From<ciborium::ser::Error<T>> for CacheError {
    fn from(value: ciborium::ser::Error<T>) -> Self {
        Self::Serde(value.to_string())
    }
}

impl<T: fmt::Debug> From<ciborium::de::Error<T>> for CacheError {
    fn from(value: ciborium::de::Error<T>) -> Self {
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
