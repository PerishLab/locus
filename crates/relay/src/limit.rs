use axum::http::HeaderMap;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::time::Instant;

const CAPACITY: usize = 4096;

pub(crate) struct Limiter {
    rate: f64,
    burst: f64,
    buckets: Mutex<HashMap<String, Bucket>>,
}

struct Bucket {
    tokens: f64,
    updated: Instant,
}

impl Limiter {
    pub(crate) fn new(rpm: u32, burst: u32) -> Self {
        Self {
            rate: f64::from(rpm) / 60.0,
            burst: f64::from(burst),
            buckets: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn check(&self, key: &str) -> Result<(), u64> {
        self.admit(key, Instant::now())
    }

    fn admit(&self, key: &str, now: Instant) -> Result<(), u64> {
        let mut map = self.buckets.lock().map_err(|_| 1u64)?;
        if let Some(bucket) = map.get_mut(key) {
            let elapsed = now.saturating_duration_since(bucket.updated).as_secs_f64();
            bucket.tokens = (bucket.tokens + elapsed * self.rate).min(self.burst);
            bucket.updated = now;
            if bucket.tokens >= 1.0 {
                bucket.tokens -= 1.0;
                return Ok(());
            }
            let wait = ((1.0 - bucket.tokens) / self.rate).ceil().max(1.0) as u64;
            return Err(wait);
        }
        if map.len() >= CAPACITY {
            self.evict(&mut map, now);
        }
        map.insert(
            key.to_owned(),
            Bucket {
                tokens: self.burst - 1.0,
                updated: now,
            },
        );
        Ok(())
    }

    fn evict(&self, map: &mut HashMap<String, Bucket>, now: Instant) {
        map.retain(|_, bucket| {
            let elapsed = now.saturating_duration_since(bucket.updated).as_secs_f64();
            elapsed * self.rate < self.burst
        });
        if map.len() >= CAPACITY
            && let Some(oldest) = map
                .iter()
                .min_by_key(|(_, bucket)| bucket.updated)
                .map(|(key, _)| key.clone())
        {
            map.remove(&oldest);
        }
    }
}

pub(crate) fn client(headers: &HeaderMap, peer: Option<SocketAddr>, forwarded: bool) -> String {
    if forwarded {
        if let Some(ip) = real(headers) {
            return ip.to_string();
        }
        if let Some(ip) = proxy(headers) {
            return ip.to_string();
        }
    }
    peer.map(|addr| addr.ip().to_string())
        .unwrap_or_else(|| "default".to_owned())
}

fn proxy(headers: &HeaderMap) -> Option<IpAddr> {
    let raw = headers
        .get_all("x-forwarded-for")
        .iter()
        .next_back()?
        .to_str()
        .ok()?;
    raw.rsplit(',').next()?.trim().parse().ok()
}

fn real(headers: &HeaderMap) -> Option<IpAddr> {
    headers.get("x-real-ip")?.to_str().ok()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn bounded() {
        let limiter = Limiter::new(60, 2);
        let start = Instant::now();
        for index in 0..(CAPACITY + 128) {
            let key = format!("10.0.{}.{}", index / 256, index % 256);
            assert!(limiter.admit(&key, start).is_ok());
        }
        assert!(limiter.buckets.lock().unwrap().len() <= CAPACITY);
    }

    #[test]
    fn refill() {
        let limiter = Limiter::new(60, 2);
        let start = Instant::now();
        assert!(limiter.admit("1.2.3.4", start).is_ok());
        assert!(limiter.admit("1.2.3.4", start).is_ok());
        assert_eq!(limiter.admit("1.2.3.4", start), Err(1));
        assert!(
            limiter
                .admit("1.2.3.4", start + Duration::from_secs(1))
                .is_ok()
        );
    }
}
