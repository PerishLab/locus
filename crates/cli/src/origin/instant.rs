const NANOS: u64 = 1_000_000_000;

pub fn parse(text: &str) -> Result<u64, String> {
    if !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()) {
        return text
            .parse()
            .map_err(|_| format!("{text} overflows nanoseconds since epoch"));
    }
    Text(text).instant().ok_or_else(|| {
        format!(
            "{text:?} is neither nanoseconds since epoch nor an RFC3339 instant with a zone, such as 2026-10-17T21:00:00+08:00 or 2026-10-17T13:00:00Z"
        )
    })
}

#[derive(Clone, Copy)]
struct Text<'a>(&'a str);

impl<'a> Text<'a> {
    fn instant(self) -> Option<u64> {
        let (date, rest) = self.0.split_once(['T', 't', ' '])?;
        let mut parts = date.splitn(3, '-');
        let year = Text(parts.next()?).number(4)?;
        let month = Text(parts.next()?).number(2)?;
        let day = Text(parts.next()?).number(2)?;
        let (clock, offset) = Text(rest).zone()?;
        let mut fields = clock.splitn(3, ':');
        let hour = Text(fields.next()?).number(2)?;
        let minute = Text(fields.next()?).number(2)?;
        let (second, fraction) = Text(fields.next()?).seconds()?;
        let ranges = [(month, 12), (hour, 23), (minute, 59), (second, 59)];
        if ranges.iter().any(|&(value, high)| value > high) || month < 1 {
            return None;
        }
        if !(1..=length(year, month)).contains(&day) {
            return None;
        }
        let total = civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second - offset;
        u64::try_from(total)
            .ok()?
            .checked_mul(NANOS)?
            .checked_add(fraction)
    }

    fn zone(self) -> Option<(&'a str, i64)> {
        if let Some(clock) = self.0.strip_suffix(['Z', 'z']) {
            return Some((clock, 0));
        }
        let index = self.0.rfind(['+', '-'])?;
        let (clock, offset) = self.0.split_at(index);
        let sign = if offset.starts_with('-') { -1 } else { 1 };
        let (hours, minutes) = offset[1..].split_once(':')?;
        let hours = Text(hours).number(2)?;
        let minutes = Text(minutes).number(2)?;
        if hours > 23 || minutes > 59 {
            return None;
        }
        Some((clock, sign * (hours * 3_600 + minutes * 60)))
    }

    fn seconds(self) -> Option<(i64, u64)> {
        let Some((whole, digits)) = self.0.split_once('.') else {
            return Some((self.number(2)?, 0));
        };
        let valid =
            (1..=9).contains(&digits.len()) && digits.bytes().all(|byte| byte.is_ascii_digit());
        if !valid {
            return None;
        }
        let padded = format!("{digits:0<9}");
        Some((Text(whole).number(2)?, padded.parse().ok()?))
    }

    fn number(self, width: usize) -> Option<i64> {
        let valid = self.0.len() == width && self.0.bytes().all(|byte| byte.is_ascii_digit());
        if valid { self.0.parse().ok() } else { None }
    }
}

fn length(year: i64, month: i64) -> i64 {
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn civil(year: i64, month: i64, day: i64) -> i64 {
    let shifted = if month <= 2 { year - 1 } else { year };
    let era = shifted.div_euclid(400);
    let within = shifted - era * 400;
    let position = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let cycle = within * 365 + within / 4 - within / 100 + position;
    era * 146_097 + cycle - 719_468
}

#[cfg(test)]
mod tests {
    use super::{NANOS, parse};

    #[test]
    fn zoned() {
        let expected = 1_792_242_000 * NANOS;
        assert_eq!(parse("2026-10-17T21:00:00+08:00"), Ok(expected));
        assert_eq!(parse("2026-10-17T13:00:00Z"), Ok(expected));
        assert_eq!(parse("2026-10-17 05:00:00-08:00"), Ok(expected));
        assert_eq!(parse("2026-10-17T13:00:00.25z"), Ok(expected + 250_000_000));
        assert_eq!(parse("2024-02-29T23:59:59Z"), Ok(1_709_251_199 * NANOS));
        assert_eq!(parse("1970-01-01T00:00:00Z"), Ok(0));
    }

    #[test]
    fn epoch() {
        assert_eq!(parse("42"), Ok(42));
        assert!(parse("99999999999999999999").is_err());
    }

    #[test]
    fn refused() {
        for text in [
            "",
            "yesterday",
            "2026-10-17T21:00:00",
            "2026-10-17",
            "2026-02-30T00:00:00Z",
            "2026-10-17T24:00:00Z",
            "2026-10-17T21:00:00+8:00",
            "1969-12-31T23:59:59Z",
            "2026-10-17T21:00:00.Z",
        ] {
            let error = parse(text).expect_err(text);
            assert!(error.contains("RFC3339"), "{error}");
        }
    }
}
