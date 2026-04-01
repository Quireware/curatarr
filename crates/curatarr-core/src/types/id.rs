use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone, thiserror::Error)]
#[error("invalid entity ID: {0}")]
pub struct ParseIdError(String);

macro_rules! entity_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            pub fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            pub fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = ParseIdError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s)
                    .map(Self)
                    .map_err(|e| ParseIdError(e.to_string()))
            }
        }
    };
}

entity_id!(WorkId);
entity_id!(EditionId);
entity_id!(AuthorId);
entity_id!(SeriesId);
entity_id!(SeriesEntryId);
entity_id!(PublisherId);
entity_id!(CollectionId);
entity_id!(TagId);
entity_id!(FileId);

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_uuid() -> impl Strategy<Value = Uuid> {
        any::<[u8; 16]>().prop_map(Uuid::from_bytes)
    }

    macro_rules! id_tests {
        ($mod_name:ident, $id_type:ident) => {
            mod $mod_name {
                use super::*;

                proptest! {
                    #[test]
                    fn serde_roundtrip(uuid in arb_uuid()) {
                        let id = $id_type::from_uuid(uuid);
                        let json = serde_json::to_string(&id).unwrap();
                        let back: $id_type = serde_json::from_str(&json).unwrap();
                        prop_assert_eq!(id, back);
                    }

                    #[test]
                    fn display_fromstr_roundtrip(uuid in arb_uuid()) {
                        let id = $id_type::from_uuid(uuid);
                        let display = id.to_string();
                        let back: $id_type = display.parse().unwrap();
                        prop_assert_eq!(id, back);
                    }
                }

                #[test]
                fn new_produces_unique_ids() {
                    let ids: Vec<$id_type> = (0..100).map(|_| $id_type::new()).collect();
                    let unique: std::collections::HashSet<_> = ids.iter().collect();
                    assert_eq!(ids.len(), unique.len());
                }

                #[test]
                fn fromstr_rejects_garbage() {
                    assert!("not-a-uuid".parse::<$id_type>().is_err());
                    assert!("".parse::<$id_type>().is_err());
                    assert!("12345".parse::<$id_type>().is_err());
                }
            }
        };
    }

    id_tests!(work_id, WorkId);
    id_tests!(edition_id, EditionId);
    id_tests!(author_id, AuthorId);
    id_tests!(series_id, SeriesId);
    id_tests!(series_entry_id, SeriesEntryId);
    id_tests!(publisher_id, PublisherId);
    id_tests!(collection_id, CollectionId);
    id_tests!(tag_id, TagId);
    id_tests!(file_id, FileId);
}
