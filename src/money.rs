//! Parsing and formatting of user supplied amounts.
//!
//! Amounts may use either `.` or `,` as the decimal separator and may
//! contain grouping separators. Grouping is only accepted where it is
//! unambiguous: groups must contain three digits.

/// Parse an amount such as `137.42`, `18,75` or `2 480,15`.
pub fn parse_amount(input: &str) -> Result<f64, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("amount is empty".to_string());
    }

    let cleaned: String = trimmed
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '_')
        .collect();
    let cleaned = cleaned.strip_prefix('+').unwrap_or(&cleaned);

    if cleaned.is_empty() {
        return Err("amount is empty".to_string());
    }
    if cleaned.starts_with('-') {
        return Err("negative amounts are not supported".to_string());
    }
    if let Some(bad) = cleaned
        .chars()
        .find(|ch| !ch.is_ascii_digit() && *ch != '.' && *ch != ',')
    {
        return Err(format!("unexpected character '{bad}' in amount"));
    }
    if !cleaned.chars().any(|ch| ch.is_ascii_digit()) {
        return Err("amount has no digits".to_string());
    }

    let normalized = normalize_separators(cleaned)?;
    let value: f64 = normalized
        .parse()
        .map_err(|_| "amount is not a valid number".to_string())?;
    if !value.is_finite() {
        return Err("amount is too large".to_string());
    }

    Ok(value)
}

/// Format an amount with a fixed number of fraction digits.
pub fn format_amount(value: f64, precision: usize) -> String {
    format!("{value:.precision$}")
}

/// Round through the textual representation so `--json` and plain output agree.
pub fn round_amount(value: f64, precision: usize) -> f64 {
    format_amount(value, precision).parse().unwrap_or(value)
}

fn normalize_separators(input: &str) -> Result<String, String> {
    let dot = input.rfind('.');
    let comma = input.rfind(',');

    let decimal_pos = match (dot, comma) {
        (Some(d), Some(c)) => Some(d.max(c)),
        (Some(d), None) if input.matches('.').count() == 1 => Some(d),
        (None, Some(c)) if input.matches(',').count() == 1 => Some(c),
        _ => None,
    };

    let (integer, fraction) = match decimal_pos {
        Some(pos) => (&input[..pos], Some(&input[pos + 1..])),
        None => (input, None),
    };

    validate_grouping(integer)?;
    if let Some(fraction) = fraction
        && fraction.contains(['.', ','])
    {
        return Err(ambiguous());
    }

    let mut out = String::with_capacity(input.len());
    for (idx, ch) in input.char_indices() {
        if Some(idx) == decimal_pos {
            out.push('.');
        } else if ch == '.' || ch == ',' {
            continue;
        } else {
            out.push(ch);
        }
    }
    Ok(out)
}

fn validate_grouping(integer: &str) -> Result<(), String> {
    if !integer.contains(['.', ',']) {
        return Ok(());
    }

    let mut groups = integer.split(['.', ',']);
    let first = groups.next().unwrap_or_default();

    if first.is_empty() || first.len() > 3 {
        return Err(ambiguous());
    }
    for group in groups {
        if group.len() != 3 {
            return Err(ambiguous());
        }
    }
    Ok(())
}

fn ambiguous() -> String {
    "ambiguous number format; use 1,234.56 or 1.234,56".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_amounts() {
        let cases = [
            ("42", 42.0),
            ("137.42", 137.42),
            ("18,75", 18.75),
            ("2 480,15", 2480.15),
            ("1,284.60", 1284.6),
            ("1.284,60", 1284.6),
            ("2.480.150,75", 2_480_150.75),
            ("2,480,150", 2_480_150.0),
            ("0", 0.0),
            (".5", 0.5),
            ("5.", 5.0),
            ("+5", 5.0),
            ("  42  ", 42.0),
            ("1\u{00a0}000", 1000.0),
            ("1\u{202f}000,25", 1000.25),
            ("1_000", 1000.0),
            ("0,000001", 0.000001),
        ];

        for (input, expected) in cases {
            let parsed = parse_amount(input).unwrap_or_else(|err| panic!("{input:?}: {err}"));
            assert!(
                (parsed - expected).abs() < 1e-9,
                "{input:?}: expected {expected}, got {parsed}"
            );
        }
    }

    #[test]
    fn rejects_invalid_amounts() {
        for input in [
            "", "   ", "abc", "-42", "--5", "12,34,56", "1.2.3", "1,23.4,5", ".", ",", "1e5", "12a",
        ] {
            assert!(parse_amount(input).is_err(), "{input:?} should be rejected");
        }
    }

    #[test]
    fn formats_with_fixed_precision() {
        assert_eq!(format_amount(820.0, 2), "820.00");
        assert_eq!(format_amount(8.6, 0), "9");
        assert_eq!(format_amount(0.123456, 6), "0.123456");
        assert_eq!(format_amount(1.005, 2), "1.00");
    }

    #[test]
    fn rounding_matches_text_output() {
        assert_eq!(round_amount(820.0000001, 2), 820.0);
        assert_eq!(round_amount(4223.82, 2), 4223.82);
        assert_eq!(round_amount(4223.816, 1), 4223.8);
    }
}
