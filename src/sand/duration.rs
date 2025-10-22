use std::num::ParseIntError;
use std::str::FromStr;
use std::time::Duration;

pub trait DurationExt {
    fn format_colon_separated(&self) -> String;
}

impl DurationExt for Duration {
    fn format_colon_separated(&self) -> String {
        let total_seconds = self.as_secs();
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        let seconds = total_seconds % 60;
        let millis = self.subsec_millis();

        format!("{:02}:{:02}:{:02}:{:03}", hours, minutes, seconds, millis)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum TimeUnit {
    Hours,
    Minutes,
    Seconds,
    Milliseconds,
}

impl TimeUnit {
    fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "h" | "hr" | "hrs" | "hours" => Some(Self::Hours),
            "m" | "min" | "mins" | "minutes" => Some(Self::Minutes),
            "s" | "sec" | "secs" | "seconds" => Some(Self::Seconds),
            "ms" | "milli" | "millis" | "milliseconds" => Some(Self::Milliseconds),
            "" => Some(Self::Seconds),
            _ => None,
        }
    }

    fn to_duration(&self, count: u64) -> Duration {
        match self {
            Self::Hours => Duration::from_secs(count * 3600),
            Self::Minutes => Duration::from_secs(count * 60),
            Self::Seconds => Duration::from_secs(count),
            Self::Milliseconds => Duration::from_millis(count),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ParseDurationComponentError {
    BadCount(ParseIntError),
    BadUnit,
}

impl std::fmt::Display for ParseDurationComponentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseDurationComponentError::BadCount(e) => write!(f, "failed to parse count: {}", e),
            ParseDurationComponentError::BadUnit => write!(f, "invalid unit"),
        }
    }
}

impl std::error::Error for ParseDurationComponentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ParseDurationComponentError::BadCount(e) => Some(e),
            ParseDurationComponentError::BadUnit => None,
        }
    }
}

pub fn parse_duration_component(component: &str) -> Result<Duration, ParseDurationComponentError> {
    use ParseDurationComponentError::*;
    let split_point = component
        .find(|c: char| !c.is_digit(10))
        .unwrap_or(component.len());
    let (count_str, unit_str) = component.split_at(split_point);
    let count = u64::from_str(count_str).map_err(|e| BadCount(e))?;
    let unit = TimeUnit::parse(unit_str).ok_or(BadUnit)?;
    Ok(unit.to_duration(count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration() {
        let cases = vec![
            ("1".to_string(), Ok(Duration::from_secs(1))),
            ("5s".to_string(), Ok(Duration::from_secs(5))),
            ("12".to_string(), Ok(Duration::from_secs(12))),
            ("30".to_string(), Ok(Duration::from_secs(30))),
            ("500ms".to_string(), Ok(Duration::from_millis(500))),
            ("30s".to_string(), Ok(Duration::from_secs(30))),
            ("5sec".to_string(), Ok(Duration::from_secs(5))),
            ("30sec".to_string(), Ok(Duration::from_secs(30))),
            ("5secs".to_string(), Ok(Duration::from_secs(5))),
            ("1m".to_string(), Ok(Duration::from_secs(60))),
            ("5m".to_string(), Ok(Duration::from_secs(5 * 60))),
            ("5min".to_string(), Ok(Duration::from_secs(5 * 60))),
            ("5mins".to_string(), Ok(Duration::from_secs(5 * 60))),
            ("15m".to_string(), Ok(Duration::from_secs(15 * 60))),
            ("15mins".to_string(), Ok(Duration::from_secs(15 * 60))),
            ("2h".to_string(), Ok(Duration::from_secs(2 * 3600))),
        ];

        for (input, expected) in cases {
            let actual = parse_duration_component(&input);
            assert_eq!(actual, expected, "Failed for input: {:?}", input);
        }
    }
}
