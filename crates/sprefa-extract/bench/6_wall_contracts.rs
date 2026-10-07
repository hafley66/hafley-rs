//! Elapsed bounds run only through bench/scripts/6_wall_contracts.py.
pub fn enabled() -> bool {
    std::env::var("SPREFA_WALL_BENCH").as_deref() == Ok("1")
}

pub fn check(name: &str, actual: f64, bound: f64, inclusive: bool) {
    if enabled() {
        println!("wall bench {name}: actual={actual}, bound={bound}, inclusive={inclusive}");
        assert!(if inclusive { actual <= bound } else { actual < bound }, "wall contract {name}: {actual} exceeds {bound}");
    }
}

pub fn expired(started: std::time::Instant, limit: std::time::Duration) -> bool {
    enabled() && started.elapsed() >= limit
}

#[cfg(feature = "cli")]
pub async fn bounded<T>(duration: std::time::Duration, future: impl std::future::Future<Output = T>) -> T {
    if enabled() { tokio::time::timeout(duration, future).await.expect("wall bench response deadline") }
    else { future.await }
}
