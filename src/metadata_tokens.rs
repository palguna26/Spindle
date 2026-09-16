use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    value: String,
    expires_at: Option<Instant>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MetadataTokens {
    entries: HashMap<String, Token>,
}

pub(crate) const MAX_KEYS: usize = 32;
pub(crate) const MAX_TTL_MS: u64 = 86_400_000;
const MAX_KEYS_PER_REQUEST: usize = 16;
const MAX_SOURCE_CHARS: usize = 80;
const MAX_KEY_CHARS: usize = 32;
const MAX_VALUE_CHARS: usize = 80;

pub(crate) fn normalize_source(source: String) -> Result<String, String> {
    let source = source.trim();
    if source.is_empty() {
        return Err("metadata source must not be empty".into());
    }
    if source.chars().count() > MAX_SOURCE_CHARS {
        return Err("metadata source must be 80 characters or fewer".into());
    }
    if !source
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ':' | '.' | '_' | '-'))
    {
        return Err("metadata source contains unsupported characters".into());
    }
    Ok(source.into())
}

pub(crate) fn normalize_patch(
    patch: HashMap<String, Option<String>>,
) -> Result<HashMap<String, Option<String>>, String> {
    if patch.len() > MAX_KEYS_PER_REQUEST {
        return Err(format!(
            "a metadata report may update at most {MAX_KEYS_PER_REQUEST} tokens"
        ));
    }
    patch
        .into_iter()
        .map(|(key, value)| {
            if key.is_empty()
                || key.chars().count() > MAX_KEY_CHARS
                || !key
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
            {
                return Err(format!("invalid metadata token key: {key}"));
            }
            let value = value.and_then(|value| {
                let value = value
                    .trim()
                    .chars()
                    .filter(|ch| !ch.is_control())
                    .take(MAX_VALUE_CHARS)
                    .collect::<String>();
                (!value.trim().is_empty()).then(|| value.trim().to_owned())
            });
            Ok((key, value))
        })
        .collect()
}

impl MetadataTokens {
    pub(crate) fn patch(
        &mut self,
        patch: HashMap<String, Option<String>>,
        ttl: Option<Duration>,
        now: Instant,
    ) -> bool {
        let expires_at = ttl.and_then(|ttl| now.checked_add(ttl));
        let mut changed = false;
        for (key, value) in patch {
            match value {
                Some(value) => {
                    let next = Token { value, expires_at };
                    if self.entries.get(&key) != Some(&next) {
                        self.entries.insert(key, next);
                        changed = true;
                    }
                }
                None => changed |= self.entries.remove(&key).is_some(),
            }
        }
        changed
    }

    pub(crate) fn key_count_after_patch(&self, patch: &HashMap<String, Option<String>>) -> usize {
        let mut keys = self
            .entries
            .keys()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        for (key, value) in patch {
            if value.is_some() {
                keys.insert(key.clone());
            } else {
                keys.remove(key);
            }
        }
        keys.len()
    }

    pub(crate) fn values(&self) -> HashMap<String, String> {
        self.entries
            .iter()
            .map(|(key, token)| (key.clone(), token.value.clone()))
            .collect()
    }

    pub(crate) fn expire_at(&mut self, now: Instant) -> bool {
        let before = self.entries.len();
        self.entries
            .retain(|_, token| token.expires_at.is_none_or(|deadline| deadline > now));
        self.entries.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_herdr_metadata_source_and_patch() {
        assert_eq!(
            normalize_source("  herdr:agent  ".into()).unwrap(),
            "herdr:agent"
        );
        assert!(normalize_source("bad source".into()).is_err());
        let patch = normalize_patch(HashMap::from([(
            "summary".into(),
            Some("  indexing\n".into()),
        )]))
        .unwrap();
        assert_eq!(patch["summary"].as_deref(), Some("indexing"));
    }

    #[test]
    fn patches_and_expires_tokens_like_herdr() {
        let now = Instant::now();
        let mut tokens = MetadataTokens::default();
        let patch = HashMap::from([("summary".into(), Some("indexing".into()))]);
        assert!(tokens.patch(patch, Some(Duration::from_secs(1)), now));
        assert_eq!(tokens.values()["summary"], "indexing");
        assert!(!tokens.expire_at(now + Duration::from_millis(999)));
        assert!(tokens.expire_at(now + Duration::from_secs(1)));
        assert!(tokens.values().is_empty());
    }

    #[test]
    fn clearing_one_token_preserves_the_rest() {
        let now = Instant::now();
        let mut tokens = MetadataTokens::default();
        tokens.patch(
            HashMap::from([
                ("summary".into(), Some("indexing".into())),
                ("model".into(), Some("codex".into())),
            ]),
            None,
            now,
        );
        tokens.patch(HashMap::from([("model".into(), None)]), None, now);
        assert_eq!(
            tokens.values(),
            HashMap::from([("summary".into(), "indexing".into())])
        );
    }
}
