//! App-owned island host. No host restart loop; only the daemon supervises Pi.
//! Linux has kernel parent-death protection. Other platforms fail closed until
//! an equivalent, independently verified lifetime mechanism exists.
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Bounded cadence of the runtime reap poll (#390): short enough that a dead
/// daemon is never a zombie for long, long enough to cost nothing.
#[cfg(any(target_os = "linux", target_os = "macos"))]
const REAP_POLL: std::time::Duration = std::time::Duration::from_millis(50);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupFailure { NodeMissing, PiMissing, HostMissing, SpawnFailed, Unsupported }
impl StartupFailure {
    pub fn code(self) -> &'static str {
        match self {
            Self::NodeMissing => "node_missing", Self::PiMissing => "pi_missing",
            Self::HostMissing => "host_missing", Self::SpawnFailed => "host_spawn_failed",
            Self::Unsupported => "host_platform_unsupported",
        }
    }
}

#[derive(Default)]
pub struct HostSlot {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    child: Arc<Mutex<Option<Host>>>,
    failure: Mutex<Option<StartupFailure>>,
    /// #390: the exit status of a host child that died ON ITS OWN while the app lived.
    exited: Arc<Mutex<Option<std::process::ExitStatus>>>,
}
impl HostSlot {
    pub fn start(&self) {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let result = start();
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let result: Result<(), StartupFailure> = Err(StartupFailure::Unsupported);
        match result {
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Ok(child) => self.adopt(child),
            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            Ok(()) => {}
            Err(reason) => {
                eprintln!("island host: no_daemon ({})", reason.code());
                #[cfg(target_os = "macos")]
                match reason {
                    StartupFailure::NodeMissing => eprintln!("Falta Node.js: instalalo desde nodejs.org."),
                    StartupFailure::PiMissing => eprintln!("Falta Pi: instalalo con npm install -g @earendil-works/pi-coding-agent."),
                    _ => {}
                }
                crate::failure::report("daemon", "daemon-start", crate::failure::Reason::DaemonStartFailed, None);
                *self.failure.lock().expect("host failure lock") = Some(reason);
            }
        }
    }
    pub fn failure(&self) -> Option<StartupFailure> { *self.failure.lock().expect("host failure lock") }
    /// Exit status of a host child that died by itself (early exit, crash, external kill);
    /// `None` while it lives or after a normal `stop()`.
    pub fn exit_status(&self) -> Option<std::process::ExitStatus> { *self.exited.lock().expect("host exit lock") }
    /// The pid of the CURRENT app-owned child, or `None` when the spawn failed or the
    /// runtime reap already took a child that died. Read only for the restart verdict
    /// (#390 fix); it is never exposed to the page.
    pub fn pid(&self) -> Option<u32> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let pid = self.child.lock().expect("host child lock").as_ref().map(Host::pid);
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let pid = None;
        pid
    }
    /// Take ownership of a spawned host and reap it at runtime if it exits by itself:
    /// a bounded `try_wait` poll (no unbounded `wait`), so a dead daemon is never left
    /// as a zombie and its early exit becomes visible via [`exit_status`].
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn adopt(&self, host: Host) {
        *self.child.lock().expect("host child lock") = Some(host);
        let (child, exited) = (Arc::clone(&self.child), Arc::clone(&self.exited));
        std::thread::spawn(move || loop {
            std::thread::sleep(REAP_POLL);
            let Ok(mut slot) = child.lock() else { return };
            let Some(host) = slot.as_mut() else { return }; // stopped: nothing left to watch
            if let Ok(Some(status)) = host.try_wait() {
                eprintln!("island host: daemon exited by itself ({status}); reaped");
                crate::failure::report("daemon", "daemon-exited", crate::failure::Reason::DaemonExited, status.code().map(i64::from));
                if let Ok(mut out) = exited.lock() { *out = Some(status); }
                *slot = None; // Drop sees the child already reaped: no signal to a reused pid
                return;
            }
        });
    }
    pub fn stop(&self) {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        drop(self.child.lock().expect("host child lock").take());
    }
}

/// Session PATH first; only these canonical locations are tried afterwards.
/// Empty/relative PATH components are ignored: no working-directory execution.
pub fn discover(name: &str, path: Option<&OsStr>, home: Option<&OsStr>) -> Result<PathBuf, String> {
    let mut candidates: Vec<PathBuf> = path.map(std::env::split_paths).into_iter().flatten()
        .filter(|p| p.is_absolute()).map(|p| p.join(name)).collect();
    // Finder does not inherit shell npm prefixes. These are bounded, customary
    // user install locations, not a shell invocation or a recursive disk search.
    #[cfg(target_os = "macos")]
    if matches!(name, "node" | "pi") {
        if let Some(home) = home.map(Path::new).filter(|h| h.is_absolute()) {
            candidates.push(home.join(".local/bin").join(name));
            candidates.push(home.join(".npm-global/bin").join(name));
        }
    }
    match name {
        "node" => {
            #[cfg(target_os = "macos")]
            candidates.push(PathBuf::from("/opt/homebrew/bin/node"));
            candidates.extend([PathBuf::from("/usr/local/bin/node"), PathBuf::from("/usr/bin/node")]);
        }
        "pi" => {
            if let Some(home) = home.map(Path::new).filter(|h| h.is_absolute()) { candidates.push(home.join(".local/bin/pi")); }
            #[cfg(target_os = "macos")]
            candidates.push(PathBuf::from("/opt/homebrew/bin/pi"));
            candidates.extend([PathBuf::from("/usr/local/bin/pi"), PathBuf::from("/usr/bin/pi")]);
        }
        _ => {}
    }
    candidates.into_iter().find(|p| executable(p)).ok_or_else(|| format!("{name} missing (PATH and canonical locations)"))
}
fn executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    #[cfg(unix)]
    { use std::os::unix::fs::PermissionsExt; meta.is_file() && meta.permissions().mode() & 0o111 != 0 }
    #[cfg(not(unix))]
    { meta.is_file() }
}

/// Use the same absolute-only PATH search for Pi/SDK subprocesses as discovery.
/// Keeping a relative entry here would undo discover()'s no-CWD-execution rule.
pub fn runtime_path(path: Option<&OsStr>, node: &Path, pi: &Path) -> Result<std::ffi::OsString, std::env::JoinPathsError> {
    let mut dirs: Vec<PathBuf> = path.map(std::env::split_paths).into_iter().flatten()
        .filter(|p| p.is_absolute()).collect();
    for runtime in [node, pi] {
        if let Some(dir) = runtime.parent() { if !dirs.iter().any(|d| d == dir) { dirs.push(dir.to_owned()); } }
    }
    std::env::join_paths(dirs)
}

/// Release layout: Tauri puts resources beside MacOS/, not beside the binary.
/// Other platforms retain their adjacent desktop-daemon layout.
pub fn installed_daemon_cli(exe: &Path) -> Option<PathBuf> {
    let directory = exe.parent()?;
    #[cfg(target_os = "macos")]
    let directory = directory.parent()?.join("Resources");
    Some(directory.join("desktop-daemon/src/cli.cjs"))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn start() -> Result<Host, StartupFailure> {
    let path = std::env::var_os("PATH"); let home = std::env::var_os("HOME");
    let node = discover("node", path.as_deref(), home.as_deref()).map_err(|_| StartupFailure::NodeMissing)?;
    let pi = discover("pi", path.as_deref(), home.as_deref()).map_err(|_| StartupFailure::PiMissing)?;
    let exe = std::env::current_exe().map_err(|_| StartupFailure::HostMissing)?;
    let mut cli = installed_daemon_cli(&exe).ok_or(StartupFailure::HostMissing)?;
    #[cfg(debug_assertions)]
    if !cli.is_file() { cli = Path::new(env!("CARGO_MANIFEST_DIR")).join("../desktop-daemon/src/cli.cjs"); }
    if !cli.is_file() { return Err(StartupFailure::HostMissing) }
    let child_path = runtime_path(path.as_deref(), &node, &pi).map_err(|_| StartupFailure::SpawnFailed)?;
    let mut command = std::process::Command::new(node);
    command.arg(cli).args(["run", "--app-owned"]).env("PATH", child_path);
    Host::spawn_command(command).map_err(|_| StartupFailure::SpawnFailed)
}

#[cfg(target_os = "macos")]
#[path = "daemonhost_mac.rs"]
mod mac;
#[cfg(target_os = "macos")]
pub use mac::{sentinel_main, Host, SENTINEL_FLAG};

#[cfg(target_os = "linux")]
pub struct Host { child: std::process::Child }
#[cfg(target_os = "linux")]
impl Host {
    /// Private process group for owned cleanup, plus kernel death signal. No shell.
    pub fn spawn_command(mut command: std::process::Command) -> std::io::Result<Self> {
        use std::os::unix::process::CommandExt;
        let parent = std::process::id() as libc::pid_t;
        command.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::inherit());
        unsafe {
            command.pre_exec(move || {
                if libc::setpgid(0, 0) != 0 { return Err(std::io::Error::last_os_error()) }
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 { return Err(std::io::Error::last_os_error()) }
                // Parent may have died between fork and prctl. Never escape that race.
                if libc::getppid() != parent { return Err(std::io::Error::other("app exited before host spawn")) }
                Ok(())
            });
        }
        command.spawn().map(|child| Self { child })
    }
    pub fn pid(&self) -> u32 { self.child.id() }
    pub fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> { self.child.try_wait() }
}
#[cfg(target_os = "linux")]
impl Drop for Host {
    fn drop(&mut self) {
        // Never signal a reused process/group id after the child was already reaped.
        if self.child.try_wait().ok().flatten().is_some() { return }
        drop(self.child.stdin.take()); // EOF lease allows graceful shutdown
        let pid = self.child.id() as libc::pid_t;
        unsafe { libc::kill(-pid, libc::SIGTERM); }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() { return }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        unsafe { libc::kill(-pid, libc::SIGKILL); }
        // A child can move out of its original group. Target our unreaped
        // direct child as well; its PID cannot be reused while we still own it.
        let _ = self.child.kill();
        let reap_deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while std::time::Instant::now() < reap_deadline {
            if self.child.try_wait().ok().flatten().is_some() { return }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // Never block app exit in wait(). In an uninterruptible kernel wait,
        // PDEATHSIG remains the final backstop when this app process exits.
        eprintln!("island host: bounded reap expired; kernel parent-death protection remains active");
    }
}
