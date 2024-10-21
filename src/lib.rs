use std::{future::Future, net, pin::Pin, time::Duration};

use chrono::{DateTime, TimeDelta, Utc};
use redis::AsyncCommands;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::time;
use tracing::Level;

mod error;
pub use error::CacheError;

#[derive(Clone)]
pub struct RedisClient {
    redis: redis::Client,
}

pub struct RedisCacheArgs<'a> {
    pub lock_name: &'a [u8],
    pub key_name: &'a [u8],
    pub expiry: Duration,
}

pub trait Client<T>
where
    Self: Sized,
{
    type ConnArgs;

    fn new(args: Self::ConnArgs) -> Result<Self, CacheError>;

    fn get(&self) -> &T;

    fn wrap<'c, 'f, 'a, Func, Inner, Args, Item, E>(
        &'c self,
        f: Func,
        cache_args: &'a RedisCacheArgs<'a>,
    ) -> Box<dyn FnOnce(Args) -> Pin<Box<dyn Future<Output = Result<Item, E>> + Send + 'f>> + 'f>
    where
        Func: Fn(Args) -> Inner + Sync + Send + 'f,
        Inner: Future<Output = Result<Item, E>> + Send + 'f,
        Args: Send + 'f,
        Item: Send + Serialize + DeserializeOwned,
        E: From<CacheError>,
        'a: 'f,
        'c: 'f;
}

impl Client<redis::Client> for RedisClient {
    type ConnArgs = (net::IpAddr, u16);

    fn new((ip, port): (net::IpAddr, u16)) -> Result<Self, CacheError> {
        Ok(Self {
            redis: redis::Client::open((ip.to_string(), port))?,
        })
    }

    fn get(&self) -> &redis::Client {
        &self.redis
    }

    fn wrap<'c, 'f, 'a, Func, Inner, Args, Item, E>(
        &'c self,
        f: Func,
        cache_args: &'a RedisCacheArgs<'a>,
    ) -> Box<dyn FnOnce(Args) -> Pin<Box<dyn Future<Output = Result<Item, E>> + Send + 'f>> + 'f>
    where
        Func: Fn(Args) -> Inner + Sync + Send + 'f,
        Inner: Future<Output = Result<Item, E>> + Send + 'f,
        Args: Send + 'f,
        Item: Send + Serialize + DeserializeOwned,
        E: From<CacheError>,
        'a: 'f,
        'c: 'f,
    {
        const LOCK_TTL: usize = 60_000; // milliseconds
        const LOCK_RETRY_TIME: Duration = Duration::from_secs(1);

        #[derive(Serialize, Deserialize)]
        struct CacheItem<Item> {
            timestamp: DateTime<Utc>,
            payload: Item,
        }

        #[tracing::instrument(skip_all, level = Level::TRACE)]
        async fn query_cache<Item>(
            conn: &mut redis::aio::MultiplexedConnection,
            key_name: &[u8],
        ) -> Result<Option<CacheItem<Item>>, CacheError>
        where
            Item: DeserializeOwned,
        {
            Ok(conn
                .get::<&[u8], Option<String>>(key_name)
                .await?
                .map(|s| serde_json::from_str(&s))
                .transpose()?)
        }

        Box::new(move |args: Args| {
            Box::pin(async move {
                let expiry = TimeDelta::from_std(cache_args.expiry)
                    .map_err(|_e| CacheError::DurationOutOfRange)?;

                let mut conn = tracing::span!(Level::TRACE, "connection")
                    .in_scope(|| async {
                        self.get()
                            .get_multiplexed_async_connection()
                            .await
                            .map_err(Into::into)
                    })
                    .await?;

                match query_cache::<Item>(&mut conn, cache_args.key_name).await? {
                    Some(response)
                        if Utc::now().signed_duration_since(response.timestamp) <= expiry =>
                    {
                        tracing::trace!(
                            "found valid cache entry (timestamp: {})",
                            response.timestamp
                        );
                        Ok(response.payload)
                    }
                    _ => {
                        tracing::trace!("cache stale, regenerating response");
                        let lock_manager = redlock::RedLock::with_client(self.get().clone());

                        if let Some(lock) = lock_manager
                            .lock(cache_args.lock_name, LOCK_TTL)
                            .map_err(Into::into)?
                        {
                            tracing::trace!("acquired lock");
                            tracing::span!(Level::TRACE, "regenerate")
                                .in_scope(|| async {
                                    let payload = f(args).await?;

                                    let cache_item = CacheItem {
                                        timestamp: Utc::now(),
                                        payload,
                                    };
                                    let _: () = conn
                                        .set(
                                            cache_args.key_name,
                                            serde_json::to_string(&cache_item)
                                                .map_err(Into::into)?,
                                        )
                                        .await
                                        .map_err(Into::into)?;
                                    lock_manager.unlock(&lock);
                                    tracing::trace!("cache updated");
                                    Ok(cache_item.payload)
                                })
                                .await
                        } else {
                            // Could not get lock because it's already. so some other process is already
                            // gathering data. Wait for it to finish and then just return
                            // the cached response.
                            tracing::trace!("could not acquire lock");
                            tracing::span!(Level::TRACE, "wait_for_lock")
                                .in_scope(|| async {
                                    loop {
                                        tracing::trace!("waiting for unlock");
                                        time::sleep(LOCK_RETRY_TIME).await;
                                        if let Some(lock) = lock_manager
                                            .lock(cache_args.lock_name, LOCK_TTL)
                                            .map_err(Into::into)?
                                        {
                                            lock_manager.unlock(&lock);
                                            match query_cache::<Item>(
                                                &mut conn,
                                                cache_args.key_name,
                                            )
                                            .await?
                                            {
                                                Some(response) => {
                                                    if Utc::now()
                                                        .signed_duration_since(response.timestamp)
                                                        > expiry
                                                    {
                                                        break Err(CacheError::Consistency(
                                                    "newly generated item older than max cache time"
                                                        .to_owned(),
                                                )
                                                .into());
                                                    }
                                                    tracing::trace!("serving response from cache");
                                                    break Ok(response.payload);
                                                }
                                                None => {
                                                    break Err(CacheError::Consistency(
                                                        "cached item expected but not found"
                                                            .to_owned(),
                                                    )
                                                    .into())
                                                }
                                            }
                                        }
                                    }
                                })
                                .await
                        }
                    }
                }
            })
        })
    }
}
