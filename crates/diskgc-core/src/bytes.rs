//! Byte quantities: formatting for humans and parsing of `docker system df` style strings.

pub const KIB: u64 = 1024;
pub const MIB: u64 = 1024 * KIB;
pub const GIB: u64 = 1024 * MIB;

pub fn gib(bytes: u64) -> f64 {
    bytes as f64 / GIB as f64
}

pub fn from_gib(g: f64) -> u64 {
    (g * GIB as f64) as u64
}

/// `1.5 GB`, `320 MB`, `12 KB`, `0 B` — binary units, short labels.
pub fn human(bytes: u64) -> String {
    let b = bytes as f64;
    if bytes >= GIB {
        format!("{:.1} GB", b / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.0} MB", b / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.0} KB", b / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Parse `13.29GB`, `75.58MB`, `1.2 GiB`, `512B`. Unknown units are treated as bytes.
pub fn parse_human(s: &str) -> u64 {
    let s = s.trim();
    let idx = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
    let num: f64 = s[..idx].trim().parse().unwrap_or(0.0);
    let mult = match s[idx..].trim().to_ascii_lowercase().as_str() {
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        "kib" => KIB as f64,
        "mib" => MIB as f64,
        "gib" => GIB as f64,
        _ => 1.0,
    };
    (num * mult) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(human(0), "0 B");
        assert_eq!(human(2048), "2 KB");
        assert_eq!(human(5 * MIB), "5 MB");
        assert_eq!(human(GIB + GIB / 2), "1.5 GB");
    }

    #[test]
    fn parses_docker_sizes() {
        assert_eq!(parse_human("13.29GB"), 13_290_000_000);
        assert_eq!(parse_human("75.58MB"), 75_580_000);
        assert_eq!(parse_human("1 GiB"), GIB);
        assert_eq!(parse_human("42B"), 42);
        assert_eq!(parse_human("garbage"), 0);
    }
}
