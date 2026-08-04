use std::ptr;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_BASIC_PROCESS_ID_LIST, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation, QueryInformationJobObject,
    SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

use crate::ProcessError;

pub(crate) struct JobObject {
    handle: HANDLE,
}
unsafe impl Send for JobObject {}

pub(crate) struct SuspendedPrimaryThread {
    handle: HANDLE,
}
// SAFETY: this value exclusively owns a Windows thread handle and is only used
// to resume the one suspended primary thread before normal supervision.
unsafe impl Send for SuspendedPrimaryThread {}

impl JobObject {
    pub(crate) fn create() -> Result<Self, ProcessError> {
        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err(ProcessError::Setup);
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&raw mut info).cast(),
                std::mem::size_of_val(&info) as u32,
            )
        };
        if ok == 0 {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(ProcessError::Setup);
        }
        Ok(Self { handle })
    }

    pub(crate) fn assign_process(&self, process: HANDLE) -> Result<(), ProcessError> {
        if unsafe { AssignProcessToJobObject(self.handle, process) } == 0 {
            Err(ProcessError::Setup)
        } else {
            Ok(())
        }
    }

    pub(crate) fn terminate_tree(&self) -> Result<(), ProcessError> {
        if unsafe { TerminateJobObject(self.handle, 1) } == 0 {
            Err(ProcessError::CleanupUnproven)
        } else {
            Ok(())
        }
    }

    pub(crate) fn is_empty(&self) -> Result<bool, ProcessError> {
        // A one-entry buffer is sufficient to prove emptiness. ERROR_MORE_DATA
        // means there are still owned members and therefore cleanup is not done.
        let mut list: JOBOBJECT_BASIC_PROCESS_ID_LIST = unsafe { std::mem::zeroed() };
        let ok = unsafe {
            QueryInformationJobObject(
                self.handle,
                JobObjectBasicProcessIdList,
                (&raw mut list).cast(),
                std::mem::size_of_val(&list) as u32,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            let error = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            if error == windows_sys::Win32::Foundation::ERROR_MORE_DATA {
                return Ok(false);
            }
            return Err(ProcessError::CleanupUnproven);
        }
        Ok(list.NumberOfAssignedProcesses == 0)
    }
}

impl Drop for JobObject {
    fn drop(&mut self) {
        if !self.handle.is_null() && self.handle != INVALID_HANDLE_VALUE {
            unsafe {
                let _ = CloseHandle(self.handle);
            }
            self.handle = ptr::null_mut();
        }
    }
}

impl SuspendedPrimaryThread {
    pub(crate) fn find_for_process(pid: u32) -> Result<Self, ProcessError> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(ProcessError::Setup);
        }
        let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
        let mut id = None;
        let mut present = unsafe { Thread32First(snapshot, &raw mut entry) } != 0;
        while present {
            if entry.th32OwnerProcessID == pid && id.replace(entry.th32ThreadID).is_some() {
                unsafe {
                    let _ = CloseHandle(snapshot);
                }
                return Err(ProcessError::Setup);
            }
            present = unsafe { Thread32Next(snapshot, &raw mut entry) } != 0;
        }
        unsafe {
            let _ = CloseHandle(snapshot);
        }
        let Some(id) = id else {
            return Err(ProcessError::Setup);
        };
        let handle = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, id) };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err(ProcessError::Setup);
        }
        Ok(Self { handle })
    }

    pub(crate) fn resume(&self) -> Result<(), ProcessError> {
        if unsafe { ResumeThread(self.handle) } != 1 {
            Err(ProcessError::Setup)
        } else {
            Ok(())
        }
    }
}

impl Drop for SuspendedPrimaryThread {
    fn drop(&mut self) {
        if !self.handle.is_null() && self.handle != INVALID_HANDLE_VALUE {
            unsafe {
                let _ = CloseHandle(self.handle);
            }
            self.handle = ptr::null_mut();
        }
    }
}
