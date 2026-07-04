//! Optional loopback bearer token (Phase 9, ARCHITECTURE §11).
//!
//! The daemon binds to `127.0.0.1` only, so remote machines can't reach it. A
//! token adds a second layer: it stops *other local processes* (or a malicious
//! page abusing a permissive local CORS setup) from driving the daemon. It is
//! **off by default** and enabled by setting `UDM_AUTH_TOKEN`. Clients pass it
//! as a `?token=...` query parameter on the WebSocket URL (browsers can't set
//! arbitrary request headers on a WS handshake, but they can set the query).

/// Read the configured token from the environment, if any (empty = disabled).
pub fn token_from_env() -> Option<String> {
    std::env::var("UDM_AUTH_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
}

/// Extract the `token` query parameter from a request URI / path string.
pub fn token_in_uri(uri: &str) -> Option<String> {
    let query = uri.split('?').nth(1)?;
    for pair in query.split('&') {
        let mut kv = pair.splitn(2, '=');
        if kv.next() == Some("token") {
            return kv.next().map(|s| s.to_string());
        }
    }
    None
}

/// Length-checked, constant-time-ish comparison of the supplied token against
/// the expected one (avoids early-exit timing leaks on the byte compare).
pub fn token_matches(expected: &str, got: Option<&str>) -> bool {
    let Some(got) = got else { return false };
    if got.len() != expected.len() {
        return false;
    }
    let diff = got
        .bytes()
        .zip(expected.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b));
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_token_query() {
        assert_eq!(token_in_uri("/?token=abc123").as_deref(), Some("abc123"));
        assert_eq!(token_in_uri("/ws?foo=1&token=xyz").as_deref(), Some("xyz"));
        assert_eq!(token_in_uri("/no-query"), None);
        assert_eq!(token_in_uri("/?other=1"), None);
    }

    #[test]
    fn matches_only_exact_token() {
        assert!(token_matches("secret", Some("secret")));
        assert!(!token_matches("secret", Some("secre")));
        assert!(!token_matches("secret", Some("Secret")));
        assert!(!token_matches("secret", None));
    }
}
