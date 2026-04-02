use std::io;

pub fn apply_process_hardening() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        linux::apply()
    }

    #[cfg(target_os = "windows")]
    {
        windows::apply()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::io;

    pub(super) fn apply() -> Result<(), String> {
        let mut errors = Vec::new();

        if let Err(err) = disable_core_dumps() {
            errors.push(format!("disable core dumps: {err}"));
        }
        if let Err(err) = disable_process_dumpability() {
            errors.push(format!("disable process dumpability: {err}"));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn disable_core_dumps() -> io::Result<()> {
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn disable_process_dumpability() -> io::Result<()> {
        if unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::io;
    use std::mem;
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::SystemServices::{
        PROCESS_MITIGATION_ASLR_POLICY, PROCESS_MITIGATION_EXTENSION_POINT_DISABLE_POLICY,
        PROCESS_MITIGATION_STRICT_HANDLE_CHECK_POLICY,
    };
    use windows_sys::Win32::System::Threading::{
        ProcessASLRPolicy, ProcessExtensionPointDisablePolicy, ProcessStrictHandleCheckPolicy,
        SetProcessDEPPolicy, SetProcessMitigationPolicy, PROCESS_DEP_DISABLE_ATL_THUNK_EMULATION,
        PROCESS_DEP_ENABLE,
    };

    const ENABLE_BOTTOM_UP_ASLR: u32 = 0b0001;
    const ENABLE_HIGH_ENTROPY_ASLR: u32 = 0b0100;
    const DISABLE_EXTENSION_POINTS: u32 = 0b0001;
    const RAISE_ON_INVALID_HANDLE: u32 = 0b0001;
    const HANDLE_EXCEPTIONS_PERMANENT: u32 = 0b0010;

    pub(super) fn apply() -> Result<(), String> {
        let mut errors = Vec::new();

        #[cfg(target_pointer_width = "32")]
        if let Err(err) = enable_dep() {
            errors.push(format!("enable DEP: {err}"));
        }

        if let Err(err) = enable_aslr() {
            errors.push(format!("enable ASLR policy: {err}"));
        }
        if let Err(err) = disable_extension_points() {
            errors.push(format!("disable extension points: {err}"));
        }
        if let Err(err) = enable_strict_handle_checks() {
            errors.push(format!("enable strict handle checks: {err}"));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    #[cfg(target_pointer_width = "32")]
    fn enable_dep() -> io::Result<()> {
        if unsafe {
            SetProcessDEPPolicy(PROCESS_DEP_ENABLE | PROCESS_DEP_DISABLE_ATL_THUNK_EMULATION)
        } != 0
        {
            Ok(())
        } else {
            Err(last_os_error())
        }
    }

    fn enable_aslr() -> io::Result<()> {
        let mut policy = PROCESS_MITIGATION_ASLR_POLICY::default();
        unsafe {
            policy.Anonymous.Flags = ENABLE_BOTTOM_UP_ASLR | ENABLE_HIGH_ENTROPY_ASLR;
        }
        set_process_mitigation(ProcessASLRPolicy, &policy)
    }

    fn disable_extension_points() -> io::Result<()> {
        let mut policy = PROCESS_MITIGATION_EXTENSION_POINT_DISABLE_POLICY::default();
        unsafe {
            policy.Anonymous.Flags = DISABLE_EXTENSION_POINTS;
        }
        set_process_mitigation(ProcessExtensionPointDisablePolicy, &policy)
    }

    fn enable_strict_handle_checks() -> io::Result<()> {
        let mut policy = PROCESS_MITIGATION_STRICT_HANDLE_CHECK_POLICY::default();
        unsafe {
            policy.Anonymous.Flags = RAISE_ON_INVALID_HANDLE | HANDLE_EXCEPTIONS_PERMANENT;
        }
        set_process_mitigation(ProcessStrictHandleCheckPolicy, &policy)
    }

    fn set_process_mitigation<T>(policy: i32, value: &T) -> io::Result<()> {
        if unsafe {
            SetProcessMitigationPolicy(policy, value as *const T as *const _, mem::size_of::<T>())
        } != 0
        {
            Ok(())
        } else {
            Err(last_os_error())
        }
    }

    fn last_os_error() -> io::Error {
        let code = unsafe { GetLastError() } as i32;
        if code == 0 {
            io::Error::new(io::ErrorKind::Other, "unknown Windows hardening error")
        } else {
            io::Error::from_raw_os_error(code)
        }
    }
}
