use super::model::{Fact, Pairing, Side};
use std::collections::BTreeMap;

#[derive(Default)]
struct Pair {
    before: u64,
    after: u64,
}

#[derive(Default)]
pub(super) struct Pairs {
    entries: BTreeMap<(String, Option<String>, String), Pair>,
}

impl Pairs {
    pub fn observe(&mut self, fact: &Fact) -> Result<(), String> {
        let (Some(call), Some(side)) = (&fact.call, fact.side) else {
            return Ok(());
        };
        let key = (fact.group.clone(), fact.round.clone(), call.clone());
        if !self.entries.contains_key(&key) && self.entries.len() == super::LIMIT {
            return Err("pair identity limit exceeded".into());
        }
        let pair = self.entries.entry(key).or_default();
        match side {
            Side::Before => pair.before += 1,
            Side::After => pair.after += 1,
        }
        Ok(())
    }

    pub fn finish(self) -> Pairing {
        let mut result = Pairing::default();
        for pair in self.entries.into_values() {
            result.before += pair.before;
            result.after += pair.after;
            result.starts += pair.before.saturating_sub(1);
            result.stops += pair.after.saturating_sub(1);
            match (pair.before, pair.after) {
                (1, 1) => result.matched += 1,
                (0, _) => result.closing += 1,
                (_, 0) => result.opening += 1,
                _ => result.ambiguous += 1,
            }
        }
        result
    }
}
