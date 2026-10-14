use anyhow::{bail, Result};
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

const NAME_PREFIX: &str = "agent-dash-";
const SUFFIX_LEN: usize = 4;
const SUFFIX_ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
const MAX_ATTEMPTS: usize = 20;

/// Lowercase, with every run of non-alphanumeric characters collapsed to one `-`.
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(c.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    if out.is_empty() {
        "session".to_string()
    } else {
        out
    }
}

/// `agent-dash-<slug(label)>`
pub fn base_name(label: &str) -> String {
    format!("{NAME_PREFIX}{}", slug(label))
}

/// Maps entropy to `SUFFIX_LEN` characters from `[a-z0-9]`.
pub fn suffix_from(mut entropy: u64) -> String {
    let base = SUFFIX_ALPHABET.len() as u64;
    (0..SUFFIX_LEN)
        .map(|_| {
            let c = SUFFIX_ALPHABET[(entropy % base) as usize] as char;
            entropy /= base;
            c
        })
        .collect()
}

pub fn random_suffix() -> String {
    // A fresh RandomState is seeded randomly per process, and hashing the clock on top keeps
    // successive calls distinct without a rand dependency.
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is before the unix epoch")
            .as_nanos(),
    );
    suffix_from(hasher.finish())
}

/// `base` if free, otherwise `base-<random suffix>` with the first suffix that is free.
pub async fn resolve_conflict(
    base: &str,
    mut is_taken: impl AsyncFnMut(&str) -> Result<bool>,
    mut next_suffix: impl FnMut() -> String,
) -> Result<String> {
    if !is_taken(base).await? {
        return Ok(base.to_string());
    }
    for _ in 0..MAX_ATTEMPTS {
        let candidate = format!("{base}-{}", next_suffix());
        if !is_taken(&candidate).await? {
            return Ok(candidate);
        }
    }
    bail!("no free session name for {base}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn slug_lowercases_and_collapses_separators() {
        assert_eq!(slug("Fix the Bug!"), "fix-the-bug");
        assert_eq!(slug("a__b  c--d"), "a-b-c-d");
        assert_eq!(slug("  --Hello/World.rs--  "), "hello-world-rs");
        assert_eq!(slug("v2"), "v2");
    }

    #[test]
    fn slug_of_nothing_usable_is_session() {
        assert_eq!(slug(""), "session");
        assert_eq!(slug("---"), "session");
        assert_eq!(slug("日本語"), "session");
    }

    #[test]
    fn base_name_is_prefixed_slug() {
        assert_eq!(base_name("My Title"), "agent-dash-my-title");
        assert_eq!(base_name(""), "agent-dash-session");
    }

    #[test]
    fn suffix_is_four_lowercase_alphanumerics() {
        for entropy in [0, 1, 35, 36, 123_456_789, u64::MAX] {
            let suffix = suffix_from(entropy);
            assert_eq!(suffix.len(), 4);
            assert!(suffix
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
        }
        assert_eq!(suffix_from(0), "aaaa");
        assert_ne!(suffix_from(1), suffix_from(2));
        let random = random_suffix();
        assert_eq!(random.len(), 4);
    }

    async fn resolve(base: &str, taken: &[&str], suffixes: &[&str]) -> Result<String> {
        let taken: HashSet<String> = taken.iter().map(|s| s.to_string()).collect();
        let mut suffixes = suffixes.iter().map(|s| s.to_string());
        resolve_conflict(
            base,
            async |name: &str| Ok(taken.contains(name)),
            || suffixes.next().expect("ran out of suffixes"),
        )
        .await
    }

    #[tokio::test]
    async fn free_name_is_used_as_is() {
        assert_eq!(
            resolve("agent-dash-x", &[], &[]).await.unwrap(),
            "agent-dash-x"
        );
    }

    #[tokio::test]
    async fn taken_name_gets_a_suffix() {
        assert_eq!(
            resolve("agent-dash-x", &["agent-dash-x"], &["ab12"])
                .await
                .unwrap(),
            "agent-dash-x-ab12"
        );
    }

    #[tokio::test]
    async fn taken_suffixed_name_retries_with_a_new_suffix() {
        assert_eq!(
            resolve(
                "agent-dash-x",
                &["agent-dash-x", "agent-dash-x-ab12"],
                &["ab12", "cd34"]
            )
            .await
            .unwrap(),
            "agent-dash-x-cd34"
        );
    }

    #[tokio::test]
    async fn gives_up_when_every_suffix_is_taken() {
        let result = resolve_conflict("n", async |_: &str| Ok(true), || "aaaa".to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn lookup_errors_propagate() {
        let result = resolve_conflict(
            "n",
            async |_: &str| bail!("tmux down"),
            || "aaaa".to_string(),
        )
        .await;
        assert!(result.is_err());
    }
}
