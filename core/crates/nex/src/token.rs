//! Tokens for the NEX login.
//!
//! The game logs in with its account ID (on Nextendo: the account's PID) as the user name and,
//! in the login's extra data, an ID token: a JWT whose `nnex` claim carries Nextendo's game
//! token (`GET /api/nex-token`, 24 h) and whose `tv` claim is the game version. Nextendo's game
//! servers check the game token in `nnex` (its HMAC, expiry and PID), not the JWT's own
//! signature. SMCraft signs its ID token with a key of its own (HS256), never one of
//! Nextendo's or Nintendo's.
//!
//! [`nx2_token`] makes game tokens in Nextendo's published shape for a local test server; it
//! is never used against Nextendo (their tokens come from their account server only).

use serde_json::json;

use crate::crypto::{b64url, hmac_sha256, random_bytes};

/// The game version Nextendo expects for SMM2.
pub const GAME_VERSION: &str = "3.0.3";

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// An ID token for the NEX login carrying `nex_token`, valid three hours.
pub fn id_token(pid: u64, nex_token: &str, key: &[u8]) -> String {
    let header = json!({"alg": "HS256", "typ": "JWT", "kid": "smcraft"});
    let t = now();
    let claims = json!({
        "iss": "smcraft",
        "sub": format!("{pid:016x}"),
        "aud": "smcraft-nex",
        "iat": t,
        "exp": t + 3 * 3600,
        "jti": b64url(&random_bytes(12)),
        "typ": "id_token",
        "tv": GAME_VERSION,
        "nnex": nex_token,
    });
    let signing = format!(
        "{}.{}",
        b64url(header.to_string().as_bytes()),
        b64url(claims.to_string().as_bytes())
    );
    let sig = b64url(&hmac_sha256(key, signing.as_bytes()));
    format!("{signing}.{sig}")
}

/// A game token in Nextendo's shape, `nx2.` + b64url("pid.username.expiry") + `.` +
/// b64url(HMAC-SHA256(secret, "nex:" + payload)), for a local test server holding `secret`.
pub fn nx2_token(secret: &[u8], pid: u64, username: &str, expiry_unix: u64) -> String {
    let payload = format!("{pid}.{username}.{expiry_unix}");
    let sig = hmac_sha256(secret, format!("nex:{payload}").as_bytes());
    format!("nx2.{}.{}", b64url(payload.as_bytes()), b64url(&sig))
}

/// [`nx2_token`] valid for a day from now.
pub fn nx2_token_for_a_day(secret: &[u8], pid: u64, username: &str) -> String {
    nx2_token(secret, pid, username, now() + 86_400)
}

/// The claims of a JWT (unverified), for tests and diagnostics.
pub fn jwt_claims(token: &str) -> Option<serde_json::Value> {
    let part = token.split('.').nth(1)?;
    serde_json::from_slice(&crate::crypto::unb64(part)?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::unb64;

    #[test]
    fn id_token_carries_the_game_token() {
        let t = id_token(42, "nx2.abc.def", b"k");
        assert_eq!(t.split('.').count(), 3);
        let c = jwt_claims(&t).unwrap();
        assert_eq!(c["nnex"], "nx2.abc.def");
        assert_eq!(c["tv"], GAME_VERSION);
        assert!(
            t.starts_with("eyJ"),
            "the server finds the JWT by its eyJ prefix"
        );
    }

    #[test]
    fn nx2_shape() {
        let t = nx2_token(b"secret", 7, "mario", 1_900_000_000);
        let parts: Vec<&str> = t.splitn(3, '.').collect();
        assert_eq!(parts[0], "nx2");
        assert_eq!(unb64(parts[1]).unwrap(), b"7.mario.1900000000");
        assert_eq!(
            unb64(parts[2]).unwrap(),
            hmac_sha256(b"secret", b"nex:7.mario.1900000000")
        );
    }
}
