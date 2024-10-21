use std::{
    fmt,
    net::{IpAddr, Ipv4Addr},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tracing::Level;

use objcache::{CacheError, Client, RedisCacheArgs, RedisClient};

#[derive(Debug, Serialize, Deserialize)]
struct Item {
    val: usize,
}

#[derive(Debug)]
struct Error(String);

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<CacheError> for Error {
    fn from(value: CacheError) -> Self {
        Self(format!("cache error: {value}"))
    }
}

async fn get_item(v: usize) -> Result<Item, Error> {
    Ok(Item { val: v })
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::Subscriber::builder()
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL)
        .with_ansi(true)
        .pretty()
        .with_max_level(Level::TRACE)
        .try_init()
        .unwrap();
    let client = RedisClient::new((IpAddr::V4(Ipv4Addr::LOCALHOST), 6379)).unwrap();

    let item = client.wrap(
        get_item,
        &RedisCacheArgs {
            lock_name: b"item_lock",
            key_name: b"item",
            expiry: Duration::from_secs(5),
        },
    )(100)
    .await?;

    println!("{item:?}");

    let item = client.wrap(
        |v| Box::pin(async move { Ok::<_, Error>(Item { val: v }) }),
        &RedisCacheArgs {
            lock_name: b"item_lock",
            key_name: b"item",
            expiry: Duration::from_secs(5),
        },
    )(100)
    .await?;

    println!("{item:?}");

    Ok(())
}
