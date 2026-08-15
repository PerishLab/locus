use super::Interval;

pub(super) struct Reach {
    pub(super) at: u64,
    pub(super) until: u64,
}

pub(super) fn tangled(intervals: &[Interval]) -> Vec<Reach> {
    let mut reaches = Vec::new();
    for (index, outer) in intervals.iter().enumerate() {
        for inner in &intervals[index + 1..] {
            if inner.at >= outer.until {
                break;
            }
            if inner.until > outer.until {
                reaches.push(Reach {
                    at: outer.at,
                    until: inner.until,
                });
            }
        }
    }
    merge(reaches)
}

pub(super) fn struck(reaches: &[Reach], interval: &Interval) -> Option<usize> {
    reaches
        .iter()
        .position(|reach| interval.at < reach.until && reach.at < interval.until)
}

fn merge(mut reaches: Vec<Reach>) -> Vec<Reach> {
    reaches.sort_by_key(|reach| (reach.at, reach.until));
    let mut merged: Vec<Reach> = Vec::new();
    for reach in reaches {
        match merged.last_mut() {
            Some(last) if reach.at <= last.until => last.until = last.until.max(reach.until),
            _ => merged.push(reach),
        }
    }
    merged
}
