//! abito-private-session — native Windows writer for the daemon session file
//! (T2/S6, decision L6).
//!
//! The Node daemon cannot set a Windows ACL. The launch token lives in
//! `session.json`, so that file is written HERE, natively:
//!
//! * Input: ONE bounded JSON object on stdin. The token NEVER travels on the
//!   command line, in the environment or in a log.
//! * Output path: exactly `<island>/runtime/session.json`, where `<island>` is
//!   the canonical protected `%LOCALAPPDATA%\Abito` root — or an
//!   `ABITO_ISLAND_DIR` override INSIDE it (`winpolicy::island_root` refuses
//!   anything else). No path ever comes from the input.
//! * Write: `winipc::ffi::private_file_bytes` — CREATE_NEW temp with an
//!   explicit protected owner-only DACL AT CREATION (O:<sid>D:P(A;;GA;;;<sid>)),
//!   then an atomic replace. Node's chmod 0600 is not an ACL; this is.
//! * Validation: closed field set, shaped endpoint, bounded token. Reparse
//!   points anywhere in the runtime path and a non-private runtime directory
//!   are refused. Everything fails closed; errors are fixed strings (never
//!   echoed input, never the token).
//!
//! No GUI, no arguments, no output on stdout except a bare `ok` on success.
use std::io::Read as _;
use std::io::Write as _;
#[cfg(windows)]
use abito_island::winipc;
use abito_island::winpolicy;
use serde_json::Value;

const PROTOCOL: &str = "island-agent/1";
/// Bounded stdin: the session payload is a few hundred bytes at most.
const MAX_INPUT: usize = 4096;
const MIN_TOKEN: usize = 16;
const MAX_TOKEN: usize = 512;

fn fail(message: &str) -> i32 {
    let _ = writeln!(std::io::stderr(), "abito-private-session: {message}");
    1
}

/// The stdin JSON with a CLOSED field set and shaped values; returns the
/// canonical bytes to write (single line + LF, matching the Unix writer).
fn validated_bytes(input: &str) -> Result<Vec<u8>, &'static str> {
    let value: Value = serde_json::from_str(input).map_err(|_| "input is not one JSON object")?;
    let map = value.as_object().ok_or("input is not one JSON object")?;
    let protocol = map.get("protocol").and_then(Value::as_str).ok_or("protocol missing")?;
    if protocol != PROTOCOL {
        return Err("unknown protocol");
    }
    let endpoint = map.get("endpoint").and_then(Value::as_str).ok_or("endpoint missing")?;
    if !winpolicy::pipe_endpoint_ok(endpoint) {
        return Err("endpoint is not this daemon's pipe shape");
    }
    let token = map.get("token").and_then(Value::as_str).ok_or("token missing")?;
    let token_ok = (MIN_TOKEN..=MAX_TOKEN).contains(&token.len())
        && token.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if !token_ok {
        return Err("token is not a bounded base64url string");
    }
    let pid = map.get("pid").and_then(Value::as_u64).ok_or("pid missing")?;
    if pid <= 1 || pid > u64::from(u32::MAX) {
        return Err("pid out of range");
    }
    let proc_start = match map.get("procStart") {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) if !text.is_empty() && text.len() <= 32 && text.bytes().all(|b| b.is_ascii_digit()) => {
            Some(Value::String(text.clone()))
        }
        Some(_) => return Err("procStart is not a bounded decimal string"),
    };
    let started_at = map.get("startedAt").and_then(Value::as_f64).ok_or("startedAt missing")?;
    if !(0.0..=9.0e15).contains(&started_at) || started_at.fract() != 0.0 {
        return Err("startedAt is not a bounded integer timestamp");
    }
    let ppid = match map.get("ppid") {
        None | Some(Value::Null) => None,
        Some(Value::Number(n)) => {
            let p = n.as_u64().ok_or("ppid is not an integer")?;
            if p > u64::from(u32::MAX) {
                return Err("ppid out of range");
            }
            Some(Value::from(p as u32))
        }
        Some(_) => return Err("ppid is not an integer"),
    };
    // Closed key set: exactly these fields, nothing else.
    let known = ["protocol", "endpoint", "token", "pid", "procStart", "startedAt", "ppid"];
    if map.keys().any(|k| !known.contains(&k.as_str())) {
        return Err("unknown field in session payload");
    }
    let canonical = serde_json::json!({
        "protocol": protocol,
        "endpoint": endpoint,
        "token": token,
        "pid": pid as u32,
        "procStart": proc_start,
        "startedAt": started_at as u64,
        "ppid": ppid,
    });
    let mut bytes = serde_json::to_vec(&canonical).map_err(|_| "canonical serialization failed")?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn run() -> i32 {
    #[cfg(windows)]
    {
        run_windows()
    }
    #[cfg(not(windows))]
    {
        fail("this helper is Windows-only")
    }
}

/// Bounded stdin read (B4): the take() limit is applied BEFORE any read, so an
/// overbound payload can never be buffered in full; one extra byte proves the
/// payload exceeds the bound and is refused before any parsing or write.
fn read_bounded_input(reader: &mut impl std::io::Read) -> Result<String, &'static str> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_INPUT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "stdin missing or unreadable")?;
    if bytes.len() > MAX_INPUT {
        return Err("stdin over the size bound");
    }
    String::from_utf8(bytes).map_err(|_| "stdin is not UTF-8")
}

#[cfg(windows)]
fn run_windows() -> i32 {
    // Identity first: without OUR user SID there is no owner for the DACL.
    let Some(sid) = winipc::ffi::current_user_sid() else {
        return fail("current user SID unavailable");
    };
    // Canonical root only: an unsafe override is refused here (never bypasses
    // the protected LOCALAPPDATA root), matching the Rust reader's policy.
    let root = match winpolicy::island_root(
        std::env::var("ABITO_ISLAND_DIR").ok().as_deref(),
        std::env::var("LOCALAPPDATA").ok().as_deref(),
    ) {
        Some(root) => root,
        None => return fail("island root is not the canonical protected one"),
    };
    let runtime = root.join("runtime");
    // The daemon creates the runtime directory; this helper only ever fills
    // session.json inside it. A missing, non-private or reparse runtime
    // directory is refused — nothing is created implicitly.
    if !winipc::ffi::directory_private(&runtime, &sid, false) {
        return fail("runtime directory missing or not private");
    }
    let text = match read_bounded_input(&mut std::io::stdin()) {
        Ok(text) => text,
        Err(message) => return fail(message),
    };
    let bytes = match validated_bytes(&text) {
        Ok(bytes) => bytes,
        Err(message) => return fail(message),
    };
    match winipc::ffi::private_file_bytes(&runtime.join("session.json"), &bytes, &sid) {
        Ok(()) => 0,
        Err(_) => fail("protected session write failed"),
    }
}

fn main() {
    std::process::exit(run());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload() -> String {
        serde_json::json!({
            "protocol": PROTOCOL,
            "endpoint": r"\\.\pipe\abito-0123456789abcdef0123456789abcdef",
            "token": "aGVsbG8td29ybGQtdG9rZW4tMTIz", // 23 base64url chars
            "pid": 4242,
            "startedAt": 1234u64,
        })
        .to_string()
    }

    #[test]
    fn overbound_input_is_refused_before_any_parse_or_write() {
        // One byte over the bound is refused; the bound itself is accepted.
        let over = vec![b'a'; MAX_INPUT + 1];
        assert_eq!(read_bounded_input(&mut over.as_slice()).unwrap_err(), "stdin over the size bound");
        let at_limit = vec![b'a'; MAX_INPUT];
        assert!(read_bounded_input(&mut at_limit.as_slice()).is_ok());
        // The bounded reader only enforces bounds + UTF-8; emptiness is the
        // VALIDATION layer's refusal, not a bounds violation.
        assert_eq!(read_bounded_input(&mut &b""[..]).unwrap(), "");
        assert!(read_bounded_input(&mut &[0xff, 0xfe][..]).is_err(), "non-UTF-8 refused");
    }

    #[test]
    fn valid_payload_roundtrips_canonically() {
        let bytes = validated_bytes(&payload()).unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["pid"], 4242);
        assert_eq!(value["protocol"], PROTOCOL);
        assert_eq!(value["endpoint"], r"\\.\pipe\abito-0123456789abcdef0123456789abcdef");
        assert_eq!(value["startedAt"], 1234);
        assert!(bytes.ends_with(b"\n"));
    }

    #[test]
    fn every_bad_shape_is_refused() {
        for patch in [
            ("protocol", serde_json::json!("island-agent/2")),
            ("endpoint", serde_json::json!("\\\\.\\pipe\\other-0123456789abcdef0123456789abcdef")),
            ("token", serde_json::json!("short")),
            ("token", serde_json::json!("not a token!")),
            ("pid", serde_json::json!(1)),
            ("pid", serde_json::json!("4242")),
            ("startedAt", serde_json::json!("1234")),
            ("procStart", serde_json::json!("12x")),
            ("ppid", serde_json::json!("x")),
        ] {
            let mut bad = serde_json::from_str::<Value>(&payload()).unwrap();
            bad[patch.0] = patch.1;
            assert!(validated_bytes(&bad.to_string()).is_err(), "{}", patch.0);
        }
        // unknown fields refused; missing fields refused
        let mut extra = serde_json::from_str::<Value>(&payload()).unwrap();
        extra["extra"] = serde_json::json!(1);
        assert!(validated_bytes(&extra.to_string()).is_err());
        assert!(validated_bytes("{}").is_err());
        assert!(validated_bytes("not json").is_err());
        assert!(validated_bytes("[1]").is_err());
    }
}
