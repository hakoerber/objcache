# objcache

Cache serde-able objects with Redis.

## Quick overview

Imagine you have the following:

1) A struct or enum that implements `Serialize` and `Deserialize`

```rust
#[derive(Serialize, Deserialize)]
struct Item {
    value: usize,
}
```

2) An expensive function that creates an instance of that struct or enum

```rust
async fn get_item(v: usize) -> Result<Item, Error> {
    // your business logic here
    Ok(Item { value: v })
}
```

3) A redis connection

```rust
let client = RedisClient::new((IpAddr::V4(Ipv4Addr::LOCALHOST), 6379)).unwrap();
```

This library lets you add a cache to your application by `wrap()`ing your function:

```rust
let item = client.wrap(
    get_item,
    &RedisCacheArgs {
        lock_name: b"item_lock",
        key_name: b"item",
        expiry: Duration::from_secs(5),
    },
)(100)
.await?;

assert_eq!(item.value, 100);
```

That's it! Any call to the wrapped function will reuse the latest response if it is not
older than 5 seconds.

## Not implemented

* Cache depending on the input parameters
* Non-async wrap functions (should be easy to implement with some refactoring)

## Status

More of a proof of concept, there may be some big hidden issues.

## Available methods

|Method|Use case|
|---|---|
|`wrap()`|You want to unconditionally cache a response|
|`wrap_opt()`|Your function returns an `Option<T>` and you only want to cache `Some` responses|
|`wrap_const()`|You want to select whether or not to cache depending on a separate parameter taking `CacheDecision`|
|`wrap_on()`|You have a function that takes your `Item` and returns a `CacheDecision`|
