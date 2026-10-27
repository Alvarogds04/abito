//! Windows privacy policy, kept OS-free so Linux CI exercises it (W1).
//!
//! Node cannot set an ACL on the files and the pipe it creates, so the Rust host
//! decides what to trust: a pipe endpoint must have the exact shape the daemon
//! generates, and the runtime directory must be private to the current user.

/// `\\.\pipe\abito-<32 lowercase hex>`: the exact name `server.cjs` generates.
pub fn pipe_endpoint_ok(endpoint: &str) -> bool {
    endpoint.strip_prefix(r"\\.\pipe\abito-")
        .is_some_and(|hex| hex.len() == 32 && hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}

pub const SID_SYSTEM: &str = "S-1-5-18";
pub const SID_ADMINISTRATORS: &str = "S-1-5-32-544";

/// A directory is private when owned by the current user or Administrators
/// (the default owner for elevated tokens), and every understood DACL entry
/// names only the current user, SYSTEM or Administrators. A missing/NULL DACL
/// grants everyone access and is refused. Metadata files retain owner-only policy.
pub fn dacl_private(owner: &str, current_user: &str, allowed_entries: Option<&[String]>) -> bool {
    if owner != current_user && owner != SID_ADMINISTRATORS { return false }
    let Some(entries) = allowed_entries else { return false };
    entries.iter().all(|sid| sid == current_user || sid == SID_SYSTEM || sid == SID_ADMINISTRATORS)
}

/// Default island root on Windows: `%LOCALAPPDATA%\Abito`, which inherits a
/// user-only profile DACL. `ABITO_ISLAND_DIR` is honoured ONLY as a
/// parallel-island/test seam INSIDE that canonical protected root: an
/// override pointing anywhere else would bypass the profile ACLs that guard
/// the token file, so it is refused (`None`), never guessed.
/// OS-free containment (these verdicts run on Linux CI too): `candidate` is
/// accepted only when it IS the canonical root or sits inside it, with no
/// `.`/`..` and no empty path component that could climb out textually.
fn within_canonical(candidate: &str, canonical: &str) -> bool {
    let norm = |s: &str| s.replace('\\', "/");
    let (c, base) = (norm(candidate), norm(canonical));
    let base = base.trim_end_matches('/');
    if c == base {
        return true;
    }
    let Some(rest) = c.strip_prefix(base) else { return false };
    let Some(rest) = rest.strip_prefix('/') else { return false };
    rest.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
}

pub fn island_root(env_dir: Option<&str>, local_app_data: Option<&str>) -> Option<std::path::PathBuf> {
    let absolute = |s: &str| {
        let b = s.as_bytes();
        (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
            && !s.chars().any(char::is_control)
    };
    let canonical = local_app_data.filter(|d| absolute(d)).map(|d| std::path::Path::new(d).join("Abito"))?;
    if let Some(d) = env_dir.filter(|d| !d.is_empty()) {
        return (absolute(d) && within_canonical(d, &canonical.to_string_lossy()))
            .then(|| std::path::PathBuf::from(d));
    }
    Some(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    const ME: &str = "S-1-5-21-1-2-3-1001";
    fn v(x: &[&str]) -> Vec<String> { x.iter().map(|s| s.to_string()).collect() }

    #[test]
    fn pipe_names_have_one_exact_shape() {
        assert!(pipe_endpoint_ok(r"\\.\pipe\abito-0123456789abcdef0123456789abcdef"));
        for bad in [r"\\.\pipe\abito-0123", r"\\.\pipe\abito-0123456789ABCDEF0123456789ABCDEF", r"\\.\pipe\other-0123456789abcdef0123456789abcdef",
            r"\\server\pipe\abito-0123456789abcdef0123456789abcdef", r"\\.\pipe\abito-0123456789abcdef0123456789abcdef\x", "/tmp/p-abcdef/s", ""] {
            assert!(!pipe_endpoint_ok(bad), "{bad}");
        }
    }
    #[test]
    fn only_owner_system_and_admins_may_hold_access() {
        assert!(dacl_private(ME, ME, Some(&v(&[ME, SID_SYSTEM, SID_ADMINISTRATORS]))));
        assert!(dacl_private(ME, ME, Some(&v(&[ME]))));
        assert!(!dacl_private(ME, ME, Some(&v(&[ME, "S-1-1-0"])))); // Everyone
        assert!(!dacl_private(ME, ME, Some(&v(&[ME, "S-1-5-32-545"])))); // Users
        assert!(!dacl_private(ME, ME, Some(&v(&[ME, "S-1-5-21-1-2-3-1002"])))); // another user
    }
    #[test]
    fn elevated_profile_owner_is_private_but_broad_aces_are_not() {
        assert!(dacl_private(SID_ADMINISTRATORS, ME, Some(&v(&[SID_SYSTEM, SID_ADMINISTRATORS, ME]))));
        for broad in ["S-1-1-0", "S-1-5-32-545", "S-1-5-11", "S-1-5-21-1-2-3-1002"] {
            assert!(!dacl_private(SID_ADMINISTRATORS, ME, Some(&v(&[ME, SID_SYSTEM, broad]))));
        }
        assert!(!dacl_private(SID_SYSTEM, ME, Some(&v(&[ME]))));
        assert!(!dacl_private(SID_ADMINISTRATORS, ME, None));
    }
    #[test]
    fn a_foreign_owner_or_a_null_dacl_is_refused() {
        assert!(!dacl_private("S-1-5-21-1-2-3-1002", ME, Some(&v(&[ME]))));
        assert!(!dacl_private(ME, ME, None));
    }
    #[test]
    fn island_root_is_absolute_and_defaults_under_local_app_data() {
        let local = r"C:\Users\a\AppData\Local";
        assert_eq!(island_root(None, Some(local)).unwrap(), std::path::Path::new(local).join("Abito"));
        // An ABITO_ISLAND_DIR override is honoured ONLY inside the canonical
        // protected root: anything else would bypass the profile ACLs that
        // guard the token file, so it is refused, never guessed.
        assert_eq!(
            island_root(Some(r"C:\Users\a\AppData\Local\Abito\test-island"), Some(local)).unwrap(),
            std::path::PathBuf::from(r"C:\Users\a\AppData\Local\Abito\test-island")
        );
        assert_eq!(
            island_root(Some(r"C:\Users\a\AppData\Local\Abito"), Some(local)).unwrap(),
            std::path::PathBuf::from(r"C:\Users\a\AppData\Local\Abito")
        );
        for bad in [r"D:\x", r"rel\x", r"C:\Users\a\AppData\Local\AbitoEvil", r"C:\other\Abito", r"C:\Users\a\AppData\Local\Abito\\.."] {
            assert!(island_root(Some(bad), Some(local)).is_none(), "{bad}");
        }
        assert!(island_root(None, Some("rel")).is_none());
        assert!(island_root(None, None).is_none());
    }
}
