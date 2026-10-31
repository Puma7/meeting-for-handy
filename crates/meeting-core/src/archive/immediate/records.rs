//! Identity-only recovery of interrupted atomic JSON; quoted content is never attribution.
use super::*;
use serde::de::{DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use std::collections::BTreeSet;

struct Identities<'a> {
    found: &'a mut BTreeSet<String>,
    depth: usize,
    root_only: bool,
    identity: bool,
}
impl<'de> DeserializeSeed<'de> for Identities<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> std::result::Result<(), D::Error> {
        if self.depth > 64 {
            return Err(serde::de::Error::custom(
                "Atomic record nesting exceeds bound",
            ));
        }
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Identities<'_> {
    type Value = ();
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("bounded atomic JSON with complete meetingId string fields")
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<(), E> {
        if self.identity {
            self.found.insert(value.into());
        }
        Ok(())
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<(), E> {
        Ok(())
    }
    /// Only meetingId fields contribute identity evidence, including plan candidate rows.
    /// Foreign identity evidence does not grant permission to erase that foreign meeting.
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<(), A::Error> {
        let mut identity_seen = false;
        while let Some(key) = map.next_key::<String>()? {
            if key == "meetingId" && std::mem::replace(&mut identity_seen, true) {
                return Err(serde::de::Error::custom(
                    "Duplicate atomic meeting identity",
                ));
            }
            map.next_value_seed(Identities {
                found: &mut *self.found,
                depth: self.depth + 1,
                root_only: self.root_only,
                identity: key == "meetingId" && (!self.root_only || self.depth == 0),
            })?;
        }
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<(), A::Error> {
        while seq
            .next_element_seed(Identities {
                found: &mut *self.found,
                depth: self.depth + 1,
                root_only: self.root_only,
                identity: false,
            })?
            .is_some()
        {}
        Ok(())
    }
}

/// A valid complete field in a syntactically valid prefix survives only EOF truncation.
/// Unknown/ambiguous bytes remain untouched and are counted for explicit preview disclosure.
pub(super) fn temporary_identities(
    path: &Path,
    root_only: bool,
) -> Result<Option<BTreeSet<String>>> {
    regular(path)?;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_MANIFEST + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_MANIFEST {
        return Err("Oversized interrupted metadata; erasure blocked".into());
    }
    let mut found = BTreeSet::new();
    let mut decoder = serde_json::Deserializer::from_slice(&bytes);
    let parsed = Identities {
        found: &mut found,
        depth: 0,
        root_only,
        identity: false,
    }
    .deserialize(&mut decoder)
    .and_then(|()| decoder.end());
    if parsed.is_err_and(|error| !error.is_eof()) || found.is_empty() {
        return Ok(None);
    }
    Ok(Some(found))
}

/// Temporary names receive identity-only parsing; normal malformed records still fail closed.
pub(super) fn is_temporary(path: &Path) -> Result<bool> {
    Ok(path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid retained record name")?
        .starts_with('.'))
}
