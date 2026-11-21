//! COMPOUND_TEXT to UTF-8.
//!
//! An input method sends committed and preedit text as X "Compound Text": ISO 2022 escape
//! sequences that name a character set for each half of the byte range, followed by text in
//! it. Which sets appear depends on the text, not on the locale: Xlib picks the smallest one
//! that holds the characters, so Cyrillic arrives as ISO 8859-5, Greek as ISO 8859-7, Hangul
//! as KS C 5601 and so on, and only what fits none of them as a UTF-8 segment.
//!
//! Decoding never fails. What a keyboard sends must not be able to take the application
//! down: a sequence this does not know is skipped, and a byte it cannot place becomes U+FFFD.

use encoding_rs::Encoding;

const ESC: u8 = 0x1B;
const CSI: u8 = 0x9B;
const STX: u8 = 0x02;

#[derive(Clone, Copy)]
enum Charset {
    /// ISO 646 (ASCII), and JIS X 0201 Roman, which differs in two characters nobody types.
    Ascii,
    /// The right half of ISO 8859-1, the initial set of the high bytes.
    Latin1,
    /// The right half of a single-byte encoding.
    Single(&'static Encoding),
    /// JIS X 0201 Katakana.
    Katakana,
    /// A 94x94 set, two bytes a character.
    Double(Double),
    /// A set this does not decode, with the bytes each of its characters takes.
    Unknown(usize),
}

#[derive(Clone, Copy)]
enum Double {
    Gb2312,
    JisX0208,
    KsC5601,
    JisX0212,
}

impl Charset {
    fn of_94(last: u8) -> Self {
        match last {
            b'B' | b'J' => Self::Ascii,
            b'I' => Self::Katakana,
            _ => Self::Unknown(1),
        }
    }

    fn of_96(last: u8) -> Self {
        match last {
            b'A' => Self::Latin1,
            b'B' => Self::Single(encoding_rs::ISO_8859_2),
            b'C' => Self::Single(encoding_rs::ISO_8859_3),
            b'D' => Self::Single(encoding_rs::ISO_8859_4),
            b'F' => Self::Single(encoding_rs::ISO_8859_7),
            b'G' => Self::Single(encoding_rs::ISO_8859_6),
            b'H' => Self::Single(encoding_rs::ISO_8859_8),
            b'L' => Self::Single(encoding_rs::ISO_8859_5),
            // ISO 8859-9 and TIS-620: the same as these from 0xA0 up, all a right half holds.
            b'M' => Self::Single(encoding_rs::WINDOWS_1254),
            b'T' => Self::Single(encoding_rs::WINDOWS_874),
            b'V' => Self::Single(encoding_rs::ISO_8859_10),
            b'Y' => Self::Single(encoding_rs::ISO_8859_13),
            b'_' => Self::Single(encoding_rs::ISO_8859_14),
            b'b' => Self::Single(encoding_rs::ISO_8859_15),
            b'f' => Self::Single(encoding_rs::ISO_8859_16),
            _ => Self::Unknown(1),
        }
    }

    fn of_94x94(last: u8) -> Self {
        match last {
            b'A' => Self::Double(Double::Gb2312),
            b'@' | b'B' => Self::Double(Double::JisX0208),
            b'C' => Self::Double(Double::KsC5601),
            b'D' => Self::Double(Double::JisX0212),
            _ => Self::Unknown(2),
        }
    }

    /// Decodes the character at the start of `rest` (never empty) and returns its length.
    fn decode(self, rest: &[u8], out: &mut String) -> usize {
        let first = rest[0];
        match self {
            Self::Ascii => {
                out.push(char::from(first & 0x7F));
                1
            }
            Self::Latin1 => {
                out.push(char::from(first | 0x80));
                1
            }
            Self::Single(encoding) => {
                out.push_str(&encoding.decode_without_bom_handling(&[first | 0x80]).0);
                1
            }
            Self::Katakana => {
                let index = u32::from(first & 0x7F);
                let character = (0x21..=0x5F)
                    .contains(&index)
                    .then(|| char::from_u32(0xFF61 + index - 0x21))
                    .flatten();
                out.push(character.unwrap_or(char::REPLACEMENT_CHARACTER));
                1
            }
            Self::Double(set) => {
                // Both bytes of a character come from the same half.
                let Some(&second) = rest.get(1).filter(|&&second| (first ^ second) & 0x80 == 0)
                else {
                    out.push(char::REPLACEMENT_CHARACTER);
                    return 1;
                };
                let pair = [first | 0x80, second | 0x80];
                let text = match set {
                    Double::Gb2312 => encoding_rs::GBK.decode_without_bom_handling(&pair).0,
                    Double::KsC5601 => encoding_rs::EUC_KR.decode_without_bom_handling(&pair).0,
                    Double::JisX0208 => encoding_rs::EUC_JP.decode_without_bom_handling(&pair).0,
                    Double::JisX0212 => {
                        let triple = [0x8F, pair[0], pair[1]];
                        out.push_str(&encoding_rs::EUC_JP.decode_without_bom_handling(&triple).0);
                        return 2;
                    }
                };
                out.push_str(&text);
                2
            }
            Self::Unknown(width) => {
                out.push(char::REPLACEMENT_CHARACTER);
                width.min(rest.len())
            }
        }
    }
}

/// Decodes COMPOUND_TEXT. Text without an escape sequence that is valid UTF-8 is taken as
/// UTF-8, which is what some input methods send in spite of the name.
pub(super) fn decode(bytes: &[u8]) -> String {
    if !bytes.contains(&ESC)
        && let Ok(text) = std::str::from_utf8(bytes)
    {
        return text.to_owned();
    }

    let mut out = String::with_capacity(bytes.len());
    let mut low = Charset::Ascii;
    let mut high = Charset::Latin1;
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        at += match byte {
            ESC => escape(&bytes[at..], &mut low, &mut high, &mut out),
            // A direction mark: parameters, then one final byte.
            CSI => {
                let rest = &bytes[at + 1..];
                let end = rest.iter().position(|byte| (0x40..=0x7E).contains(byte));
                1 + end.map_or(rest.len(), |end| end + 1)
            }
            b'\t' | b'\n' | b' ' => {
                out.push(char::from(byte));
                1
            }
            // The other control characters are not text.
            0x00..=0x1F | 0x7F..=0x9F => 1,
            0x21..=0x7E => low.decode(&bytes[at..], &mut out),
            0xA0..=0xFF => high.decode(&bytes[at..], &mut out),
        };
    }
    out
}

/// Takes the escape sequence at the start of `bytes` and returns its length.
fn escape(bytes: &[u8], low: &mut Charset, high: &mut Charset, out: &mut String) -> usize {
    let rest = &bytes[1..];
    let intermediates = rest
        .iter()
        .take_while(|byte| (0x20..=0x2F).contains(*byte))
        .count();
    let Some(&last) = rest.get(intermediates) else {
        return bytes.len();
    };
    let length = 1 + intermediates + 1;
    let after = &bytes[length..];
    match &rest[..intermediates] {
        b"(" => *low = Charset::of_94(last),
        b")" => *high = Charset::of_94(last),
        b"-" => *high = Charset::of_96(last),
        b"$" | b"$(" => *low = Charset::of_94x94(last),
        b"$)" => *high = Charset::of_94x94(last),
        // UTF-8 up to "ESC % @".
        b"%" if last == b'G' => {
            let end = after
                .windows(3)
                .position(|window| window == [ESC, b'%', b'@']);
            out.push_str(&String::from_utf8_lossy(
                &after[..end.unwrap_or(after.len())],
            ));
            return length + end.map_or(after.len(), |end| end + 3);
        }
        // An extended segment: two length bytes, the encoding's name, STX, the text.
        b"%/" => {
            let (Some(&more), Some(&less)) = (after.first(), after.get(1)) else {
                return bytes.len();
            };
            let size = usize::from(more & 0x7F) * 128 + usize::from(less & 0x7F);
            let segment = &after[2..(2 + size).min(after.len())];
            let name_end = segment.iter().position(|&byte| byte == STX);
            let (name, text) = match name_end {
                Some(end) => (&segment[..end], &segment[end + 1..]),
                None => (&[][..], segment),
            };
            match Encoding::for_label(name) {
                Some(encoding) => out.push_str(&encoding.decode_without_bom_handling(text).0),
                None => out.push_str(&String::from_utf8_lossy(text)),
            }
            return length + 2 + segment.len();
        }
        _ => {}
    }
    length
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn cyrillic_greek_and_accented_latin() {
        // What Xlib writes for these in a UTF-8 locale: a right half of ISO 8859-5, -7, -1.
        assert_eq!(decode(b"\x1b-L\xd1\xd2\xd6\xd7"), "бвжз");
        assert_eq!(decode(b"abc \x1b-L\xd1\xd2 1"), "abc бв 1");
        assert_eq!(decode(b"\x1b-F\xe1\xe2\xe3"), "αβγ");
        assert_eq!(decode(b"caf\x1b-A\xe9"), "café");
        assert_eq!(decode(b"\xa1\xb8\xc0\xd1\xe2\xf3"), "¡¸ÀÑâó");
        assert_eq!(decode(b"\x1b-B\xa1\xa3\xa5\xa6\xa9\xab"), "ĄŁĽŚŠŤ");
        assert_eq!(decode(b"\x1b-L\xd1\x1b-A\xe9\x1b-F\xe1"), "бéα");
    }

    #[test]
    fn hangul_in_either_half() {
        assert_eq!(
            decode(b"\x1b$(C\x33\x4d\x43\x56\x30\x6d\x3e\x5f"),
            "넌최고야"
        );
        assert_eq!(
            decode(b"\x1b$)C\xb3\xcd\xc3\xd6\xb0\xed\xbe\xdf"),
            "넌최고야"
        );
        assert_eq!(decode(b"a\x1b$)C\xb3\xcd b"), "a넌 b");
    }

    #[test]
    fn japanese_and_chinese() {
        assert_eq!(decode(&[27, 36, 40, 66, 69, 108, 53, 126]), "東京");
        assert_eq!(decode(b"\x1b$B\x45\x6c\x35\x7e\x1b(B!"), "東京!");
        assert_eq!(decode(b"\x1b)I\xb1\xb2"), "ｱｲ");
        assert_eq!(
            decode(b"\x1b$(A\x3a\x5c\x38\x5f\x50\x4b\x48\x4f\x4a\x36\x44\x63"),
            "很高兴认识你"
        );
        assert_eq!(
            decode(b"2026\x1b$(A\x44\x6a\x1b(B07\x1b$(A\x54\x42\x1b(B16\x1b$(A\x48\x55"),
            "2026年07月16日"
        );
    }

    #[test]
    fn utf8_segments_and_plain_utf8() {
        assert_eq!(decode("\x1b%G가나다\x1b%@".as_bytes()), "가나다");
        assert_eq!(decode("a\x1b%Gб😀\x1b%@b".as_bytes()), "aб😀b");
        assert_eq!(decode("\x1b%Gнет конца".as_bytes()), "нет конца");
        assert_eq!(decode("бвжз".as_bytes()), "бвжз");
        assert_eq!(decode(b""), "");
        assert_eq!(decode(b"(B) plain"), "(B) plain");
    }

    #[test]
    fn extended_segments() {
        let mut bytes = b"\x1b%/1\x80\x8bkoi8-r\x02\xc2\xd7\xd6\xda!".to_vec();
        assert_eq!(decode(&bytes), "бвжз!");
        bytes.truncate(8);
        decode(&bytes);
    }

    #[test]
    fn what_cannot_be_decoded_is_not_a_panic() {
        assert_eq!(decode(b"\x1b-Zab\xa1"), "ab\u{fffd}");
        assert_eq!(decode(b"\x1b$(C\x33"), "\u{fffd}");
        assert_eq!(decode(b"\x1b$)C\xb3a"), "\u{fffd}a");
        assert_eq!(decode(b"a\x9b1]b\x9b"), "ab");
        for input in [
            &b"\x1b"[..],
            b"\x1b%",
            b"\x1b%G",
            b"\x1b%G\x1b%",
            b"\x1b$",
            b"\x1b$(",
            b"\x1b%/",
            b"\x1b%/1",
            b"\x1b%/1\xff",
            b"\x1b%/1\xff\xff",
            b"\x1b%/1\xff\xffname",
            b"\x1b%@\xff\xfe",
            b"\x1b$(D\x22\x2f",
        ] {
            decode(input);
        }
    }

    #[test]
    fn every_short_sequence() {
        let interesting = [
            0x00, 0x02, 0x1B, 0x20, 0x24, 0x25, 0x28, 0x29, 0x2D, 0x2F, 0x31, 0x40, 0x41, 0x42,
            0x43, 0x47, 0x49, 0x4C, 0x7E, 0x7F, 0x80, 0x8F, 0x9B, 0xA0, 0xA1, 0xB3, 0xFF,
        ];
        let mut input = [0u8; 4];
        let count = interesting.len();
        for mut index in 0..count.pow(4) {
            for byte in &mut input {
                *byte = interesting[index % count];
                index /= count;
            }
            for length in 1..=input.len() {
                decode(&input[..length]);
            }
        }
    }
}
