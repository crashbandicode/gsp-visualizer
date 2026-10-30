//! UTF-16 label formatting for the Quickplay fighter select.

use crate::glicko::{Estimate, LIMIT};

pub const DOT: &[u16] = &[0x0020, 0x00b7, 0x0020];
pub const FALLBACK_DOT: &[u16] = &[0x0020, 0x002d, 0x0020];

const ELITE: &[u16] = &[0x0045, 0x006c, 0x0069, 0x0074, 0x0065, 0x0020];
const COUNT_LIMIT: &[u16] = &[0x0039, 0x0039, 0x0039, 0x002b];
const WIN: &[u16] = &[0x0020, 0x0077, 0x0069, 0x006e];
const WINS: &[u16] = &[0x0020, 0x0077, 0x0069, 0x006e, 0x0073];
const LOSS: &[u16] = &[0x0020, 0x006c, 0x006f, 0x0073, 0x0073];
const LOSSES: &[u16] = &[0x0020, 0x006c, 0x006f, 0x0073, 0x0073, 0x0065, 0x0073];

/// Finds the first thousands separator between adjacent ASCII digits.
pub fn separator(gsp_text: &[u16]) -> Option<u16> {
    let len = gsp_text
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(gsp_text.len());
    if len < 3 {
        return None;
    }

    for window in gsp_text[..len].windows(3) {
        if is_digit(window[0]) && !is_digit(window[1]) && is_digit(window[2]) {
            return Some(window[1]);
        }
    }
    None
}

/// Writes a grouped decimal number, truncating at the output's end.
pub fn group(n: u32, sep: u16, out: &mut [u16]) -> usize {
    let mut digits = [0u16; 10];
    let mut value = n;
    let mut digit_count = 0;
    loop {
        digits[digit_count] = (value % 10) as u16 + 0x0030;
        digit_count += 1;
        value /= 10;
        if value == 0 {
            break;
        }
    }

    let mut written = 0;
    for (position, &digit) in digits[..digit_count].iter().rev().enumerate() {
        if position > 0 && (digit_count - position) % 3 == 0 {
            // Keep a separator with its following digit when the buffer is short.
            if out.len() - written < 2 {
                break;
            }
            out[written] = sep;
            written += 1;
        }
        if written == out.len() {
            break;
        }
        out[written] = digit;
        written += 1;
    }
    written
}

/// Composes the existing label and an Elite Smash reach or keep estimate.
pub fn compose(
    original: &[u16],
    joiner: &[u16],
    dot: &[u16],
    elite_gsp: u32,
    estimate: Estimate,
    out: &mut [u16],
) -> usize {
    if out.is_empty() {
        return 0;
    }

    let limit = out.len() - 1;
    let mut len = 0;
    append(original, out, &mut len, limit);
    append(joiner, out, &mut len, limit);
    append(ELITE, out, &mut len, limit);

    let sep = separator(original).unwrap_or(0x002c);
    len += group(elite_gsp, sep, &mut out[len..limit]);
    append(dot, out, &mut len, limit);

    let (count, wins) = match estimate {
        Estimate::Reach(count) => (count, true),
        Estimate::Keep(count) => (count, false),
    };
    if count >= LIMIT {
        append(COUNT_LIMIT, out, &mut len, limit);
    } else {
        len += group(count, 0, &mut out[len..limit]);
    }
    let word = match (wins, count == 1) {
        (true, true) => WIN,
        (true, false) => WINS,
        (false, true) => LOSS,
        (false, false) => LOSSES,
    };
    append(word, out, &mut len, limit);

    out[len] = 0;
    len
}

fn is_digit(unit: u16) -> bool {
    (0x0030..=0x0039).contains(&unit)
}

fn append(source: &[u16], out: &mut [u16], len: &mut usize, limit: usize) {
    for &unit in source {
        if unit == 0 || *len == limit {
            break;
        }
        out[*len] = unit;
        *len += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }

    fn text(units: &[u16]) -> String {
        String::from_utf16(units).unwrap()
    }

    fn grouped(n: u32) -> String {
        let mut out = [0u16; 16];
        let len = group(n, 0x002c, &mut out);
        text(&out[..len])
    }

    fn composed(estimate: Estimate) -> String {
        let original = utf16("Quickplay");
        let joiner = utf16(" | ");
        let mut out = [0u16; 96];
        let len = compose(&original, &joiner, DOT, 14_749_910, estimate, &mut out);
        assert_eq!(out[len], 0);
        text(&out[..len])
    }

    #[test]
    fn detects_common_thousands_separators() {
        assert_eq!(separator(&utf16("Elite 14,749,910")), Some(0x002c));
        assert_eq!(separator(&utf16("Elite 14.749.910")), Some(0x002e));
        assert_eq!(
            separator(&utf16("Elite 14\u{00a0}749\u{00a0}910")),
            Some(0x00a0)
        );
        assert_eq!(separator(&utf16("Elite 14749910")), None);
        assert_eq!(separator(&[0x0031, 0, 0x002c, 0x0032]), None);
    }

    #[test]
    fn groups_numbers_with_a_three_digit_stride() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(14_749_910), "14,749,910");
        assert_eq!(grouped(u32::MAX), "4,294,967,295");

        let mut short = [0u16; 3];
        let len = group(1_000_000, 0x002c, &mut short);
        assert_eq!(text(&short[..len]), "1,0");
        assert_eq!(group(1, 0x002c, &mut []), 0);
    }

    #[test]
    fn composes_singular_plural_and_limit_labels() {
        assert_eq!(
            composed(Estimate::Reach(1)),
            "Quickplay | Elite 14,749,910 · 1 win"
        );
        assert_eq!(
            composed(Estimate::Reach(2)),
            "Quickplay | Elite 14,749,910 · 2 wins"
        );
        assert_eq!(
            composed(Estimate::Keep(1)),
            "Quickplay | Elite 14,749,910 · 1 loss"
        );
        assert_eq!(
            composed(Estimate::Keep(2)),
            "Quickplay | Elite 14,749,910 · 2 losses"
        );
        assert_eq!(
            composed(Estimate::Reach(LIMIT)),
            "Quickplay | Elite 14,749,910 · 999+ wins"
        );
        assert_eq!(
            composed(Estimate::Keep(LIMIT)),
            "Quickplay | Elite 14,749,910 · 999+ losses"
        );
    }

    #[test]
    fn uses_original_separator_and_defaults_to_comma() {
        let mut out = [0u16; 80];
        let original = utf16("14.749.910");
        let len = compose(
            &original,
            &[],
            DOT,
            14_749_910,
            Estimate::Reach(1),
            &mut out,
        );
        assert_eq!(text(&out[..len]), "14.749.910Elite 14.749.910 · 1 win");

        let original = utf16("plain");
        let len = compose(
            &original,
            &[],
            DOT,
            14_749_910,
            Estimate::Reach(1),
            &mut out,
        );
        assert_eq!(text(&out[..len]), "plainElite 14,749,910 · 1 win");
    }

    #[test]
    fn truncates_and_always_terminates_nonempty_output() {
        let original = utf16("An existing label");
        let joiner = utf16(" / ");
        let mut full = [0u16; 96];
        let full_len = compose(
            &original,
            &joiner,
            DOT,
            14_749_910,
            Estimate::Keep(7),
            &mut full,
        );

        let mut short = [0xffffu16; 10];
        let short_len = compose(
            &original,
            &joiner,
            DOT,
            14_749_910,
            Estimate::Keep(7),
            &mut short,
        );
        assert!(full_len > short_len);
        assert_eq!(short_len, short.len() - 1);
        assert_eq!(&short[..short_len], &full[..short_len]);
        assert_eq!(short[short_len], 0);
        assert_eq!(
            compose(&original, &joiner, DOT, 1, Estimate::Reach(1), &mut []),
            0
        );

        let mut one = [0xffffu16; 1];
        assert_eq!(
            compose(&original, &joiner, DOT, 1, Estimate::Reach(1), &mut one),
            0
        );
        assert_eq!(one[0], 0);
    }
}
