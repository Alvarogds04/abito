//! Native Windows filesystem facts and writes for the hook/Pi installers (HU-047).
//!
//! Node cannot read or set an ACL, so every trust-sensitive read of facts and
//! every write of an installer-managed file goes through `abito-private-session`
//! flags backed by this module. All verdicts are fail-closed with fixed reasons.
//!
//! * `check_path` is strictly read-only.
//! * `write_private` verifies the parent chain, creates the temp file in the same
//!   directory WITH a protected owner-only DACL at creation (CREATE_NEW), writes
//!   bounded bytes, flushes, replaces atomically (a rename by the temp handle,
//!   relative to the held parent directory, ReplaceIfExists) and re-verifies the final file.
//! * Owner policy: current user SID; BUILTIN\Administrators only for an elevated token.
//! * Paths: drive-letter absolute only (`winpolicy::fs_path_ok`), reparse points
//!   refused on every component.

/// Largest payload `--write-private` accepts (matches the installers' 1 MiB file cap).
pub const MAX_WRITE: usize = 1024 * 1024;

/// Fixed refusal reasons surfaced to Ajustes (never paths, never content).
pub const REASONS: [&str; 9] = ["unsafe_path", "not_private", "reparse", "exists", "missing", "mismatch", "too_large", "busy", "io"];

#[cfg(windows)]
pub mod ffi {
    use super::MAX_WRITE;
    use crate::winipc::ffi::{current_user_sid, last_error, owner_and_dacl, plain_path, prepare_private_directory, wide};
    use crate::winpolicy::{install_acl_private, Expect, FsCommand};
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::path::Path;
    use windows_sys::Win32::Foundation::{LocalFree, HANDLE, INVALID_HANDLE_VALUE, ERROR_FILE_EXISTS, GENERIC_WRITE};
    use windows_sys::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
    use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, GetFileAttributesW, GetFileInformationByHandle, GetFinalPathNameByHandleW, SetFileInformationByHandle, FileDispositionInfo, FileRenameInfo, FILE_DISPOSITION_INFO, FILE_RENAME_INFO, FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, DELETE, FILE_GENERIC_READ,
        BY_HANDLE_FILE_INFORMATION, CREATE_NEW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, INVALID_FILE_ATTRIBUTES, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    type Refusal = &'static str;

    thread_local! { static DETAIL: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) }; }
    /// Which call failed and with which OS error, for the CURRENT thread's last refusal. Never part of the helper's JSON
    /// (reasons stay fixed); tests append it to their failure message and `--check` style diagnostics can read it.
    pub fn last_detail() -> String { DETAIL.with(|d| d.borrow().clone()) }
    fn note(what: &str, e: &std::io::Error) { DETAIL.with(|d| *d.borrow_mut() = format!("{what}: {e}")); }
    fn noted(what: &str) -> impl Fn(std::io::Error) -> Refusal + '_ { move |e| { note(what, &e); "io" } }

    pub fn token_elevated() -> Option<bool> {
        let mut token: HANDLE = std::ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 { return None; }
        let token = unsafe { OwnedHandle::from_raw_handle(token as _) };
        let mut info = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut len = 0u32;
        let ok = unsafe { GetTokenInformation(token.as_raw_handle() as HANDLE, TokenElevation, (&mut info as *mut TOKEN_ELEVATION).cast(), std::mem::size_of::<TOKEN_ELEVATION>() as u32, &mut len) };
        (ok != 0).then_some(info.TokenIsElevated != 0)
    }

    pub struct Who { sid: String, elevated: bool }
    pub fn who() -> Result<Who, Refusal> {
        Ok(Who { sid: current_user_sid().ok_or("io")?, elevated: token_elevated().ok_or("io")? })
    }

    fn attrs(path: &Path) -> Option<u32> {
        let a = unsafe { GetFileAttributesW(wide(&path.to_string_lossy()).as_ptr()) };
        (a != INVALID_FILE_ATTRIBUTES).then_some(a)
    }

    /// Link count of an existing entry, opened WITHOUT following a reparse point.
    fn link_count(path: &Path) -> Option<u32> {
        let h = unsafe {
            CreateFileW(wide(&path.to_string_lossy()).as_ptr(), FILE_READ_ATTRIBUTES, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(), OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT, std::ptr::null_mut())
        };
        if h == INVALID_HANDLE_VALUE { return None; }
        let h = unsafe { OwnedHandle::from_raw_handle(h as _) };
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        (unsafe { GetFileInformationByHandle(h.as_raw_handle() as HANDLE, &mut info) } != 0).then_some(info.nNumberOfLinks)
    }

    fn entry_private(path: &Path, who: &Who) -> bool {
        let Some((owner, allowed)) = owner_and_dacl(path) else { return false };
        install_acl_private(&owner, &who.sid, who.elevated, Some(&allowed))
    }

    /// Parent-directory policy: plain chain (no reparse anywhere), a real directory,
    /// install owner and private principals.
    pub fn dir_ok(dir: &Path, who: &Who) -> bool {
        plain_path(dir) && attrs(dir).is_some_and(|a| a & FILE_ATTRIBUTE_DIRECTORY != 0) && entry_private(dir, who)
    }

    /// A regular, single-link, private file (not a directory, not reparse).
    fn file_ok(file: &Path, who: &Who) -> bool {
        plain_path(file) && attrs(file).is_some_and(|a| a & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) == 0)
            && link_count(file) == Some(1) && entry_private(file, who)
    }


    /// HU-047 M3: the verified parent directory, opened ONCE and kept open for the whole operation.
    /// Share mode is READ|WRITE and deliberately WITHOUT FILE_SHARE_DELETE, so while this value lives nobody can rename,
    /// delete or replace that directory (or an ancestor) with a junction: such a call fails with a sharing violation.
    /// `real` is the handle's own final path (the OS resolves 8.3 names and links), taken once at open: anything created
    /// in the directory must resolve to `real` + name, and `still_here` re-asserts that the handle still sits there.
    /// EVERY ancestor up to the drive root is held the same way (top down, none may be a reparse point), so the chain
    /// cannot be re-pointed after the check without relying on how NTFS treats child handles. The plain-chain check
    /// (`dir_ok`) runs AFTER the opens. All swaps, renames and deletes below go through THIS handle (RootDirectory).
    pub struct HeldDir { handle: OwnedHandle, _ancestors: Vec<OwnedHandle>, dir: std::path::PathBuf, real: String }

    fn final_path(h: HANDLE) -> Option<String> {
        let mut buf = vec![0u16; 1024];
        let n = unsafe { GetFinalPathNameByHandleW(h, buf.as_mut_ptr(), buf.len() as u32, 0) } as usize;
        if n == 0 || n >= buf.len() { return None; }
        Some(String::from_utf16_lossy(&buf[..n]))
    }
    /// `\\?\C:\a\b` -> `c:\a\b` (case-insensitive compare form).
    fn norm(p: &str) -> String { p.strip_prefix(r"\\?\").unwrap_or(p).trim_end_matches('\\').to_ascii_lowercase() }

    impl HeldDir {
        /// Opens `dir` without following a reparse point, refuses anything that is not a plain directory, and
        /// refuses when the handle does not point at exactly the requested path. Share mode excludes DELETE.
        pub fn open(dir: &Path, who: &Who) -> Result<HeldDir, Refusal> {
            let mut chain: Vec<&Path> = dir.ancestors().filter(|a| !a.as_os_str().is_empty()).collect();
            chain.reverse(); // drive root first
            // THE BOUNDARY (HU-047 M3): every ancestor from the FIRST directory owned by the install owner (our user, or
            // Administrators for an elevated token) down to the target is held, and a level in that range that cannot be
            // opened refuses the operation. Levels ABOVE it (C:\, C:\Users ...) belong to the system or to another account;
            // a program running as our user cannot rename them, so they are outside the threat model and are not held.
            let from = chain.iter().position(|level| owner_and_dacl(level).is_some_and(|(owner, _)| crate::winpolicy::install_owner_ok(&owner, &who.sid, who.elevated))).ok_or("not_private")?;
            let mut held: Vec<OwnedHandle> = Vec::new();
            let last = chain.len().saturating_sub(1);
            // Ancestors need only FILE_READ_ATTRIBUTES; the directory itself also needs the add rights because it is the
            // RootDirectory of the relative rename.
            for (i, level) in chain.into_iter().enumerate().skip(from) { held.push(open_plain_dir(level, i == last)?); }
            let handle = held.pop().ok_or("unsafe_path")?;
            let real = final_path(handle.as_raw_handle() as HANDLE).ok_or("io")?;
            Ok(HeldDir { handle, _ancestors: held, dir: dir.to_path_buf(), real })
        }
        #[allow(dead_code)]
        pub fn path(&self) -> &Path { &self.dir }
        /// True while the held handle still resolves to the same path (cheap re-assertion right before a swap).
        pub fn still_here(&self) -> bool {
            final_path(self.handle.as_raw_handle() as HANDLE).is_some_and(|r| norm(&r) == norm(&self.real))
        }
        /// Policy on the held directory: plain chain, a real directory, install owner and private principals.
        pub fn verified(dir: &Path, who: &Who) -> Result<HeldDir, Refusal> {
            let held = HeldDir::open(dir, who)?;
            if !dir_ok(dir, who) { return Err("not_private"); }
            Ok(held)
        }
    }

    /// One directory level, opened without following a reparse point and WITHOUT FILE_SHARE_DELETE.
    fn open_plain_dir(dir: &Path, target: bool) -> Result<OwnedHandle, Refusal> {
        let access = if target { FILE_LIST_DIRECTORY | FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY | FILE_READ_ATTRIBUTES } else { FILE_READ_ATTRIBUTES };
        let h = unsafe {
            CreateFileW(wide(&dir.to_string_lossy()).as_ptr(), access, FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(), OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT, std::ptr::null_mut())
        };
        if h == INVALID_HANDLE_VALUE {
            let e = last_error();
            note(&format!("open directory {}", dir.display()), &e);
            return Err(match e.raw_os_error() { Some(2) | Some(3) => "missing", Some(32) => "busy", _ => "io" });
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(h as _) };
        let info = handle_info(handle.as_raw_handle() as HANDLE).ok_or("io")?;
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 { #[cfg(test)] eprintln!("HeldDir: ancestor {dir:?} is a reparse point"); return Err("reparse"); }
        if info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0 { return Err("unsafe_path"); }
        Ok(handle)
    }

    fn handle_info(h: HANDLE) -> Option<BY_HANDLE_FILE_INFORMATION> {
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        (unsafe { GetFileInformationByHandle(h, &mut info) } != 0).then_some(info)
    }

    /// Opens an EXISTING entry of the held directory (file or directory) without following a link, WITHOUT FILE_SHARE_DELETE,
    /// with DELETE access so the same handle can rename or delete it. It must resolve directly under the held directory.
    fn open_entry(held: &HeldDir, name: &str) -> Result<(OwnedHandle, BY_HANDLE_FILE_INFORMATION), Refusal> {
        let path = held.dir.join(name);
        let h = unsafe {
            CreateFileW(wide(&path.to_string_lossy()).as_ptr(), DELETE | FILE_READ_ATTRIBUTES, FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(), OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT, std::ptr::null_mut())
        };
        if h == INVALID_HANDLE_VALUE {
            let e = last_error();
            note(&format!("open entry {}", path.display()), &e);
            return Err(match e.raw_os_error() { Some(2) | Some(3) => "missing", Some(32) => "busy", _ => "io" });
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(h as _) };
        let info = handle_info(handle.as_raw_handle() as HANDLE).ok_or("io")?;
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 { return Err("reparse"); }
        if !lives_under(handle.as_raw_handle() as HANDLE, held, name) { return Err("reparse"); }
        Ok((handle, info))
    }

    /// Rename (or move) the open `file` (by ITS OWN handle) to `leaf` inside the held directory `dest`.
    ///
    /// The Win32 wrapper rejects a non-null RootDirectory, so the target is the FULL final path (`dest.real` + leaf) with
    /// RootDirectory NULL. That is safe ONLY because of what HeldDir guarantees: every ancestor from the first user-owned
    /// directory down to `dest` is held open without FILE_SHARE_DELETE, so no program running as this user can rename,
    /// delete or re-point any of them, and the same path string therefore resolves to the same directory for the whole
    /// operation (`still_here` re-asserts it just before). Levels above that range are system-owned (outside the threat model).
    /// `replace` is the only way an existing target is overwritten.
    fn rename_into(file: HANDLE, dest: &HeldDir, leaf: &str, replace: bool) -> Result<(), Refusal> {
        let full = format!("{}\\{}", dest.real.trim_end_matches('\\'), leaf);
        let name: Vec<u16> = full.encode_utf16().collect();
        let off = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
        let bytes = off + name.len() * 2 + 2;
        let mut buf = vec![0u64; bytes.div_ceil(8)];
        unsafe {
            let info = buf.as_mut_ptr() as *mut FILE_RENAME_INFO;
            (*info).Anonymous.ReplaceIfExists = replace;
            (*info).RootDirectory = std::ptr::null_mut();
            (*info).FileNameLength = (name.len() * 2) as u32;
            std::ptr::copy_nonoverlapping(name.as_ptr(), (buf.as_mut_ptr() as *mut u8).add(off) as *mut u16, name.len());
            if SetFileInformationByHandle(file, FileRenameInfo, buf.as_ptr().cast(), bytes as u32) == 0 {
                let e = last_error();
                note(&format!("rename to {full}"), &e);
                return Err(match e.raw_os_error() { Some(80) | Some(183) => "exists", Some(32) => "busy", _ => "io" });
            }
        }
        Ok(())
    }

    /// A file just created (or about to be replaced) must live directly under the held directory.
    fn lives_under(file: HANDLE, held: &HeldDir, name: &str) -> bool {
        final_path(file).is_some_and(|r| norm(&r) == format!("{}\\{}", norm(&held.real), name.to_ascii_lowercase()))
    }

    fn parent_of(path: &Path) -> Result<&Path, Refusal> { path.parent().ok_or("unsafe_path") }

    fn json_escape(s: &str) -> String { serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into()) }

    fn read_bounded(file: &Path) -> Result<Vec<u8>, Refusal> {
        let f = std::fs::File::open(file).map_err(|_| "io")?;
        let mut buf = Vec::new();
        f.take((MAX_WRITE + 1) as u64).read_to_end(&mut buf).map_err(|_| "io")?;
        if buf.len() > MAX_WRITE { return Err("too_large"); }
        Ok(buf)
    }
    fn hex(bytes: &[u8]) -> String { Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect() }

    /// Read-only. Always prints facts for the PARENT and, when it exists, the target.
    pub fn check_path(path: &str) -> Result<String, Refusal> {
        let who = who()?;
        let target = Path::new(path);
        let parent_ok = dir_ok(parent_of(target)?, &who);
        let meta = std::fs::symlink_metadata(target);
        let (exists, kind, size, mtime_ms) = match &meta {
            Ok(m) => (true, if m.is_dir() { "dir" } else if m.is_file() { "file" } else { "other" }, m.len(),
                m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (false, "none", 0, 0),
            Err(_) => return Err("io"),
        };
        let reparse = exists && attrs(target).is_some_and(|a| a & FILE_ATTRIBUTE_REPARSE_POINT != 0);
        let private = exists && !reparse && match kind { "file" => file_ok(target, &who), "dir" => dir_ok(target, &who), _ => false };
        let nlink = if exists && !reparse { link_count(target).unwrap_or(0) } else { 0 };
        Ok(format!("{{\"schema\":\"abito-fs/1\",\"ok\":true,\"path\":{},\"parent_ok\":{},\"exists\":{},\"kind\":\"{}\",\"reparse\":{},\"private\":{},\"nlink\":{},\"size\":{},\"mtime_ms\":{},\"elevated\":{}}}",
            json_escape(path), parent_ok, exists, kind, reparse, private, nlink, size, mtime_ms, who.elevated))
    }

    /// Creates ONE directory (parent must pass the directory policy) with a protected DACL at creation.
    /// `created:false` means it already existed and passes the policy (used as the lock primitive).
    pub fn make_dir(path: &str) -> Result<String, Refusal> {
        let who = who()?;
        let dir = Path::new(path);
        let _held = HeldDir::verified(parent_of(dir)?, &who)?; // M3: held (no FILE_SHARE_DELETE) until the directory exists and is checked
        let existed = std::fs::symlink_metadata(dir).is_ok();
        if existed {
            if !dir_ok(dir, &who) { return Err(if attrs(dir).is_some_and(|a| a & FILE_ATTRIBUTE_REPARSE_POINT != 0) { "reparse" } else { "not_private" }); }
        } else {
            prepare_private_directory(dir, &who.sid, dir).map_err(|_| "io")?;
            if !dir_ok(dir, &who) { return Err("not_private"); }
        }
        Ok(format!("{{\"schema\":\"abito-fs/1\",\"ok\":true,\"created\":{}}}", !existed))
    }

    pub fn write_private(path: &str, expect: &Expect, data: &[u8]) -> Result<String, Refusal> {
        write_private_with(path, expect, data, &|| {})
    }

    /// `before_swap` runs after the temp file is written and BEFORE the rename (tests use it to attack that window).
    pub fn write_private_with(path: &str, expect: &Expect, data: &[u8], before_swap: &dyn Fn()) -> Result<String, Refusal> {
        if data.len() > MAX_WRITE { return Err("too_large"); }
        let who = who()?;
        let target = Path::new(path);
        let dir = parent_of(target)?;
        let held = HeldDir::verified(dir, &who)?; // M3: parent chain held through check, create, swap and re-verification
        match (std::fs::symlink_metadata(target), expect) {
            (Err(e), Expect::Absent) if e.kind() == std::io::ErrorKind::NotFound => {}
            (Err(e), Expect::Sha256(_)) if e.kind() == std::io::ErrorKind::NotFound => return Err("missing"),
            (Err(_), _) => return Err("io"),
            (Ok(_), Expect::Absent) => return Err("exists"),
            (Ok(_), Expect::Sha256(want)) => {
                if attrs(target).is_some_and(|a| a & FILE_ATTRIBUTE_REPARSE_POINT != 0) { return Err("reparse"); }
                if !file_ok(target, &who) { return Err("not_private"); }
                if &hex(&read_bounded(target)?) != want { return Err("mismatch"); }
            }
        }
        let leaf = target.file_name().ok_or("unsafe_path")?.to_string_lossy().into_owned();
        let sddl = crate::winipc::sddl_owner_full(&who.sid).ok_or("io")?;
        let sddl_w = wide(&sddl);
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl_w.as_ptr(), SDDL_REVISION_1, &mut sd, std::ptr::null_mut()) } == 0 { note("build security descriptor", &last_error()); return Err("io"); }
        let attributes = SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: sd, bInheritHandle: 0 };
        // The temp handle (share mode 0, DELETE|read|write) stays OPEN until after the swap: nobody can rename or replace it
        // in between, and the same handle is renamed relative to the held parent and read back for the hash.
        let mut created: Option<(std::path::PathBuf, OwnedHandle)> = None;
        for attempt in 0..8u32 {
            let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
            let candidate = dir.join(format!(".{leaf}.abito-tmp-{}-{nonce}-{attempt}", std::process::id()));
            let h = unsafe { CreateFileW(wide(&candidate.to_string_lossy()).as_ptr(), GENERIC_WRITE | FILE_GENERIC_READ | DELETE, 0, &attributes, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, std::ptr::null_mut()) };
            if h != INVALID_HANDLE_VALUE {
                let h = unsafe { OwnedHandle::from_raw_handle(h as _) };
                let tmp_name = candidate.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if !lives_under(h.as_raw_handle() as HANDLE, &held, &tmp_name) {
                    DETAIL.with(|d| *d.borrow_mut() = format!("temp {tmp_name} resolves to {:?}, expected under {}", final_path(h.as_raw_handle() as HANDLE), held.real));
                    // Created somewhere else: delete exactly what we hold (by handle), never by path.
                    let _ = mark_delete(h.as_raw_handle() as HANDLE);
                    unsafe { LocalFree(sd.cast()) };
                    return Err("reparse");
                }
                created = Some((candidate, h));
                break;
            }
            let e = last_error();
            note(&format!("create temp {}", candidate.display()), &e);
            if e.raw_os_error() != Some(ERROR_FILE_EXISTS as i32) { break; }
        }
        unsafe { LocalFree(sd.cast()) };
        let (_tmp, handle) = created.ok_or("io")?;
        let raw = handle.as_raw_handle() as HANDLE;
        let result = (|| -> Result<(), Refusal> {
            let mut f = std::fs::File::from(handle.try_clone().map_err(|e| { note("duplicate temp handle", &e); "io" })?);
            f.write_all(data).map_err(noted("write temp"))?;
            f.sync_all().map_err(noted("flush temp"))?;
            before_swap();
            // Final guard right before the swap: the chain is still the held one and still private.
            if !held.still_here() || !dir_ok(dir, &who) { return Err("not_private"); }
            rename_into(raw, &held, &leaf, true)
        })();
        if let Err(e) = result { let _ = mark_delete(raw); return Err(e); }
        // Verify the file we renamed (same handle): right place, right bytes, private.
        if !lives_under(raw, &held, &leaf) { return Err("reparse"); }
        let mut back = std::fs::File::from(handle);
        use std::io::Seek;
        back.seek(std::io::SeekFrom::Start(0)).map_err(noted("seek back"))?;
        let mut buf = Vec::new();
        (&mut back).take((MAX_WRITE + 1) as u64).read_to_end(&mut buf).map_err(noted("read back"))?;
        if hex(&buf) != hex(data) { return Err("mismatch"); }
        if !file_ok(target, &who) { return Err("not_private"); }
        Ok(format!("{{\"schema\":\"abito-fs/1\",\"ok\":true,\"sha256\":\"{}\",\"size\":{}}}", hex(data), data.len()))
    }

    /// Marks the entry behind `h` for deletion (it goes when the last handle closes). By handle, never by path.
    fn mark_delete(h: HANDLE) -> Result<(), Refusal> {
        let info = FILE_DISPOSITION_INFO { DeleteFile: true };
        if unsafe { SetFileInformationByHandle(h, FileDispositionInfo, (&info as *const FILE_DISPOSITION_INFO).cast(), std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32) } == 0 { note("mark for deletion", &last_error()); return Err("io"); }
        Ok(())
    }

    /// Rename within ONE volume and ONE private parent chain each; never replaces, never copies.
    pub fn move_entry(from: &str, to: &str) -> Result<String, Refusal> {
        move_entry_with(from, to, &|| {})
    }

    /// `before_rename` runs after the source is verified and BEFORE the rename (tests use it to attack that window).
    pub fn move_entry_with(from: &str, to: &str, before_rename: &dyn Fn()) -> Result<String, Refusal> {
        let who = who()?;
        let (src, dst) = (Path::new(from), Path::new(to));
        if from.as_bytes()[0].to_ascii_lowercase() != to.as_bytes()[0].to_ascii_lowercase() { return Err("unsafe_path"); }
        let (sp, dp) = (parent_of(src)?, parent_of(dst)?);
        let held_src = HeldDir::verified(sp, &who)?; // M3: both parents held until the rename is done
        let held_dst = if norm(&sp.to_string_lossy()) == norm(&dp.to_string_lossy()) { None } else { Some(HeldDir::verified(dp, &who)?) };
        let (sleaf, dleaf) = (src.file_name().ok_or("unsafe_path")?.to_string_lossy().into_owned(), dst.file_name().ok_or("unsafe_path")?.to_string_lossy().into_owned());
        // The SOURCE is held too (no FILE_SHARE_DELETE) from the check to the rename, and renamed by its own handle.
        let (entry, info) = open_entry(&held_src, &sleaf)?;
        let is_dir = info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
        let ok = if is_dir { dir_ok(src, &who) } else { info.nNumberOfLinks == 1 && file_ok(src, &who) };
        if !ok { return Err("not_private"); }
        before_rename();
        let dest = held_dst.as_ref().unwrap_or(&held_src);
        if !held_src.still_here() || !dest.still_here() { return Err("not_private"); }
        rename_into(entry.as_raw_handle() as HANDLE, dest, &dleaf, false)?;
        if !lives_under(entry.as_raw_handle() as HANDLE, dest, &dleaf) { return Err("reparse"); }
        Ok("{\"schema\":\"abito-fs/1\",\"ok\":true}".into())
    }

    /// Deletes one file or one EMPTY directory that passes the policy. Never recursive.
    pub fn delete_entry(path: &str) -> Result<String, Refusal> {
        let who = who()?;
        let t = Path::new(path);
        let held = HeldDir::verified(parent_of(t)?, &who)?; // M3: parent chain AND the entry held until it is gone
        let leaf = t.file_name().ok_or("unsafe_path")?.to_string_lossy().into_owned();
        let (entry, info) = open_entry(&held, &leaf)?;
        let dir = info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
        if !(if dir { dir_ok(t, &who) } else { info.nNumberOfLinks == 1 && file_ok(t, &who) }) { return Err("not_private"); }
        mark_delete(entry.as_raw_handle() as HANDLE)?; // a non-empty directory refuses here; deleted when `entry` closes
        drop(entry);
        Ok("{\"schema\":\"abito-fs/1\",\"ok\":true}".into())
    }

    pub fn run(cmd: &FsCommand, stdin: &mut impl Read) -> Result<String, Refusal> {
        match cmd {
            FsCommand::CheckPath(p) => check_path(p),
            FsCommand::MakeDir(p) => make_dir(p),
            FsCommand::Delete(p) => delete_entry(p),
            FsCommand::Move { from, to } => move_entry(from, to),
            FsCommand::WritePrivate { path, expect } => {
                let mut buf = Vec::new();
                stdin.take((MAX_WRITE + 1) as u64).read_to_end(&mut buf).map_err(|_| "io")?;
                write_private(path, expect, &buf)
            }
        }
    }
}

#[cfg(all(test, windows))]
mod native_tests {
    use super::ffi::*;
    use crate::winpolicy::Expect;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        // Never under %LOCALAPPDATA%\Abito (the installed smoke refuses a runner that already has it):
        // a fresh child of the temp dir with our own protected DACL.
        let dir = std::env::temp_dir().join(format!("abito-fs-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sid = crate::winipc::ffi::current_user_sid().unwrap();
        crate::winipc::ffi::prepare_runtime(&dir, &sid).unwrap();
        dir
    }
    fn p(dir: &std::path::Path, n: &str) -> String { dir.join(n).to_string_lossy().into_owned() }

    #[test]
    fn write_private_creates_replaces_and_guards_expectation() {
        let d = temp_dir("w");
        let f = p(&d, "settings.json");
        must(write_private(&f, &Expect::Absent, b"{}\n"));
        assert!(check_path(&f).unwrap().contains("\"private\":true"));
        assert_eq!(write_private(&f, &Expect::Absent, b"x").unwrap_err(), "exists");
        assert_eq!(write_private(&f, &Expect::Sha256("0".repeat(64)), b"x").unwrap_err(), "mismatch");
        let h = { use sha2::{Digest, Sha256}; Sha256::digest(b"{}\n").iter().map(|b| format!("{b:02x}")).collect::<String>() };
        must(write_private(&f, &Expect::Sha256(h), b"{\"a\":1}\n"));
        assert_eq!(std::fs::read(&f).unwrap(), b"{\"a\":1}\n");
        assert_eq!(write_private(&p(&d, "big"), &Expect::Absent, &vec![0u8; super::MAX_WRITE + 1]).unwrap_err(), "too_large");
        let leftovers: Vec<_> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(leftovers, vec!["settings.json".to_string()]);
    }

    #[test]
    fn a_junction_parent_or_target_is_refused() {
        let d = temp_dir("j");
        let real = d.join("real");
        std::fs::create_dir(&real).ok();
        let link = d.join("link");
        let ok = std::process::Command::new("cmd").args(["/C", "mklink", "/J", &link.to_string_lossy(), &real.to_string_lossy()]).output().map(|o| o.status.success()).unwrap_or(false);
        assert!(ok, "mklink /J must work on the runner");
        assert!(write_private(&p(&link, "x.json"), &Expect::Absent, b"{}").is_err());
        assert!(check_path(&p(&d, "link")).unwrap().contains("\"reparse\":true"));
        assert!(move_entry(&p(&d, "link"), &p(&d, "other")).is_err());
        assert!(delete_entry(&p(&d, "link")).is_err());
    }

    #[test]
    fn move_never_replaces_and_make_dir_is_idempotent_for_private_dirs() {
        let d = temp_dir("m");
        let (a, b) = (p(&d, "a.txt"), p(&d, "b.txt"));
        write_private(&a, &Expect::Absent, b"1").unwrap();
        write_private(&b, &Expect::Absent, b"2").unwrap();
        assert_eq!(move_entry(&a, &b).unwrap_err(), "exists");
        must(move_entry(&a, &p(&d, "c.txt")));
        assert!(make_dir(&p(&d, "sub")).unwrap().contains("\"created\":true"));
        assert!(make_dir(&p(&d, "sub")).unwrap().contains("\"created\":false"));
        must(delete_entry(&p(&d, "sub")));
    }

    // ---- HU-047 M3: the verified parent stays held; a swap for a junction cannot happen during an operation ----

    #[test]
    fn a_held_parent_cannot_be_renamed_deleted_or_replaced_by_a_junction() {
        let d = temp_dir("h");
        let sub = d.join("sub");
        must(make_dir(&sub.to_string_lossy()));
        let who = who().unwrap();
        let held = HeldDir::verified(&sub, &who).unwrap();
        let (moved, real) = (d.join("moved"), d.join("real"));
        std::fs::create_dir(&real).unwrap();
        assert!(blocked(std::fs::rename(&sub, &moved).unwrap_err().raw_os_error()), "renaming a held directory must fail (sharing violation)");
        assert!(blocked(std::fs::remove_dir(&sub).unwrap_err().raw_os_error()), "deleting a held directory must fail");
        assert!(blocked(std::fs::rename(&d, d.with_extension("gone")).unwrap_err().raw_os_error()), "renaming an ANCESTOR of a held directory must fail too");
        assert!(held.still_here());
        // the junction swap needs the rename first, so it is impossible while held
        assert!(!moved.exists());
        drop(held);
        assert!(std::fs::rename(&sub, &moved).is_ok(), "once released the directory is an ordinary one again");
    }

    /// Windows blocks a rename/delete of a held directory with ERROR_ACCESS_DENIED (5) when other handles are open below it
    /// and with ERROR_SHARING_VIOLATION (32) when the entry itself is the held one: both mean "blocked".
    fn blocked(code: Option<i32>) -> bool { matches!(code, Some(5) | Some(32)) }
    fn must<T: std::fmt::Debug>(r: Result<T, &'static str>) -> T { r.unwrap_or_else(|why| panic!("operation refused: {why} [{}]", last_detail())) }

    /// Every swap the attacker can attempt inside the operation's own window must FAIL with a sharing violation
    /// (the verifier's R-a/R-b/R-d), not merely "not happen to land".
    #[test]
    fn the_windows_inside_write_and_move_cannot_be_used_to_swap_anything() {
        let d = temp_dir("w2");
        let sub = d.join("sub");
        must(make_dir(&sub.to_string_lossy()));
        let kinds = std::cell::RefCell::new(Vec::<Option<i32>>::new());
        let attack_write = || {
            // after the temp file is written, before the swap: the temp, the parent and an ancestor are all taken
            let tmp = std::fs::read_dir(&sub).unwrap().map(|e| e.unwrap().path()).find(|p| p.to_string_lossy().contains(".abito-tmp-")).expect("the temp file exists in the window");
            kinds.borrow_mut().push(std::fs::rename(&tmp, sub.join("stolen")).err().and_then(|e| e.raw_os_error()));
            kinds.borrow_mut().push(std::fs::remove_file(&tmp).err().and_then(|e| e.raw_os_error()));
            kinds.borrow_mut().push(std::fs::File::create(&tmp).err().and_then(|e| e.raw_os_error()));
            kinds.borrow_mut().push(std::fs::rename(&sub, d.join("sub-away")).err().and_then(|e| e.raw_os_error()));
            kinds.borrow_mut().push(std::fs::rename(&d, d.with_extension("away")).err().and_then(|e| e.raw_os_error()));
        };
        let f = p(&sub, "a.json");
        must(write_private_with(&f, &Expect::Absent, b"good", &attack_write));
        assert_eq!(kinds.borrow().len(), 5);
        assert!(kinds.borrow().iter().all(|k| blocked(*k)), "every attempt must be blocked (5 or 32), got {:?}", kinds.borrow());
        assert_eq!(std::fs::read(&f).unwrap(), b"good");
        assert!(!sub.join("stolen").exists());

        kinds.borrow_mut().clear();
        let g = p(&sub, "b.json");
        let attack_move = || {
            // after the source is verified, before the rename: the SOURCE itself is held
            kinds.borrow_mut().push(std::fs::rename(&f, sub.join("hijack")).err().and_then(|e| e.raw_os_error()));
            kinds.borrow_mut().push(std::fs::remove_file(&f).err().and_then(|e| e.raw_os_error()));
            kinds.borrow_mut().push(std::fs::rename(&sub, d.join("sub-away")).err().and_then(|e| e.raw_os_error()));
        };
        must(move_entry_with(&f, &g, &attack_move));
        assert!(kinds.borrow().iter().all(|k| blocked(*k)), "got {:?}", kinds.borrow());
        assert_eq!(std::fs::read(&g).unwrap(), b"good");
        assert!(!sub.join("hijack").exists() && !std::path::Path::new(&f).exists());
    }

    /// Attacker thread hammering the swap with NO outer hold: control first (the attack works when nothing is held),
    /// then the operations either succeed in the real directory or refuse; nothing ever lands elsewhere.
    #[test]
    fn writes_moves_and_deletes_stay_in_the_verified_directory_under_a_swap_race() {
        let d = temp_dir("r");
        let (sub, away) = (d.join("sub"), d.join("away"));
        must(make_dir(&sub.to_string_lossy()));
        let run_attacker = |stop: std::sync::Arc<std::sync::atomic::AtomicBool>, swapped: std::sync::Arc<std::sync::atomic::AtomicU32>| {
            let (sub, away) = (sub.clone(), away.clone());
            std::thread::spawn(move || {
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    if std::fs::rename(&sub, &away).is_ok() {
                        swapped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let _ = std::fs::rename(&away, &sub);
                    }
                }
            })
        };
        // control: with nothing held the attacker DOES swap (the test would otherwise prove nothing)
        let (stop, swapped) = (std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)), std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)));
        let t = run_attacker(stop.clone(), swapped.clone());
        let started = std::time::Instant::now();
        while swapped.load(std::sync::atomic::Ordering::Relaxed) == 0 && started.elapsed().as_secs() < 5 { std::thread::yield_now(); }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        t.join().unwrap();
        assert!(swapped.load(std::sync::atomic::Ordering::Relaxed) > 0, "control: the attacker must be able to swap an unheld directory");
        // now the real operations against the same attacker
        let (stop, swapped) = (std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)), std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)));
        let t = run_attacker(stop.clone(), swapped.clone());
        let mut ok = 0;
        for i in 0..100 {
            let (f, g) = (p(&sub, &format!("f{i}.json")), p(&sub, &format!("g{i}.json")));
            if write_private(&f, &Expect::Absent, b"{}").is_err() { continue; }
            if move_entry(&f, &g).is_ok() && delete_entry(&g).is_ok() { ok += 1; }
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        t.join().unwrap();
        // Each operation holds the directory for its own duration; the attacker may swap only BETWEEN operations, never inside one.
        // So whatever an operation did, it did in the directory it verified: nothing is ever created next to it or under `away`.
        let names = |p: &std::path::Path| std::fs::read_dir(p).map(|r| r.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect::<Vec<_>>()).unwrap_or_default();
        let top = names(&d);
        assert!(top.iter().all(|n| n == "sub" || n == "away"), "nothing was created outside the verified directory: {top:?}");
        for dir in [&sub, &away] { assert!(names(dir).is_empty(), "no leftover temp/file in {dir:?}: {:?}", names(dir)); }
        assert!(ok > 0, "operations must still complete between the attacker's swaps (else the test proves nothing)");
    }

    /// An ancestor with NO child open must resist rename/delete while a deeper directory is held: the ancestor handle itself
    /// carries the protection (error 32), asserted by kind.
    #[test]
    fn a_held_ancestor_resists_rename_on_its_own() {
        let d = temp_dir("a");
        let (mid, leaf) = (d.join("mid"), d.join("mid").join("leaf"));
        must(make_dir(&mid.to_string_lossy()));
        must(make_dir(&leaf.to_string_lossy()));
        let who = who().unwrap();
        let held = HeldDir::verified(&leaf, &who).unwrap();
        // `mid` has the open child `leaf`, but `d` only has `mid` below it, which is itself only held via its own ancestor handle
        for victim in [&mid, &d] {
            let e = std::fs::rename(victim, victim.with_extension("away")).unwrap_err();
            assert!(blocked(e.raw_os_error()), "{victim:?}: {e}");
        }
        let e = std::fs::remove_dir(&leaf).unwrap_err();
        assert!(blocked(e.raw_os_error()));
        drop(held);
        assert!(std::fs::rename(&mid, d.join("mid2")).is_ok(), "released: ordinary again");
    }

    #[test]
    fn a_parent_swapped_for_a_junction_before_the_open_is_refused_by_every_operation() {
        let d = temp_dir("p");
        let (real, link) = (d.join("real"), d.join("link"));
        std::fs::create_dir(&real).unwrap();
        let ok = std::process::Command::new("cmd").args(["/C", "mklink", "/J", &link.to_string_lossy(), &real.to_string_lossy()]).output().map(|o| o.status.success()).unwrap_or(false);
        assert!(ok);
        let who = who().unwrap();
        assert!(matches!(HeldDir::verified(&link, &who), Err("reparse")), "a junction is refused at the open, not after");
        assert!(write_private(&p(&link, "x"), &Expect::Absent, b"1").is_err());
        assert!(make_dir(&p(&link, "y")).is_err());
        assert!(delete_entry(&p(&link, "z")).is_err());
        assert!(std::fs::read_dir(&real).unwrap().next().is_none(), "nothing was written through the junction");
    }
}
