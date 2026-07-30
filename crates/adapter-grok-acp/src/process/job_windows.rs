//! Windows Job Object ownership for the ACP process tree.
//!
//! Configures `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` so closing the job handle
//! terminates every process assigned to the job (parent and descendants).
//! Cleanup must not claim success by observing only the direct child.

use std::ptr;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

use crate::error::internal;
use keeper_agent_protocol::RuntimeError;

/// Owned Windows Job Object with kill-on-close semantics.
pub struct JobObject {
    handle: HANDLE,
}

// SAFETY: Job object handles are usable across threads; we only move ownership.
unsafe impl Send for JobObject {}

/// Handle to the one primary thread created suspended with the child process.
pub struct SuspendedPrimaryThread {
    handle: HANDLE,
}

impl SuspendedPrimaryThread {
    /// Locate the only thread owned by a newly CREATE_SUSPENDED process.
    pub fn find_for_process(pid: u32) -> Result<Self, RuntimeError> {
        // SAFETY: captures a system-owned snapshot that is closed below on every path.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(internal("acp_primary_thread_discovery_failed"));
        }

        // SAFETY: THREADENTRY32 is a plain Win32 output structure. Its size is required.
        let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
        let mut thread_id = None;
        let mut multiple_threads = false;

        // SAFETY: snapshot is valid and entry points to initialized writable storage.
        let mut present = unsafe { Thread32First(snapshot, &raw mut entry) } != 0;
        while present {
            if entry.th32OwnerProcessID == pid {
                if thread_id.replace(entry.th32ThreadID).is_some() {
                    multiple_threads = true;
                    break;
                }
            }
            // SAFETY: snapshot and entry remain valid for the enumeration.
            present = unsafe { Thread32Next(snapshot, &raw mut entry) } != 0;
        }

        // SAFETY: snapshot was created above and is no longer needed.
        unsafe {
            let _ = CloseHandle(snapshot);
        }

        let Some(thread_id) = thread_id.filter(|_| !multiple_threads) else {
            return Err(internal("acp_primary_thread_discovery_failed"));
        };
        // SAFETY: thread_id was returned by ToolHelp for the still-suspended child.
        let handle = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, thread_id) };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err(internal("acp_primary_thread_discovery_failed"));
        }
        Ok(Self { handle })
    }

    /// Resume exactly the CREATE_SUSPENDED primary thread.
    pub fn resume(&self) -> Result<(), RuntimeError> {
        // SAFETY: handle is an owned thread handle opened with THREAD_SUSPEND_RESUME.
        let previous_suspend_count = unsafe { ResumeThread(self.handle) };
        if previous_suspend_count != 1 {
            return Err(internal("acp_primary_thread_resume_failed"));
        }
        Ok(())
    }
}

impl Drop for SuspendedPrimaryThread {
    fn drop(&mut self) {
        if self.handle.is_null() || self.handle == INVALID_HANDLE_VALUE {
            return;
        }
        // SAFETY: handle is exclusively owned by this value.
        unsafe {
            let _ = CloseHandle(self.handle);
        }
        self.handle = ptr::null_mut();
    }
}

impl JobObject {
    /// Create a job configured for kill-on-close (no breakaway).
    pub fn create() -> Result<Self, RuntimeError> {
        // SAFETY: null name/security creates an anonymous job object.
        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err(internal("acp_job_create_failed"));
        }

        // SAFETY: zeroed extended limit info is valid for SetInformationJobObject.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        // SAFETY: handle is a valid job object; size matches the info struct.
        let ok = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&raw mut info).cast(),
                std::mem::size_of_val(&info) as u32,
            )
        };
        if ok == 0 {
            // SAFETY: handle was created by us and not yet wrapped.
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(internal("acp_job_configure_failed"));
        }

        Ok(Self { handle })
    }

    /// Assign an OS process handle to this job.
    pub fn assign_process(&self, process: HANDLE) -> Result<(), RuntimeError> {
        // SAFETY: both handles are valid; assignment is idempotent for same job.
        let ok = unsafe { AssignProcessToJobObject(self.handle, process) };
        if ok == 0 {
            return Err(internal("acp_job_assign_failed"));
        }
        Ok(())
    }

    /// Force-terminate every process currently in the job.
    pub fn terminate_tree(&self) {
        // SAFETY: handle is a live job object owned by this struct.
        unsafe {
            let _ = TerminateJobObject(self.handle, 1);
        }
    }
}

impl Drop for JobObject {
    fn drop(&mut self) {
        if self.handle.is_null() || self.handle == INVALID_HANDLE_VALUE {
            return;
        }
        // kill-on-close: closing the last handle terminates remaining members.
        // SAFETY: handle is exclusively owned; clear after close to avoid double-free.
        unsafe {
            let _ = CloseHandle(self.handle);
        }
        self.handle = ptr::null_mut();
    }
}
