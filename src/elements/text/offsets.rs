//! Positions between Rust's UTF-8 bytes and a platform's text unit.

use crate::ffi::{NG_TEXT_UNIT_CHAR, ng_platform_text_unit};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unit {
    /// UTF-16 code units: Windows and macOS.
    Utf16,
    /// Characters: GTK.
    Char,
}

impl Unit {
    pub(crate) fn of_platform() -> Self {
        if unsafe { ng_platform_text_unit() } == NG_TEXT_UNIT_CHAR {
            Self::Char
        } else {
            Self::Utf16
        }
    }

    fn width(self, c: char) -> usize {
        match self {
            Self::Utf16 => c.len_utf16(),
            Self::Char => 1,
        }
    }
}

pub(crate) fn to_units(text: &str, byte: usize, unit: Unit) -> Option<usize> {
    text.is_char_boundary(byte)
        .then(|| text[..byte].chars().map(|c| unit.width(c)).sum())
}

pub(crate) fn to_bytes(text: &str, units: usize, unit: Unit) -> usize {
    let mut seen = 0;
    for (at, c) in text.char_indices() {
        let width = unit.width(c);
        if seen + width > units {
            return at;
        }
        seen += width;
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "a한🙂b";

    #[test]
    fn bytes_become_utf16_units() {
        assert_eq!(to_units(TEXT, 0, Unit::Utf16), Some(0));
        assert_eq!(to_units(TEXT, 4, Unit::Utf16), Some(2));
        assert_eq!(to_units(TEXT, 8, Unit::Utf16), Some(4));
        assert_eq!(to_units(TEXT, TEXT.len(), Unit::Utf16), Some(5));
    }

    #[test]
    fn bytes_become_characters() {
        assert_eq!(to_units(TEXT, 8, Unit::Char), Some(3));
        assert_eq!(to_units(TEXT, TEXT.len(), Unit::Char), Some(4));
    }

    #[test]
    fn a_byte_inside_a_character_has_no_position() {
        assert_eq!(to_units(TEXT, 2, Unit::Utf16), None);
        assert_eq!(to_units(TEXT, TEXT.len() + 1, Unit::Char), None);
    }

    #[test]
    fn units_become_bytes_and_round_trip() {
        for byte in [0, 1, 4, 8, TEXT.len()] {
            for unit in [Unit::Utf16, Unit::Char] {
                let units = to_units(TEXT, byte, unit).unwrap_or(usize::MAX);
                assert_eq!(to_bytes(TEXT, units, unit), byte, "{byte} via {unit:?}");
            }
        }
    }

    #[test]
    fn half_a_surrogate_pair_lands_before_it() {
        assert_eq!(to_bytes(TEXT, 3, Unit::Utf16), 4);
        assert_eq!(to_bytes(TEXT, 99, Unit::Utf16), TEXT.len());
    }
}
