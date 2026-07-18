//! Keyed candidate names and their fixed-width nonces (PRD 10, Decision 2).
//!
//! Every temporary CrateVista name under `<output>`'s parent is built from two
//! fixed-width lowercase-hex fields:
//!
//! - the **output key** — exactly 16 lowercase-hex characters (see
//!   [`super::output_identity`]);
//! - the **nonce** — exactly 32 lowercase-hex characters, from an OS-random source.
//!
//! ```text
//! .cratevista-<key16>-staging-<nonce32>     a staging directory
//! .cratevista-<key16>-backup-<nonce32>      a saved previous <output>
//! .cratevista-<key16>.lock                  the per-output advisory lock
//! .cratevista-static-site.json.tmp-<nonce32>  a crash-safe marker temp
//! ```
//!
//! Recognition is **exact**: a name with any prefix, extra suffix, uppercase hex,
//! shortened field or path separator is **not** a CrateVista candidate and is left
//! untouched. This is what keeps recovery from ever deleting an unrelated directory
//! that merely looks similar.

/// The shared filename stem.
const PREFIX: &str = ".cratevista-";
/// The staging infix.
const STAGING_INFIX: &str = "-staging-";
/// The backup infix.
const BACKUP_INFIX: &str = "-backup-";
/// The marker-temp stem (a temp sits *inside* a candidate directory).
const MARKER_TEMP_PREFIX: &str = ".cratevista-static-site.json.tmp-";

/// The exact output-key width, in lowercase-hex characters.
pub const KEY_HEX_LEN: usize = 16;
/// The exact nonce width, in lowercase-hex characters.
pub const NONCE_HEX_LEN: usize = 32;

/// Whether `value` is exactly `len` lowercase-hexadecimal ASCII characters.
fn is_fixed_lower_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Whether `value` is a syntactically valid 16-hex output key.
pub fn is_valid_key(value: &str) -> bool {
    is_fixed_lower_hex(value, KEY_HEX_LEN)
}

/// Whether `value` is a syntactically valid 32-hex nonce.
pub fn is_valid_nonce(value: &str) -> bool {
    is_fixed_lower_hex(value, NONCE_HEX_LEN)
}

/// Generates a fresh 32-hex nonce from the OS CSPRNG.
///
/// Timestamp/PID are deliberately **not** used: two processes racing on the same
/// output must not collide, so the source is `getrandom`.
pub fn generate_nonce() -> String {
    let mut bytes = [0u8; NONCE_HEX_LEN / 2];
    // A CSPRNG read of 16 bytes does not fail on any supported target; if it ever
    // did, a still-unique fallback keeps names well-formed rather than panicking.
    if getrandom::fill(&mut bytes).is_err() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        bytes.copy_from_slice(&stamp.to_le_bytes());
    }
    let mut hex = String::with_capacity(NONCE_HEX_LEN);
    for byte in bytes {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// The exact staging directory name for `key` and `nonce`.
pub fn staging_name(key: &str, nonce: &str) -> String {
    format!("{PREFIX}{key}{STAGING_INFIX}{nonce}")
}

/// The exact backup directory name for `key` and `nonce`.
pub fn backup_name(key: &str, nonce: &str) -> String {
    format!("{PREFIX}{key}{BACKUP_INFIX}{nonce}")
}

/// The exact advisory-lock file name for `key`.
pub fn lock_name(key: &str) -> String {
    format!("{PREFIX}{key}.lock")
}

/// The kind of keyed candidate a filename denotes, **for one exact key**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Candidate {
    /// `.cratevista-<key>-staging-<nonce>`.
    Staging,
    /// `.cratevista-<key>-backup-<nonce>`.
    Backup,
}

/// Classifies `name` as a candidate **for this exact `key`**, requiring the exact
/// fixed-width format. Returns `None` for any other name (a different key, a bad
/// nonce, an extra prefix/suffix, or an unrelated file).
pub fn classify_for_key(name: &str, key: &str) -> Option<Candidate> {
    let rest = name.strip_prefix(PREFIX)?;
    // `<key><infix><nonce>` — the key must match exactly.
    let after_key = rest.strip_prefix(key)?;
    if let Some(nonce) = after_key.strip_prefix(STAGING_INFIX) {
        return is_valid_nonce(nonce).then_some(Candidate::Staging);
    }
    if let Some(nonce) = after_key.strip_prefix(BACKUP_INFIX) {
        return is_valid_nonce(nonce).then_some(Candidate::Backup);
    }
    None
}

/// Whether `name` is an **exact** marker-temp name (`.cratevista-static-site.json
/// .tmp-<nonce32>`), the only non-authoritative entry a P0 shell may contain.
pub fn is_marker_temp(name: &str) -> bool {
    match name.strip_prefix(MARKER_TEMP_PREFIX) {
        Some(nonce) => is_valid_nonce(nonce),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "0123456789abcdef";

    #[test]
    fn generated_nonces_are_32_lowercase_hex_and_vary() {
        let a = generate_nonce();
        let b = generate_nonce();
        assert!(is_valid_nonce(&a), "{a}");
        assert!(is_valid_nonce(&b), "{b}");
        assert_ne!(a, b, "two nonces must differ");
    }

    #[test]
    fn key_and_nonce_widths_are_exact() {
        assert!(is_valid_key(KEY));
        assert!(!is_valid_key("0123456789abcde")); // 15
        assert!(!is_valid_key("0123456789abcdef0")); // 17
        assert!(!is_valid_key("0123456789ABCDEF")); // uppercase
        assert!(is_valid_nonce(&"a".repeat(32)));
        assert!(!is_valid_nonce(&"a".repeat(31)));
        assert!(!is_valid_nonce(&"A".repeat(32)));
        assert!(!is_valid_nonce(&"g".repeat(32))); // non-hex
    }

    #[test]
    fn round_trips_staging_and_backup_names() {
        let nonce = generate_nonce();
        let staging = staging_name(KEY, &nonce);
        let backup = backup_name(KEY, &nonce);
        assert_eq!(classify_for_key(&staging, KEY), Some(Candidate::Staging));
        assert_eq!(classify_for_key(&backup, KEY), Some(Candidate::Backup));
    }

    #[test]
    fn a_name_for_another_key_is_never_a_candidate() {
        let nonce = generate_nonce();
        let other = staging_name("fedcba9876543210", &nonce);
        assert_eq!(classify_for_key(&other, KEY), None);
    }

    #[test]
    fn prefixes_suffixes_and_bad_nonces_are_rejected() {
        let nonce = generate_nonce();
        let good = staging_name(KEY, &nonce);
        assert_eq!(classify_for_key(&format!("x{good}"), KEY), None);
        assert_eq!(classify_for_key(&format!("{good}x"), KEY), None);
        assert_eq!(
            classify_for_key(&staging_name(KEY, "short"), KEY),
            None,
            "a shortened nonce is not a candidate"
        );
        assert_eq!(
            classify_for_key(&staging_name(KEY, &"A".repeat(32)), KEY),
            None,
            "uppercase hex is not a candidate"
        );
    }

    #[test]
    fn marker_temp_recognition_is_exact() {
        let nonce = generate_nonce();
        assert!(is_marker_temp(&format!(
            ".cratevista-static-site.json.tmp-{nonce}"
        )));
        assert!(!is_marker_temp(".cratevista-static-site.json"));
        assert!(!is_marker_temp(".cratevista-static-site.json.tmp-short"));
        assert!(!is_marker_temp(&format!(
            ".cratevista-static-site.json.tmp-{nonce}x"
        )));
        assert!(!is_marker_temp("unrelated.tmp-file"));
    }

    #[test]
    fn lock_name_is_keyed() {
        assert_eq!(lock_name(KEY), ".cratevista-0123456789abcdef.lock");
    }
}
