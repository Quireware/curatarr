use serde::{Deserialize, Deserializer};

/// Deserialize an `Option<Option<T>>` update field so that a JSON `null` means
/// "clear the value" (`Some(None)`) while an absent key means "leave unchanged" (`None`).
/// Pair with `#[serde(default, deserialize_with = "double_option")]`.
pub fn double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Deserialize)]
    struct Update {
        #[serde(default, deserialize_with = "double_option")]
        note: Option<Option<String>>,
    }

    #[test]
    fn absent_key_is_unchanged() {
        let u: Update = serde_json::from_str("{}").unwrap();
        assert_eq!(u.note, None);
    }

    #[test]
    fn null_clears() {
        let u: Update = serde_json::from_str(r#"{"note": null}"#).unwrap();
        assert_eq!(u.note, Some(None));
    }

    #[test]
    fn value_sets() {
        let u: Update = serde_json::from_str(r#"{"note": "hi"}"#).unwrap();
        assert_eq!(u.note, Some(Some("hi".into())));
    }
}
