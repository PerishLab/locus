use crate::check::Entry;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const CAPACITY: usize = 16_384;
const WINDOW: Duration = Duration::from_secs(300);

pub(crate) struct Cache {
    capacity: usize,
    window: Duration,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    seen: HashMap<String, Instant>,
    order: VecDeque<(String, Instant)>,
}

impl Cache {
    pub(crate) fn new() -> Self {
        Self::bounded(CAPACITY, WINDOW)
    }

    fn bounded(capacity: usize, window: Duration) -> Self {
        Self {
            capacity,
            window,
            state: Mutex::new(State::default()),
        }
    }

    pub(crate) fn reserve(&self, items: Vec<Entry>) -> Vec<Entry> {
        self.admit(items, Instant::now())
    }

    pub(crate) fn release(&self, ids: &[String]) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        for id in ids {
            state.seen.remove(id);
        }
    }

    fn admit(&self, items: Vec<Entry>, now: Instant) -> Vec<Entry> {
        let Ok(mut state) = self.state.lock() else {
            return items;
        };
        self.expire(&mut state, now);
        let mut fresh = Vec::with_capacity(items.len());
        for item in items {
            if state
                .seen
                .get(&item.id)
                .is_some_and(|at| now.saturating_duration_since(*at) < self.window)
            {
                continue;
            }
            state.seen.insert(item.id.clone(), now);
            state.order.push_back((item.id.clone(), now));
            fresh.push(item);
            self.evict(&mut state);
        }
        fresh
    }

    fn expire(&self, state: &mut State, now: Instant) {
        while let Some((_, at)) = state.order.front() {
            if now.saturating_duration_since(*at) < self.window {
                break;
            }
            if let Some((key, stamp)) = state.order.pop_front()
                && state.seen.get(&key) == Some(&stamp)
            {
                state.seen.remove(&key);
            }
        }
    }

    fn evict(&self, state: &mut State) {
        while state.seen.len() > self.capacity {
            let Some((key, stamp)) = state.order.pop_front() else {
                break;
            };
            if state.seen.get(&key) == Some(&stamp) {
                state.seen.remove(&key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> Entry {
        Entry {
            id: id.into(),
            line: format!("{{\"id\":\"{id}\"}}"),
        }
    }

    #[test]
    fn dedupe() {
        let cache = Cache::bounded(2, Duration::from_secs(60));
        let start = Instant::now();
        let first = cache.admit(vec![entry("a"), entry("a"), entry("b")], start);
        assert_eq!(first.len(), 2);
        assert!(cache.admit(vec![entry("a")], start).is_empty());
        let third = cache.admit(vec![entry("c")], start + Duration::from_secs(1));
        assert_eq!(third.len(), 1);
        assert_eq!(
            cache
                .admit(vec![entry("a")], start + Duration::from_secs(2))
                .len(),
            1
        );
    }

    #[test]
    fn ttl() {
        let cache = Cache::bounded(8, Duration::from_secs(10));
        let start = Instant::now();
        assert_eq!(cache.admit(vec![entry("x")], start).len(), 1);
        assert!(
            cache
                .admit(vec![entry("x")], start + Duration::from_secs(9))
                .is_empty()
        );
        assert_eq!(
            cache
                .admit(vec![entry("x")], start + Duration::from_secs(10))
                .len(),
            1
        );
    }

    #[test]
    fn rollback() {
        let cache = Cache::bounded(8, Duration::from_secs(60));
        let start = Instant::now();
        assert_eq!(cache.admit(vec![entry("r1")], start).len(), 1);
        cache.release(&["r1".into()]);
        assert_eq!(cache.admit(vec![entry("r1")], start).len(), 1);
    }
}
