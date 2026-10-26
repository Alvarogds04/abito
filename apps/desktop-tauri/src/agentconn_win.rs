//! #306 (Windows, W1) — the connection half of the island-agent client over a
//! NAMED PIPE. Same contract as `agentconn.rs` (keep in sync), different gate:
//!
//! * `session.json` lives under `%LOCALAPPDATA%\Abito\runtime`; before reading it
//!   the directory's owner and DACL are verified (only the current user, SYSTEM and
//!   Administrators may have access) — node cannot set ACLs, so we check them.
//! * Squat guard: before the token is sent, `GetNamedPipeServerProcessId` must equal
//!   the pid recorded in `session.json` AND that process must run as OUR user.
//! * Reads never block inside `ReadFile` (they poll `PeekNamedPipe`): a synchronous
//!   pipe handle serialises ReadFile and WriteFile, so a blocked read would deadlock
//!   every command write.
//! Everything fails closed.
use crate::agentclient::{parse_ui_command, ClientState, CommandError, FrameBuffer, Reason, Session, UiCommand};
use crate::winpolicy;
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{LocalFree, HANDLE, WAIT_TIMEOUT};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    GetAce, GetAclInformation, GetTokenInformation, AclSizeInformation, ACCESS_ALLOWED_ACE, ACE_HEADER, ACL,
    ACL_SIZE_INFORMATION, DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, TokenUser, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::System::Pipes::{GetNamedPipeServerProcessId, PeekNamedPipe};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE,
};

const HELLO_WAIT: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(10);

pub struct AgentConn {
    state: Arc<Mutex<ClientState>>,
    writer: Arc<Mutex<Option<Arc<Pipe>>>>,
    stop: Arc<AtomicBool>,
    changed: Arc<dyn Fn(Value) + Send + Sync>,
}

/// `<island>/runtime/session.json` for the configured island root.
pub fn session_path(island_root: &Path) -> PathBuf {
    island_root.join("runtime").join("session.json")
}

/// Island root: `ABITO_ISLAND_DIR` (absolute) or `%LOCALAPPDATA%\Abito`. `home` is unused here.
pub fn island_root(env_dir: Option<&str>, _home: Option<&str>) -> Option<PathBuf> {
    winpolicy::island_root(env_dir, std::env::var("LOCALAPPDATA").ok().as_deref())
}

// ---- tokens / SIDs ------------------------------------------------------------------

fn token_user_sid_string(token: HANDLE) -> Option<String> {
    let mut needed = 0u32;
    unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed) };
    if needed == 0 || needed > 4096 { return None }
    let mut buf = vec![0u8; needed as usize + 8];
    // The buffer is only read back as a TOKEN_USER (pointer-aligned via Vec<u64> below).
    let mut aligned = vec![0u64; buf.len() / 8 + 1];
    let ok = unsafe { GetTokenInformation(token, TokenUser, aligned.as_mut_ptr().cast(), (aligned.len() * 8) as u32, &mut needed) };
    drop(buf.drain(..));
    if ok == 0 { return None }
    let user = unsafe { &*(aligned.as_ptr() as *const TOKEN_USER) };
    sid_to_string(user.User.Sid)
}

fn sid_to_string(sid: *mut core::ffi::c_void) -> Option<String> {
    let mut wide: *mut u16 = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut wide) } == 0 || wide.is_null() { return None }
    let mut len = 0;
    while unsafe { *wide.add(len) } != 0 { len += 1; if len > 256 { unsafe { LocalFree(wide.cast()) }; return None } }
    let text = String::from_utf16(unsafe { std::slice::from_raw_parts(wide, len) }).ok();
    unsafe { LocalFree(wide.cast()) };
    text
}

fn current_user_sid() -> Option<String> {
    let mut token: HANDLE = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 { return None }
    let token = unsafe { OwnedHandle::from_raw_handle(token as _) };
    token_user_sid_string(token.as_raw_handle() as HANDLE)
}

/// SID of the user running `pid`, or None if it cannot be opened (fail closed).
fn process_user_sid(pid: u32) -> Option<String> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() { return None }
    let process = unsafe { OwnedHandle::from_raw_handle(process as _) };
    let mut token: HANDLE = std::ptr::null_mut();
    if unsafe { OpenProcessToken(process.as_raw_handle() as HANDLE, TOKEN_QUERY, &mut token) } == 0 { return None }
    let token = unsafe { OwnedHandle::from_raw_handle(token as _) };
    token_user_sid_string(token.as_raw_handle() as HANDLE)
}

fn process_alive(pid: u32) -> bool {
    let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() { return false }
    let process = unsafe { OwnedHandle::from_raw_handle(process as _) };
    unsafe { WaitForSingleObject(process.as_raw_handle() as HANDLE, 0) == WAIT_TIMEOUT }
}

/// Owner + DACL of `dir` against the policy. Any API failure refuses.
fn directory_private(dir: &Path, current: &str) -> bool {
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
    let (mut owner, mut dacl, mut descriptor) = (std::ptr::null_mut(), std::ptr::null_mut::<ACL>(), std::ptr::null_mut());
    let status = unsafe {
        GetNamedSecurityInfoW(wide.as_ptr(), SE_FILE_OBJECT, OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner, std::ptr::null_mut(), &mut dacl, std::ptr::null_mut(), &mut descriptor)
    };
    if status != 0 { return false }
    let verdict = (|| {
        let owner = sid_to_string(owner)?;
        if dacl.is_null() { return Some(winpolicy::dacl_private(&owner, current, None)) }
        let mut info: ACL_SIZE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetAclInformation(dacl, (&mut info as *mut ACL_SIZE_INFORMATION).cast(), std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32, AclSizeInformation) } == 0 { return None }
        let mut allowed = Vec::new();
        for index in 0..info.AceCount {
            let mut ace: *mut core::ffi::c_void = std::ptr::null_mut();
            if unsafe { GetAce(dacl, index, &mut ace) } == 0 || ace.is_null() { return None }
            let header = unsafe { &*(ace as *const ACE_HEADER) };
            match header.AceType {
                0 => { // ACCESS_ALLOWED_ACE_TYPE
                    let ace = unsafe { &*(ace as *const ACCESS_ALLOWED_ACE) };
                    allowed.push(sid_to_string((&ace.SidStart as *const u32).cast_mut().cast())?);
                }
                1 => {} // ACCESS_DENIED: never widens access
                _ => return None, // object/callback/unknown ACEs: not understood, refuse
            }
        }
        Some(winpolicy::dacl_private(&owner, current, Some(&allowed)))
    })().unwrap_or(false);
    unsafe { LocalFree(descriptor) };
    verdict
}

// ---- the pipe ------------------------------------------------------------------------

/// A connected pipe client. Reads poll `PeekNamedPipe`, writes are plain `WriteFile`.
struct Pipe { file: File, closed: AtomicBool }
impl Pipe {
    fn handle(&self) -> HANDLE { self.file.as_raw_handle() as HANDLE }
    /// Bytes available, `Ok(None)` when the peer is gone.
    fn available(&self) -> Option<u32> {
        let mut avail = 0u32;
        let ok = unsafe { PeekNamedPipe(self.handle(), std::ptr::null_mut(), 0, std::ptr::null_mut(), &mut avail, std::ptr::null_mut()) };
        (ok != 0).then_some(avail)
    }
    /// 0 = EOF, `TimedOut` after `wait`. Never blocks in ReadFile.
    fn read_timeout(&self, out: &mut [u8], wait: Duration) -> std::io::Result<usize> {
        let end = Instant::now() + wait;
        loop {
            if self.closed.load(Ordering::SeqCst) { return Ok(0) }
            match self.available() {
                None => return Ok(0),
                Some(n) if n > 0 => return (&self.file).read(out),
                Some(_) if Instant::now() >= end => return Err(std::io::ErrorKind::TimedOut.into()),
                Some(_) => std::thread::sleep(POLL),
            }
        }
    }
    fn write_all(&self, bytes: &[u8]) -> std::io::Result<()> { (&self.file).write_all(bytes) }
    fn close(&self) { self.closed.store(true, Ordering::SeqCst) }
}

/// Open the daemon pipe and prove who is on the other end BEFORE any byte (the token) is sent.
fn connect_verified(session: &Session) -> Result<Pipe, Reason> {
    let file = std::fs::OpenOptions::new().read(true).write(true).open(&session.endpoint).map_err(|_| Reason::NoDaemon)?;
    let mut server = 0u32;
    if unsafe { GetNamedPipeServerProcessId(file.as_raw_handle() as HANDLE, &mut server) } == 0 || server != session.pid {
        return Err(Reason::Refused);
    }
    let (mine, theirs) = (current_user_sid().ok_or(Reason::Refused)?, process_user_sid(server).ok_or(Reason::Refused)?);
    if mine != theirs { return Err(Reason::Refused) }
    Ok(Pipe { file, closed: AtomicBool::new(false) })
}

/// The session file must sit in a private directory and the recorded daemon must be alive.
fn read_session(path: &Path) -> Result<Session, Reason> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| Reason::NoDaemon)?;
    if !meta.file_type().is_file() || meta.file_type().is_symlink() { return Err(Reason::BadSession) }
    let me = current_user_sid().ok_or(Reason::BadSession)?;
    // The island root, the runtime directory and the file's own directory must all be private.
    let runtime = path.parent().ok_or(Reason::BadSession)?;
    let root = runtime.parent().ok_or(Reason::BadSession)?;
    if !directory_private(runtime, &me) || !directory_private(root, &me) { return Err(Reason::BadSession) }
    let text = std::fs::read_to_string(path).map_err(|_| Reason::BadSession)?;
    let session = Session::parse(&text).map_err(|_| Reason::BadSession)?;
    if !process_alive(session.pid) { return Err(Reason::NoDaemon) }
    Ok(session)
}

impl AgentConn {
    pub fn new(changed: impl Fn(Value) + Send + Sync + 'static) -> AgentConn {
        AgentConn {
            state: Arc::new(Mutex::new(ClientState::disconnected(Reason::NoDaemon))),
            writer: Arc::new(Mutex::new(None)),
            stop: Arc::new(AtomicBool::new(false)),
            changed: Arc::new(changed),
        }
    }

    pub fn startup_failed(&self, reason: crate::daemonhost::StartupFailure) {
        let mut state = ClientState::disconnected(Reason::NoDaemon);
        state.note_startup_failure(reason);
        self.set(state);
    }

    pub fn view(&self) -> Value {
        serde_json::to_value(&*self.state.lock().expect("agent state lock")).unwrap_or(Value::Null)
    }

    fn set(&self, s: ClientState) {
        let view = serde_json::to_value(&s).unwrap_or(Value::Null);
        *self.state.lock().expect("agent state lock") = s;
        (self.changed)(view);
    }

    pub fn connect(&self, island_root: &Path) {
        self.close();
        let session = match read_session(&session_path(island_root)) {
            Ok(s) => s,
            Err(reason) => return self.set(ClientState::disconnected(reason)),
        };
        let pipe = match connect_verified(&session) {
            Ok(p) => Arc::new(p),
            Err(reason) => return self.set(ClientState::disconnected(reason)),
        };
        if pipe.write_all(session.hello().as_bytes()).is_err() {
            crate::failure::report("island", "agent-refused", crate::failure::Reason::AgentRefused, None);
            return self.set(ClientState::disconnected(Reason::Refused));
        }
        let mut buf = FrameBuffer::default();
        let mut chunk = [0u8; 4096];
        let deadline = Instant::now() + HELLO_WAIT;
        let first = loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match pipe.read_timeout(&mut chunk, left) {
                Ok(0) | Err(_) => {
                    crate::failure::report("island", "agent-no-answer", crate::failure::Reason::AgentNoAnswer, None);
                    return self.set(ClientState::disconnected(Reason::Refused));
                }
                Ok(n) => match buf.push(&chunk[..n]) {
                    Ok(frames) if !frames.is_empty() => break frames,
                    Ok(_) => continue,
                    Err(_) => return self.set(ClientState::disconnected(Reason::Refused)),
                },
            }
        };
        if first[0].get("type").and_then(Value::as_str) != Some("ready") {
            return self.set(ClientState::disconnected(Reason::Refused));
        }
        self.stop.store(false, Ordering::SeqCst);
        *self.writer.lock().expect("agent writer lock") = Some(pipe.clone());
        self.set(ClientState::connected());
        self.spawn_reader(pipe, buf);
        let _ = self.send(&UiCommand::Refresh);
    }

    fn spawn_reader(&self, reader: Arc<Pipe>, mut buf: FrameBuffer) {
        let (state, changed, stop, writer) = (self.state.clone(), self.changed.clone(), self.stop.clone(), self.writer.clone());
        std::thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            loop {
                if stop.load(Ordering::SeqCst) { return }
                match reader.read_timeout(&mut chunk, Duration::from_millis(500)) {
                    Ok(0) => break,
                    Ok(n) => match buf.push(&chunk[..n]) {
                        Ok(frames) => {
                            let view = {
                                let mut st = state.lock().expect("agent state lock");
                                for f in &frames { let _ = st.apply(f); }
                                serde_json::to_value(&*st).unwrap_or(Value::Null)
                            };
                            changed(view);
                        }
                        Err(_) => break,
                    },
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                    Err(_) => break,
                }
            }
            if !stop.load(Ordering::SeqCst) {
                *writer.lock().expect("agent writer lock") = None;
                crate::failure::report("island", "agent-lost", crate::failure::Reason::AgentLost, None);
                let lost = ClientState::disconnected(Reason::Lost);
                let view = serde_json::to_value(&lost).unwrap_or(Value::Null);
                *state.lock().expect("agent state lock") = lost;
                changed(view);
            }
        });
    }

    pub fn send(&self, cmd: &UiCommand) -> Result<(), CommandError> {
        let lines = match cmd {
            UiCommand::Refresh => cmd.refresh_lines(),
            UiCommand::View => return Ok(()),
            UiCommand::Account(_) => return Err(CommandError),
            other => vec![other.line()],
        };
        let guard = self.writer.lock().expect("agent writer lock");
        let pipe = guard.as_ref().ok_or(CommandError)?;
        for line in lines {
            if pipe.write_all(line.as_bytes()).is_err() { return Err(CommandError) }
        }
        Ok(())
    }

    pub fn command(&self, payload: &Value) -> Result<Value, CommandError> {
        let cmd = parse_ui_command(payload)?;
        if matches!(cmd, UiCommand::View) { return Ok(self.view()) }
        self.send(&cmd)?;
        Ok(self.view())
    }

    pub fn close(&self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(p) = self.writer.lock().expect("agent writer lock").take() { p.close(); }
    }
}

impl Drop for AgentConn {
    fn drop(&mut self) { self.close(); }
}
