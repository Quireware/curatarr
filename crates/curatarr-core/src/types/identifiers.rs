use crate::error::CoreError;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Isbn13(String);

impl Isbn13 {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(s: &str) -> Result<(), CoreError> {
        let digits: Vec<u8> = s
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<Vec<_>>()
            .iter()
            .map(|c| u8::try_from(u32::from(*c) - u32::from('0')).unwrap_or(0))
            .collect();

        if digits.len() != 13 {
            return Err(CoreError::InvalidIsbn {
                value: s.to_string(),
                reason: format!("expected 13 digits, got {}", digits.len()),
            });
        }

        let sum: u32 = digits
            .iter()
            .enumerate()
            .map(|(i, &d)| {
                let weight: u32 = if i % 2 == 0 { 1 } else { 3 };
                u32::from(d) * weight
            })
            .sum();

        if sum % 10 != 0 {
            return Err(CoreError::InvalidIsbn {
                value: s.to_string(),
                reason: "invalid checksum".into(),
            });
        }

        Ok(())
    }
}

impl TryFrom<&str> for Isbn13 {
    type Error = CoreError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::validate(s)?;
        let normalized: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
        Ok(Self(normalized))
    }
}

impl TryFrom<String> for Isbn13 {
    type Error = CoreError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::try_from(s.as_str())
    }
}

impl From<Isbn13> for String {
    fn from(isbn: Isbn13) -> Self {
        isbn.0
    }
}

impl fmt::Display for Isbn13 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}-{}-{}-{}-{}",
            &self.0[..3],
            &self.0[3..4],
            &self.0[4..7],
            &self.0[7..12],
            &self.0[12..13]
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Isbn10(String);

impl Isbn10 {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(s: &str) -> Result<(), CoreError> {
        let chars: Vec<char> = s.chars().filter(|c| c.is_ascii_alphanumeric()).collect();

        if chars.len() != 10 {
            return Err(CoreError::InvalidIsbn {
                value: s.to_string(),
                reason: format!("expected 10 characters, got {}", chars.len()),
            });
        }

        let sum: u32 = chars
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let val = if c == 'X' || c == 'x' {
                    10u32
                } else {
                    u32::from(c) - u32::from('0')
                };
                val * u32::try_from(10 - i).unwrap_or(0)
            })
            .sum();

        if sum % 11 != 0 {
            return Err(CoreError::InvalidIsbn {
                value: s.to_string(),
                reason: "invalid checksum".into(),
            });
        }

        // X only valid as final character
        if chars[..9].iter().any(|&c| c == 'X' || c == 'x') {
            return Err(CoreError::InvalidIsbn {
                value: s.to_string(),
                reason: "X only valid as check digit (last position)".into(),
            });
        }

        Ok(())
    }
}

impl TryFrom<&str> for Isbn10 {
    type Error = CoreError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::validate(s)?;
        let normalized: String = s
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_uppercase();
        Ok(Self(normalized))
    }
}

impl TryFrom<String> for Isbn10 {
    type Error = CoreError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::try_from(s.as_str())
    }
}

impl From<Isbn10> for String {
    fn from(isbn: Isbn10) -> Self {
        isbn.0
    }
}

impl fmt::Display for Isbn10 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}-{}-{}-{}",
            &self.0[..1],
            &self.0[1..4],
            &self.0[4..9],
            &self.0[9..10]
        )
    }
}

/// Convert ISBN-10 to ISBN-13 by prepending "978" and recalculating the check digit.
pub fn isbn10_to_isbn13(isbn10: &Isbn10) -> Isbn13 {
    let base = format!("978{}", &isbn10.0[..9]);
    let digits: Vec<u32> = base
        .chars()
        .map(|c| u32::from(c) - u32::from('0'))
        .collect();
    let sum: u32 = digits
        .iter()
        .enumerate()
        .map(|(i, &d)| {
            let weight = if i % 2 == 0 { 1u32 } else { 3u32 };
            d * weight
        })
        .sum();
    let check = (10 - (sum % 10)) % 10;
    let full = format!("{base}{check}");
    Isbn13(full)
}

/// Convert ISBN-13 to ISBN-10. Only valid for ISBNs starting with "978".
pub fn isbn13_to_isbn10(isbn13: &Isbn13) -> Result<Isbn10, CoreError> {
    if !isbn13.0.starts_with("978") {
        return Err(CoreError::InvalidIsbn {
            value: isbn13.0.clone(), // clone: serde field for error reporting
            reason: "only 978-prefix ISBNs can convert to ISBN-10".into(),
        });
    }

    let digits: Vec<u32> = isbn13.0[3..12]
        .chars()
        .map(|c| u32::from(c) - u32::from('0'))
        .collect();

    let sum: u32 = digits
        .iter()
        .enumerate()
        .map(|(i, &d)| d * u32::try_from(10 - i).unwrap_or(0))
        .sum();

    let check = (11 - (sum % 11)) % 11;
    let check_char = if check == 10 {
        'X'
    } else {
        char::from_digit(check, 10).unwrap_or('0')
    };

    let isbn10_str: String = digits
        .iter()
        .map(|d| char::from_digit(*d, 10).unwrap_or('0'))
        .collect::<String>()
        + &check_char.to_string();
    Ok(Isbn10(isbn10_str))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Asin(String);

impl Asin {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for Asin {
    type Error = CoreError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let trimmed = s.trim();
        if trimmed.len() != 10 {
            return Err(CoreError::InvalidIdentifier {
                value: s.to_string(),
                reason: format!("ASIN must be 10 characters, got {}", trimmed.len()),
            });
        }
        if !trimmed.starts_with('B') {
            return Err(CoreError::InvalidIdentifier {
                value: s.to_string(),
                reason: "ASIN must start with 'B'".into(),
            });
        }
        if !trimmed.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(CoreError::InvalidIdentifier {
                value: s.to_string(),
                reason: "ASIN must be alphanumeric".into(),
            });
        }
        Ok(Self(trimmed.to_uppercase()))
    }
}

impl TryFrom<String> for Asin {
    type Error = CoreError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::try_from(s.as_str())
    }
}

impl From<Asin> for String {
    fn from(asin: Asin) -> Self {
        asin.0
    }
}

impl fmt::Display for Asin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExternalId {
    OpenLibraryWork(String),
    OpenLibraryEdition(String),
    GoodreadsWork(String),
    GoodreadsBook(String),
    HardcoverId(String),
    ComicvineVolume(String),
    ComicvineIssue(String),
    AniListMedia(i32),
    MangaUpdatesSeries(String),
    MangaDexSeries(String),
    MyAnimeList(i32),
    LibraryThingWork(String),
    Oclc(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rstest::rstest;

    fn isbn13_check_digit(first_12: &[u8; 12]) -> u8 {
        let sum: u32 = first_12
            .iter()
            .enumerate()
            .map(|(i, &d)| {
                let w = if i % 2 == 0 { 1u32 } else { 3u32 };
                u32::from(d) * w
            })
            .sum();
        u8::try_from((10 - (sum % 10)) % 10).unwrap_or(0)
    }

    fn arb_valid_isbn13() -> impl Strategy<Value = String> {
        prop::array::uniform12(0u8..10u8).prop_map(|digits| {
            let check = isbn13_check_digit(&digits);
            let s: String = digits
                .iter()
                .chain(std::iter::once(&check))
                .map(|d| char::from_digit(u32::from(*d), 10).unwrap())
                .collect();
            s
        })
    }

    fn isbn10_check_digit(first_9: &[u8; 9]) -> char {
        let sum: u32 = first_9
            .iter()
            .enumerate()
            .map(|(i, &d)| u32::from(d) * u32::try_from(10 - i).unwrap_or(0))
            .sum();
        let check = (11 - (sum % 11)) % 11;
        if check == 10 {
            'X'
        } else {
            char::from_digit(check, 10).unwrap()
        }
    }

    fn arb_valid_isbn10() -> impl Strategy<Value = String> {
        prop::array::uniform9(0u8..10u8).prop_map(|digits| {
            let check = isbn10_check_digit(&digits);
            let s: String = digits
                .iter()
                .map(|d| char::from_digit(u32::from(*d), 10).unwrap())
                .collect::<String>()
                + &check.to_string();
            s
        })
    }

    proptest! {
        #[test]
        fn valid_isbn13_accepted(isbn in arb_valid_isbn13()) {
            let result = Isbn13::try_from(isbn.as_str());
            prop_assert!(result.is_ok(), "Valid ISBN-13 {} was rejected: {:?}", isbn, result.err());
        }

        #[test]
        fn valid_isbn10_accepted(isbn in arb_valid_isbn10()) {
            let result = Isbn10::try_from(isbn.as_str());
            prop_assert!(result.is_ok(), "Valid ISBN-10 {} was rejected: {:?}", isbn, result.err());
        }

        #[test]
        fn isbn13_serde_roundtrip(isbn in arb_valid_isbn13()) {
            let parsed = Isbn13::try_from(isbn.as_str()).unwrap();
            let json = serde_json::to_string(&parsed).unwrap();
            let back: Isbn13 = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(parsed, back);
        }

        #[test]
        fn isbn10_serde_roundtrip(isbn in arb_valid_isbn10()) {
            let parsed = Isbn10::try_from(isbn.as_str()).unwrap();
            let json = serde_json::to_string(&parsed).unwrap();
            let back: Isbn10 = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(parsed, back);
        }

        #[test]
        fn isbn10_to_isbn13_roundtrip(isbn in arb_valid_isbn10()) {
            let isbn10 = Isbn10::try_from(isbn.as_str()).unwrap();
            let isbn13 = isbn10_to_isbn13(&isbn10);
            // Verify the ISBN-13 is valid
            prop_assert!(Isbn13::try_from(isbn13.as_str()).is_ok());
            // Verify roundtrip back to ISBN-10
            let back = isbn13_to_isbn10(&isbn13).unwrap();
            prop_assert_eq!(isbn10, back);
        }

        #[test]
        fn random_bytes_do_not_panic_isbn13(s in "\\PC{0,20}") {
            let _ = Isbn13::try_from(s.as_str());
        }

        #[test]
        fn random_bytes_do_not_panic_isbn10(s in "\\PC{0,20}") {
            let _ = Isbn10::try_from(s.as_str());
        }
    }

    #[rstest]
    #[case("978-0-306-40615-7", true)]
    #[case("9780306406157", true)]
    #[case("978-3-16-148410-0", true)]
    #[case("978-0-306-40615-0", false)]
    #[case("123", false)]
    #[case("", false)]
    fn isbn13_known_values(#[case] input: &str, #[case] valid: bool) {
        assert_eq!(Isbn13::try_from(input).is_ok(), valid, "ISBN-13 {input}");
    }

    #[rstest]
    #[case("0-306-40615-2", true)]
    #[case("0306406152", true)]
    #[case("007462542X", true)]
    #[case("0306406150", false)]
    #[case("123", false)]
    fn isbn10_known_values(#[case] input: &str, #[case] valid: bool) {
        assert_eq!(Isbn10::try_from(input).is_ok(), valid, "ISBN-10 {input}");
    }

    #[rstest]
    #[case("B08N5WRWNW", true)]
    #[case("B000000001", true)]
    #[case("B08n5wrwnw", true)]
    #[case("A08N5WRWNW", false)]
    #[case("B08N5", false)]
    #[case("B08N5WRWNW1", false)]
    fn asin_validation(#[case] input: &str, #[case] valid: bool) {
        assert_eq!(Asin::try_from(input).is_ok(), valid, "ASIN {input}");
    }

    #[test]
    fn isbn13_display_formats_with_hyphens() {
        let isbn = Isbn13::try_from("9780306406157").unwrap();
        assert_eq!(isbn.to_string(), "978-0-306-40615-7");
    }

    #[test]
    fn isbn13_to_isbn10_rejects_979_prefix() {
        let isbn = Isbn13::try_from("9790000000001").unwrap();
        assert!(isbn13_to_isbn10(&isbn).is_err());
    }

    #[test]
    fn external_id_serde_roundtrip() {
        let ids = vec![
            ExternalId::OpenLibraryWork("OL12345W".into()),
            ExternalId::AniListMedia(42),
            ExternalId::MangaDexSeries("abc-123".into()),
        ];
        for id in &ids {
            let json = serde_json::to_string(id).unwrap();
            let back: ExternalId = serde_json::from_str(&json).unwrap();
            assert_eq!(*id, back);
        }
    }
}
