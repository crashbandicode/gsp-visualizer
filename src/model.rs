//! Host-testable half: hook sites, game addresses, the ruleset key and the
//! diagnostic settings.
//!
//! On the online character select (panel mode 20), hovering a fighter makes
//! `.1a19bf8` look the fighter up in the panel's map (`+0x290`, key = the
//! fighter's Hash40, value at node `+0x28`) and call `.1a1c070(part, info)`.
//! When `info` holds a GSP (see [`shows_number`]), the setter writes it into
//! the part's `set_txt_match` pane through `mel_chara_select_world_pw_n`,
//! which ends in the pane text setter `.37a2270`. The plugin's only hook is
//! at `.1a1c118`, after that write: it reads the text the game set and calls
//! the setter again with the label appended. It hooks nothing that other UI
//! code shares; another plugin's hook on the setter just runs inside that call.

use crate::label;

pub const SUPPORTED_VERSION: &str = "13.0.5";

/// How a site is checked before anything is installed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Hooked or relied on in place: all 64 bytes must be the vanilla ones.
    /// Another plugin's hook here means this plugin installs nothing.
    Exact,
    /// A function the plugin only calls: its first [`ENTRY_PATCH`] bytes may
    /// carry another plugin's hook (calling through it is fine); the rest
    /// must be vanilla.
    Called,
}

/// Bytes a Skyline hook may rewrite at a function entry (an absolute jump:
/// `ldr x17, #8; br x17; .quad target`).
pub const ENTRY_PATCH: usize = 16;

/// A code site and the 64 bytes that must be there (13.0.5 image).
pub struct Site {
    pub name: &'static str,
    pub offset: usize,
    pub kind: Kind,
    pub guard: [u8; 64],
}

impl Site {
    /// The guard bytes the check compares.
    pub fn checked(&self) -> core::ops::Range<usize> {
        match self.kind {
            Kind::Exact => 0..64,
            Kind::Called => ENTRY_PATCH..64,
        }
    }
}

/// The world GSP setter after its text call, where both paths meet: `ldr x0,
/// [sp, #8]`. x19 = the info (the map node + 0x28, so the fighter key is at
/// x19 - 8), x29 = the frame (the pane handle object at x29 - 0x20, the
/// return address at x29 + 8). Nothing jumps into the 16 bytes from here.
pub const APPLY_OFFSET: usize = 0x1a1c118;
pub const APPLY: Site = Site {
    name: "apply",
    offset: APPLY_OFFSET,
    kind: Kind::Exact,
    guard: [
        0xe0, 0x07, 0x40, 0xf9, 0x08, 0xc1, 0x01, 0xb0, 0x08, 0xa1, 0x26, 0x91, 0xe8, 0x7f, 0x00,
        0xa9, 0x40, 0x00, 0x00, 0xb4, 0x85, 0x4a, 0x7c, 0x94, 0xfd, 0x7b, 0x42, 0xa9, 0xf4, 0x4f,
        0x41, 0xa9, 0xff, 0xc3, 0x00, 0x91, 0xc0, 0x03, 0x5f, 0xd6, 0xff, 0xc3, 0x01, 0xd1, 0xf7,
        0x1b, 0x00, 0xf9, 0xf6, 0x57, 0x04, 0xa9, 0xf4, 0x4f, 0x05, 0xa9, 0xfd, 0x7b, 0x06, 0xa9,
        0xfd, 0x83, 0x01, 0x91,
    ],
};

// The site is inside the setter `.1a1c070` (which returns at `.1a1c13c`).
const _: () = assert!(0x1a1c070 < APPLY_OFFSET && APPLY_OFFSET + ENTRY_PATCH <= 0x1a1c13c);

/// The main-image offsets this plugin hooks, as `gsp_visualizer_hooked_sites_v1`
/// reports them to other plugins.
pub const HOOKED_SITES: [usize; 1] = [APPLY_OFFSET];

/// The hover call `bl .1a1c070`; the return address the hook expects is the
/// next instruction. Checked, never hooked.
pub const HOVER_CALL_OFFSET: usize = 0x1a19c44;
pub const HOVER_RETURN_OFFSET: usize = HOVER_CALL_OFFSET + 4;
pub const HOVER_CALL: Site = Site {
    name: "hover_call",
    offset: HOVER_CALL_OFFSET,
    kind: Kind::Exact,
    guard: [
        0x0b, 0x09, 0x00, 0x94, 0x26, 0x00, 0x00, 0x14, 0xd5, 0x11, 0x00, 0x35, 0x69, 0x62, 0x41,
        0xb9, 0x3f, 0x05, 0x00, 0x71, 0x61, 0x11, 0x00, 0x54, 0x01, 0x01, 0x41, 0xf9, 0x60, 0x42,
        0x0a, 0x91, 0xe8, 0xc3, 0x00, 0x91, 0xe2, 0x63, 0x00, 0x91, 0xe1, 0x1b, 0x00, 0xf9, 0xe8,
        0x0f, 0x00, 0xf9, 0x23, 0x07, 0x00, 0x94, 0xf5, 0x03, 0x00, 0xaa, 0xc1, 0x44, 0x01, 0xd0,
        0x21, 0x9c, 0x35, 0x91,
    ],
};

/// Pane text setter `(pane, text, units, flags)`; every pane text passes it.
/// Called, never hooked: it grows the pane's buffer to fit (TextBox vtable
/// `+0xe8`) before setting the text (`+0x110`).
pub const SET_TEXT_OFFSET: usize = 0x37a2270;
pub const SET_TEXT: Site = Site {
    name: "set_text",
    offset: SET_TEXT_OFFSET,
    kind: Kind::Called,
    guard: [
        0xfc, 0x57, 0xbd, 0xa9, 0xf4, 0x4f, 0x01, 0xa9, 0xfd, 0x7b, 0x02, 0xa9, 0xfd, 0x83, 0x00,
        0x91, 0xff, 0x0b, 0x40, 0xd1, 0xff, 0x43, 0x00, 0xd1, 0xf4, 0x03, 0x01, 0xaa, 0xf3, 0x03,
        0x00, 0xaa, 0xbf, 0x83, 0x1d, 0xf8, 0xe3, 0x0b, 0x00, 0x36, 0xbf, 0x83, 0x1d, 0xf8, 0x74,
        0x0f, 0x00, 0xb4, 0x8a, 0x02, 0x40, 0x79, 0x2a, 0x0f, 0x00, 0x34, 0xe8, 0x03, 0x1f, 0x2a,
        0xe9, 0x07, 0x00, 0x32,
    ],
};

/// Integer setting `(manager, key) -> i32`: the server's value, else the
/// game's parameter resource.
pub const GET_INT_OFFSET: usize = 0x1739120;
pub const GET_INT: Site = Site {
    name: "get_int",
    offset: GET_INT_OFFSET,
    kind: Kind::Called,
    guard: [
        0xf3, 0x0f, 0x1e, 0xf8, 0xfd, 0x7b, 0x01, 0xa9, 0xfd, 0x43, 0x00, 0x91, 0xf3, 0x03, 0x00,
        0xaa, 0xfc, 0xfe, 0xff, 0x97, 0x20, 0x14, 0xf8, 0x37, 0x1f, 0xd0, 0x07, 0x71, 0x62, 0x15,
        0x00, 0x54, 0x69, 0xce, 0x20, 0x8b, 0x29, 0xf1, 0x40, 0x39, 0x08, 0x7c, 0x40, 0x93, 0x89,
        0x00, 0x00, 0x34, 0x68, 0x0e, 0x08, 0x8b, 0x00, 0x39, 0x40, 0xb9, 0x99, 0x00, 0x00, 0x14,
        0x69, 0x0a, 0x40, 0xf9,
    ],
};

/// Float setting `(manager, key) -> f32`: the integer over its divisor.
pub const GET_FLOAT_OFFSET: usize = 0x17393f0;
pub const GET_FLOAT: Site = Site {
    name: "get_float",
    offset: GET_FLOAT_OFFSET,
    kind: Kind::Called,
    guard: [
        0xf3, 0x0f, 0x1e, 0xf8, 0xfd, 0x7b, 0x01, 0xa9, 0xfd, 0x43, 0x00, 0x91, 0xf3, 0x03, 0x00,
        0xaa, 0x48, 0xfe, 0xff, 0x97, 0xe0, 0x03, 0x27, 0x1e, 0xc0, 0x1e, 0xf8, 0x37, 0x1f, 0xd0,
        0x07, 0x71, 0xc2, 0x2a, 0x00, 0x54, 0x69, 0xce, 0x20, 0x8b, 0x29, 0xf1, 0x40, 0x39, 0x08,
        0x7c, 0x40, 0x93, 0x89, 0x04, 0x00, 0x34, 0x69, 0x0e, 0x08, 0x8b, 0x29, 0x39, 0x40, 0xb9,
        0x6a, 0x0a, 0x40, 0xf9,
    ],
};

/// Fighter Hash40 to rating slot `(fighter db, hash) -> i32` (-1 if none).
pub const FIGHTER_SLOT_OFFSET: usize = 0x3264270;
pub const FIGHTER_SLOT: Site = Site {
    name: "fighter_slot",
    offset: FIGHTER_SLOT_OFFSET,
    kind: Kind::Called,
    guard: [
        0xff, 0x43, 0x01, 0xd1, 0xf8, 0x5f, 0x01, 0xa9, 0xf6, 0x57, 0x02, 0xa9, 0xf4, 0x4f, 0x03,
        0xa9, 0xfd, 0x7b, 0x04, 0xa9, 0xfd, 0x03, 0x01, 0x91, 0xf5, 0x03, 0x01, 0xaa, 0xf3, 0x03,
        0x00, 0xaa, 0x04, 0xfc, 0xff, 0x97, 0xe0, 0x00, 0x00, 0x36, 0xe1, 0x00, 0x97, 0xd2, 0x81,
        0x39, 0xb6, 0xf2, 0x21, 0x02, 0xc0, 0xf2, 0xe0, 0x03, 0x13, 0xaa, 0xbe, 0xfb, 0xff, 0x97,
        0xf5, 0x03, 0x00, 0xaa,
    ],
};

/// Rating to displayed GSP `(owner, kind, rating) -> u32`.
pub const TO_GSP_OFFSET: usize = 0x17d1480;
pub const TO_GSP: Site = Site {
    name: "to_gsp",
    offset: TO_GSP_OFFSET,
    kind: Kind::Called,
    guard: [
        0xff, 0x03, 0x02, 0xd1, 0xea, 0x1b, 0x00, 0xfd, 0xe9, 0x23, 0x04, 0x6d, 0xf6, 0x57, 0x05,
        0xa9, 0xf4, 0x4f, 0x06, 0xa9, 0xfd, 0x7b, 0x07, 0xa9, 0xfd, 0xc3, 0x01, 0x91, 0x28, 0x3c,
        0x00, 0x12, 0xf3, 0x03, 0x02, 0x2a, 0xf4, 0x03, 0x00, 0xaa, 0x1f, 0x55, 0x00, 0x71, 0x81,
        0x06, 0x00, 0x54, 0xe0, 0x03, 0x00, 0x91, 0x3b, 0x66, 0xfd, 0x97, 0xe8, 0x03, 0x40, 0x39,
        0x88, 0x09, 0x00, 0x34,
    ],
};

/// Every site checked before anything is installed.
pub const SITES: [&Site; 7] = [
    &APPLY,
    &HOVER_CALL,
    &SET_TEXT,
    &GET_INT,
    &GET_FLOAT,
    &FIGHTER_SLOT,
    &TO_GSP,
];

/// `nn::ui2d::TextBox` fields the setter compares before rewriting a pane
/// (`.37a2494..37a24a0`): the current UTF-16 string and its length.
pub mod textbox {
    pub const STRING: usize = 0xd8;
    pub const LENGTH: usize = 0x112;
}

/// Whether the setter took its GSP path (`.1a1c0d0..1a1c0e8`): `info[0]`
/// set and mode 1, or mode 2.
pub fn shows_number(has_value: u32, mode: u32) -> bool {
    (has_value != 0 && mode == 1) || mode == 2
}

/// Pointer chains from the image base (as `.17049d0`, `.17994c0`,
/// `.1a27608` and `.1704a0c` walk them).
pub mod addr {
    /// `*(*(base + RATING) + 8)` = the rating state: `+0xa8` the 99 cached
    /// records (stride 0x20, from `+0xc0`: header u64, A, B, C), `+0xb0` the
    /// settings manager.
    pub const RATING: usize = 0x52c4c58;
    pub const RATING_RECORDS: usize = 0xa8;
    pub const RATING_SETTINGS: usize = 0xb0;
    pub const RECORD_BASE: usize = 0xc0;
    pub const RECORD_STRIDE: usize = 0x20;
    pub const RECORD_A: usize = 8;
    pub const RECORD_B: usize = 0xc;
    pub const RECORD_C: usize = 0x10;
    pub const SLOTS: i32 = 99;
    /// `*(*(*(base + FIGHTERS) + 8) + 0x168)` = the fighter database.
    pub const FIGHTERS: usize = 0x532f730;
    /// `*(*(*(*(base + GSP_OWNER)) + 0x68))` = the converter's owner; its
    /// `*(+8) + 0x22e8` is the kind-21 table's entry count (0 = no table).
    pub const GSP_OWNER: usize = 0x5314510;
    pub const GSP_TABLE_COUNT: usize = 0x22e8;
    /// Current rules, as `.17a6564..66c8` reads them.
    pub const RULE: usize = 0x530a975;
    pub const RULE_SECONDS: usize = 0x530a978;
    pub const RULE_A: usize = 0x530a97a;
    pub const RULE_B: usize = 0x530a97b;
}

/// GSP kind of a fighter's online rating.
pub const KIND_FIGHTER: u32 = 21;

/// Settings keys.
pub mod key {
    pub const R0: u32 = 1000;
    pub const DEFAULT_DEVIATION: u32 = 1001;
    pub const VOLATILITY: u32 = 1002;
    pub const ELITE_THRESHOLD: u32 = 1201;
    pub const ELITE_ENABLED: u32 = 9010;
}

/// The setting that holds K for the rules (`.17a6564..66c8`); `None` keeps
/// the game's 1.0.
pub fn ruleset_key(rule: u8, seconds: u16, a: u8, b: u8) -> Option<u32> {
    match rule {
        0 => match seconds {
            120 => Some(1010),
            150 => Some(1011),
            180 => Some(1012),
            _ => None,
        },
        1 => match a {
            1..=3 => Some(1012 + u32::from(a)),
            _ => None,
        },
        2 => match (a, b) {
            (1..=3, 4..=8) => Some(1016 + 5 * u32::from(a - 1) + u32::from(b - 4)),
            _ => None,
        },
        _ => None,
    }
}

/// Settings. The shipped build uses `DEFAULT`; Eden-only diagnostic builds
/// read `diag.txt` (`joiner=newline|space`, `dot=middle|dash`,
/// `threshold=<rating>|native`, one per line).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Settings {
    pub newline: bool,
    pub middle_dot: bool,
    pub threshold: Option<i32>,
}

impl Settings {
    pub const DEFAULT: Self = Self {
        newline: true,
        middle_dot: true,
        threshold: None,
    };

    pub fn joiner(self) -> &'static [u16] {
        if self.newline {
            &[0x0a]
        } else {
            &[0x20, 0x20]
        }
    }

    pub fn dot(self) -> &'static [u16] {
        if self.middle_dot {
            label::DOT
        } else {
            label::FALLBACK_DOT
        }
    }

    pub fn parse(text: &[u8]) -> Self {
        let mut settings = Self::DEFAULT;
        for line in text.split(|b| *b == b'\n') {
            let Some(eq) = line.iter().position(|b| *b == b'=') else {
                continue;
            };
            match (trim(&line[..eq]), trim(&line[eq + 1..])) {
                (b"joiner", b"newline") => settings.newline = true,
                (b"joiner", b"space") => settings.newline = false,
                (b"dot", b"middle") => settings.middle_dot = true,
                (b"dot", b"dash") => settings.middle_dot = false,
                (b"threshold", b"native") => settings.threshold = None,
                (b"threshold", value) => {
                    if let Some(t) = core::str::from_utf8(value)
                        .ok()
                        .and_then(|v| v.parse().ok())
                    {
                        settings.threshold = Some(t);
                    }
                }
                _ => {}
            }
        }
        settings
    }

    /// Two words, so hooks can read settings the writer thread reloads.
    pub fn to_bits(self) -> u64 {
        let flags = u64::from(self.newline) | (u64::from(self.middle_dot) << 1);
        match self.threshold {
            Some(t) => flags | 4 | (u64::from(t as u32) << 32),
            None => flags,
        }
    }

    pub fn from_bits(bits: u64) -> Self {
        Self {
            newline: bits & 1 != 0,
            middle_dot: bits & 2 != 0,
            threshold: (bits & 4 != 0).then_some((bits >> 32) as u32 as i32),
        }
    }
}

fn trim(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map_or(start, |i| i + 1);
    &bytes[start..end]
}

/// The units of `text` before its first NUL.
pub fn until_nul(text: &[u16]) -> &[u16] {
    &text[..text.iter().position(|u| *u == 0).unwrap_or(text.len())]
}

/// A text line built in a fixed buffer; anything past `N` bytes is dropped.
pub struct Line<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> Line<N> {
    pub const fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// The text plus a NUL, for `nn::fs` paths; `None` when there is no room.
    pub fn with_nul(&mut self) -> Option<&[u8]> {
        if self.len >= N {
            return None;
        }
        self.buf[self.len] = 0;
        Some(&self.buf[..=self.len])
    }
}

impl<const N: usize> Default for Line<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> core::fmt::Write for Line<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let take = s.len().min(N - self.len);
        self.buf[self.len..self.len + take].copy_from_slice(&s.as_bytes()[..take]);
        self.len += take;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Write as _;

    #[test]
    fn guards_start_with_the_expected_instructions() {
        assert_eq!(APPLY.guard[..4], [0xe0, 0x07, 0x40, 0xf9]); // ldr x0, [sp, #8]
        assert_eq!(HOVER_CALL.guard[..4], [0x0b, 0x09, 0x00, 0x94]); // bl .1a1c070
        assert_eq!(SET_TEXT.guard[..4], [0xfc, 0x57, 0xbd, 0xa9]); // stp x28, x21, [sp, #-0x30]!

        // bl .1a1c070 from .1a19c44: imm26 = (0x1a1c070 - 0x1a19c44) / 4.
        let imm = u32::from_le_bytes(HOVER_CALL.guard[..4].try_into().unwrap()) & 0x03ff_ffff;
        assert_eq!(HOVER_CALL_OFFSET + imm as usize * 4, 0x1a1c070);
    }

    #[test]
    fn hooked_and_called_sites_are_checked_differently() {
        let hooked: Vec<_> = SITES
            .iter()
            .filter(|s| s.kind == Kind::Exact)
            .map(|s| s.name)
            .collect();
        assert_eq!(hooked, ["apply", "hover_call"]);
        assert_eq!(APPLY.checked(), 0..64);
        assert_eq!(SET_TEXT.checked(), ENTRY_PATCH..64);
        assert_eq!(HOOKED_SITES, [APPLY_OFFSET]);
        // Every called function keeps enough vanilla bytes to be recognised.
        for site in SITES.iter().filter(|s| s.kind == Kind::Called) {
            assert!(site.checked().len() >= 48, "{}", site.name);
        }
    }

    #[test]
    fn gsp_path_matches_the_setter() {
        assert!(shows_number(1, 1));
        assert!(shows_number(0, 2));
        assert!(shows_number(1, 2));
        assert!(!shows_number(0, 1));
        assert!(!shows_number(1, 0));
        assert!(!shows_number(0, 0));
    }

    #[test]
    fn ruleset_keys_follow_the_game_table() {
        assert_eq!(ruleset_key(0, 120, 0, 0), Some(1010));
        assert_eq!(ruleset_key(0, 150, 0, 0), Some(1011));
        assert_eq!(ruleset_key(0, 180, 0, 0), Some(1012));
        assert_eq!(ruleset_key(0, 240, 0, 0), None);
        assert_eq!(ruleset_key(1, 0, 1, 0), Some(1013));
        assert_eq!(ruleset_key(1, 0, 3, 0), Some(1015));
        assert_eq!(ruleset_key(1, 0, 4, 0), None);
        assert_eq!(ruleset_key(2, 0, 1, 4), Some(1016));
        assert_eq!(ruleset_key(2, 0, 1, 8), Some(1020));
        assert_eq!(ruleset_key(2, 0, 2, 4), Some(1021));
        assert_eq!(ruleset_key(2, 0, 3, 7), Some(1029));
        assert_eq!(ruleset_key(2, 0, 3, 8), Some(1030));
        assert_eq!(ruleset_key(2, 0, 3, 3), None);
        assert_eq!(ruleset_key(2, 0, 0, 5), None);
        assert_eq!(ruleset_key(3, 120, 1, 4), None);
    }

    #[test]
    fn settings_parse_and_round_trip() {
        assert_eq!(Settings::parse(b""), Settings::DEFAULT);
        let s = Settings::parse(b"joiner = space\r\ndot=dash\nthreshold=1600\njunk\n");
        assert_eq!(
            s,
            Settings {
                newline: false,
                middle_dot: false,
                threshold: Some(1600)
            }
        );
        assert_eq!(
            Settings::parse(b"threshold=5\nthreshold=native").threshold,
            None
        );
        assert_eq!(
            Settings::parse(b"threshold=5\nthreshold=junk").threshold,
            Some(5)
        );
        for s in [
            Settings::DEFAULT,
            s,
            Settings {
                threshold: Some(-3),
                ..Settings::DEFAULT
            },
        ] {
            assert_eq!(Settings::from_bits(s.to_bits()), s);
        }
        assert_eq!(Settings::DEFAULT.joiner(), &[0x0a]);
        assert_eq!(s.dot(), label::FALLBACK_DOT);
    }

    #[test]
    fn until_nul_stops_at_the_first_nul() {
        assert_eq!(until_nul(&[0x31, 0x32, 0, 0x33]), &[0x31, 0x32]);
        assert_eq!(until_nul(&[0x31]), &[0x31]);
        assert_eq!(until_nul(&[]), &[] as &[u16]);
    }

    #[test]
    fn line_drops_overflow_and_adds_nul() {
        let mut line = Line::<8>::new();
        let _ = write!(line, "0123456789");
        assert_eq!(line.as_bytes(), b"01234567");
        assert!(line.with_nul().is_none());
        line.clear();
        let _ = write!(line, "ab");
        assert_eq!(line.with_nul(), Some(&b"ab\0"[..]));
    }
}
