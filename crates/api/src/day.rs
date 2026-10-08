pub const NANOS: u64 = 86_400_000_000_000;

pub fn civil(day: u64) -> String {
    let shifted = i64::try_from(day).unwrap_or(i64::MAX) + 719_468;
    let era = shifted.div_euclid(146_097);
    let offset = shifted.rem_euclid(146_097);
    let annual = (offset - offset / 1_460 + offset / 36_524 - offset / 146_096) / 365;
    let ordinal = offset - (365 * annual + annual / 4 - annual / 100);
    let phase = (5 * ordinal + 2) / 153;
    let date = ordinal - (153 * phase + 2) / 5 + 1;
    let month = if phase < 10 { phase + 3 } else { phase - 9 };
    let year = annual + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{date:02}")
}

pub fn parse(name: &str) -> Option<u64> {
    let mut fields = name.splitn(3, '-');
    let year: i64 = fields.next()?.parse().ok()?;
    let month: i64 = fields.next()?.parse().ok()?;
    let date: i64 = fields.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&date) {
        return None;
    }
    let shifted = if month <= 2 { year - 1 } else { year };
    let era = shifted.div_euclid(400);
    let annual = shifted.rem_euclid(400);
    let phase = if month > 2 { month - 3 } else { month + 9 };
    let ordinal = (153 * phase + 2) / 5 + date - 1;
    let offset = annual * 365 + annual / 4 - annual / 100 + ordinal;
    let day = era * 146_097 + offset - 719_468;
    let day = u64::try_from(day).ok()?;
    (civil(day) == name).then_some(day)
}
