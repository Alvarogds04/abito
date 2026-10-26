//! Windows lifetime guarantee for the app-owned daemon host (W1a).
//!
//! The Linux arm uses PR_SET_PDEATHSIG + a private process group. The Windows
//! equivalent is a Job Object with `KILL_ON_JOB_CLOSE`: the only handle to the
//! job lives in the app process, so when the app dies for ANY reason the kernel
//! closes it and kills the whole tree (grandchildren included, because
//! `BREAKAWAY_OK` is never set). The child is created suspended and put in the
//! job BEFORE its first instruction runs, so it can never escape or fork out of
//! the job. Any failure to establish that guarantee is fail-closed: the child is
//! terminated and the spawn reports an error.
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Threading::{
    OpenThread, ResumeThread, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, CREATE_SUSPENDED,
    THREAD_SUSPEND_RESUME,
};

/// Owned kernel handle, stored as an integer so `Host` stays `Send`.
struct Owned(isize);
impl Owned {
    fn new(h: HANDLE) -> Option<Self> {
        (!h.is_null() && h != INVALID_HANDLE_VALUE).then_some(Self(h as isize))
    }
    fn raw(&self) -> HANDLE { self.0 as HANDLE }
}
impl Drop for Owned {
    fn drop(&mut self) { unsafe { CloseHandle(self.raw()); } }
}

pub struct Host { child: Child, job: Owned }

fn kill_job_object() -> std::io::Result<Owned> {
    let job = Owned::new(unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) })
        .ok_or_else(std::io::Error::last_os_error)?;
    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    info.BasicLimitInformation.LimitFlags =
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
    let ok = unsafe {
        SetInformationJobObject(
            job.raw(),
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if ok == 0 { return Err(std::io::Error::last_os_error()) }
    Ok(job)
}

/// Resume the (single) initial thread of a process created with CREATE_SUSPENDED.
fn resume_initial_thread(pid: u32) -> std::io::Result<()> {
    let snap = Owned::new(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) })
        .ok_or_else(std::io::Error::last_os_error)?;
    let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
    let mut more = unsafe { Thread32First(snap.raw(), &mut entry) } != 0;
    while more {
        if entry.th32OwnerProcessID == pid {
            let thread = Owned::new(unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) })
                .ok_or_else(std::io::Error::last_os_error)?;
            // ResumeThread returns the previous suspend count, or u32::MAX on failure.
            return match unsafe { ResumeThread(thread.raw()) } {
                u32::MAX => Err(std::io::Error::last_os_error()),
                _ => Ok(()),
            };
        }
        more = unsafe { Thread32Next(snap.raw(), &mut entry) } != 0;
    }
    Err(std::io::Error::other("suspended host thread not found"))
}

impl Host {
    /// Suspended create -> assign to a kill-on-close job -> resume. No shell.
    pub fn spawn_command(mut command: Command) -> std::io::Result<Self> {
        let job = kill_job_object()?;
        command.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::inherit());
        command.creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
        let mut child = command.spawn()?;
        let attach = (|| {
            if unsafe { AssignProcessToJobObject(job.raw(), child.as_raw_handle() as HANDLE) } == 0 {
                return Err(std::io::Error::last_os_error());
            }
            resume_initial_thread(child.id())
        })();
        if let Err(error) = attach {
            // Fail closed: a host outside the lifetime guarantee never runs.
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        Ok(Self { child, job })
    }
    pub fn pid(&self) -> u32 { self.child.id() }
    pub fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> { self.child.try_wait() }
}

impl Drop for Host {
    fn drop(&mut self) {
        // Never act on a child that was already reaped.
        if self.child.try_wait().ok().flatten().is_some() { return }
        drop(self.child.stdin.take()); // EOF lease allows graceful shutdown
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() { return }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        unsafe { TerminateJobObject(self.job.raw(), 1); }
        let reap_deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while std::time::Instant::now() < reap_deadline {
            if self.child.try_wait().ok().flatten().is_some() { return }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // Never block app exit; closing the job handle (field drop) kills the tree anyway.
        eprintln!("island host: bounded reap expired; the job object kills the tree on close");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::JobObjects::IsProcessInJob;

    fn sleeper() -> Command {
        let mut c = Command::new("ping");
        c.args(["-n", "60", "127.0.0.1"]);
        c
    }

    #[test]
    fn host_runs_inside_the_kill_on_close_job() {
        let host = Host::spawn_command(sleeper()).expect("spawn");
        let mut inside = 0;
        let ok = unsafe { IsProcessInJob(host.child.as_raw_handle() as HANDLE, host.job.raw(), &mut inside) };
        assert!(ok != 0 && inside != 0, "child must be in OUR job");
    }

    #[test]
    fn closing_the_job_kills_the_child_without_a_graceful_exit() {
        let mut host = Host::spawn_command(sleeper()).expect("spawn");
        let Host { child, job } = &mut host;
        unsafe { TerminateJobObject(job.raw(), 7) };
        let status = child.wait().expect("wait");
        assert_eq!(status.code(), Some(7));
    }

    #[test]
    fn dropping_the_host_does_not_leave_a_running_child() {
        let host = Host::spawn_command(sleeper()).expect("spawn");
        let pid = host.pid();
        drop(host);
        let out = std::process::Command::new("tasklist").args(["/FI", &format!("PID eq {pid}"), "/NH"]).output().unwrap();
        assert!(!String::from_utf8_lossy(&out.stdout).contains("ping.exe"));
    }
}
