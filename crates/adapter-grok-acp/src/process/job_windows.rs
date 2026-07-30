//! Windows Job Object ownership for the ACP process tree.
//!
//! Configures `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` so closing the job handle
//! terminates every process assigned to the job (parent and descendants).
//! Cleanup must not claim success by observing only the direct child.

use std::ptr;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};

use crate::error::internal;
use keeper_agent_protocol::RuntimeError;

/// Owned Windows Job Object with kill-on-close semantics.
pub struct JobObject {
    handle: HANDLE,
}

// SAFETY: Job object handles are usable across threads; we only move ownership.
unsafe impl Send for JobObject {}

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
