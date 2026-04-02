use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;
#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_INSUFFICIENT_BUFFER,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    SE_FILE_OBJECT,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::Security::{
    GetTokenInformation, SetFileSecurityW, TokenUser, DACL_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, TOKEN_QUERY, TOKEN_USER,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

#[cfg(unix)]
const PRIVATE_DIR_MODE: u32 = 0o700;
#[cfg(unix)]
const PRIVATE_FILE_MODE: u32 = 0o600;

#[derive(Clone, Copy)]
enum AtomicWriteMode {
    Standard,
    Private,
}

#[cfg(target_os = "windows")]
#[derive(Clone, Copy)]
enum PrivateObjectKind {
    File,
    Directory,
}

pub fn ensure_private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    set_private_dir_permissions(path)
}

pub fn write_atomic_file(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    write_file_atomically(path, contents.as_ref(), AtomicWriteMode::Standard)
}

pub fn write_private_file(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    write_file_atomically(path, contents.as_ref(), AtomicWriteMode::Private)
}

#[cfg(unix)]
fn open_temp_file(path: &Path, mode: AtomicWriteMode) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    if matches!(mode, AtomicWriteMode::Private) {
        options.mode(PRIVATE_FILE_MODE);
    }
    options.open(path)
}

#[cfg(not(unix))]
fn open_temp_file(path: &Path, _mode: AtomicWriteMode) -> io::Result<File> {
    OpenOptions::new().create_new(true).write(true).open(path)
}

fn write_file_atomically(path: &Path, contents: &[u8], mode: AtomicWriteMode) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let (temp_path, mut file) = create_temp_file(path, mode)?;
    let result = (|| -> io::Result<()> {
        file.write_all(contents)?;
        apply_target_permissions(path, &temp_path, mode)?;
        file.sync_all()?;
        drop(file);
        replace_file(&temp_path, path)?;
        sync_parent_dir(path)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }

    result
}

fn create_temp_file(path: &Path, mode: AtomicWriteMode) -> io::Result<(PathBuf, File)> {
    for _ in 0..32 {
        let temp_path = temp_file_path(path);
        match open_temp_file(&temp_path, mode) {
            Ok(file) => return Ok((temp_path, file)),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "Failed to allocate a temporary file name.",
    ))
}

fn temp_file_path(path: &Path) -> PathBuf {
    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

    let suffix = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let file_name = path.file_name().unwrap_or_default();
    let mut temp_name = OsString::from(".");
    temp_name.push(file_name);
    temp_name.push(format!(".tmp-{}-{suffix}", process::id()));

    path.with_file_name(temp_name)
}

fn apply_target_permissions(
    path: &Path,
    temp_path: &Path,
    mode: AtomicWriteMode,
) -> io::Result<()> {
    match mode {
        AtomicWriteMode::Private => set_private_file_permissions(temp_path),
        AtomicWriteMode::Standard => copy_existing_permissions(path, temp_path),
    }
}

fn copy_existing_permissions(path: &Path, temp_path: &Path) -> io::Result<()> {
    let permissions = match fs::metadata(path) {
        Ok(metadata) => metadata.permissions(),
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };

    fs::set_permissions(temp_path, permissions)
}

#[cfg(not(target_os = "windows"))]
fn replace_file(temp_path: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(temp_path, destination)
}

#[cfg(target_os = "windows")]
fn replace_file(temp_path: &Path, destination: &Path) -> io::Result<()> {
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;

    let temp_wide = wide_path(temp_path);
    let destination_wide = wide_path(destination);

    let result = unsafe {
        MoveFileExW(
            temp_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(target_os = "windows")]
unsafe extern "system" {
    fn MoveFileExW(existing_file_name: *const u16, new_file_name: *const u16, flags: u32) -> i32;
}

#[cfg(unix)]
fn sync_parent_dir(path: &Path) -> io::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_dir(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE_DIR_MODE))
}

#[cfg(target_os = "windows")]
fn set_private_dir_permissions(path: &Path) -> io::Result<()> {
    apply_private_windows_dacl(path, PrivateObjectKind::Directory)
}

#[cfg(not(any(unix, target_os = "windows")))]
fn set_private_dir_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE_FILE_MODE))
}

#[cfg(target_os = "windows")]
fn set_private_file_permissions(path: &Path) -> io::Result<()> {
    apply_private_windows_dacl(path, PrivateObjectKind::File)
}

#[cfg(not(any(unix, target_os = "windows")))]
fn set_private_file_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(target_os = "windows")]
fn apply_private_windows_dacl(path: &Path, kind: PrivateObjectKind) -> io::Result<()> {
    let user_sid = current_user_sid_string()?;
    let sddl = match kind {
        PrivateObjectKind::File => format!("D:P(A;;FA;;;SY)(A;;FA;;;{user_sid})"),
        PrivateObjectKind::Directory => format!("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;{user_sid})"),
    };
    apply_security_descriptor(path, &sddl)
}

#[cfg(target_os = "windows")]
fn current_user_sid_string() -> io::Result<String> {
    let token = current_process_token()?;
    let mut required_size = 0u32;
    let probe_result = unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut required_size,
        )
    };
    if probe_result != 0 {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Unexpectedly read token information without a buffer.",
        ));
    }

    let error = unsafe { GetLastError() };
    if error != ERROR_INSUFFICIENT_BUFFER {
        return Err(io::Error::from_raw_os_error(error as i32));
    }

    let mut buffer = vec![0u8; required_size as usize];
    let result = unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            required_size,
            &mut required_size,
        )
    };
    if result == 0 {
        return Err(last_windows_error());
    }

    let token_user = unsafe { &*(buffer.as_ptr().cast::<TOKEN_USER>()) };
    sid_to_string(token_user.User.Sid)
}

#[cfg(target_os = "windows")]
fn current_process_token() -> io::Result<OwnedHandle> {
    let mut handle = 0isize;
    let result = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) };
    if result == 0 {
        Err(last_windows_error())
    } else {
        Ok(OwnedHandle(handle))
    }
}

#[cfg(target_os = "windows")]
fn sid_to_string(sid: *mut core::ffi::c_void) -> io::Result<String> {
    let mut sid_string = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut sid_string) } == 0 {
        return Err(last_windows_error());
    }

    let sid_string = LocalBuffer(sid_string.cast());
    Ok(utf16_ptr_to_string(sid_string.0))
}

#[cfg(target_os = "windows")]
fn apply_security_descriptor(path: &Path, sddl: &str) -> io::Result<()> {
    let wide_sddl = wide_string(sddl);
    let mut security_descriptor = std::ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide_sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut security_descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(last_windows_error());
    }

    let security_descriptor = LocalBuffer(security_descriptor.cast());
    let path_wide = wide_path(path);
    if unsafe {
        SetFileSecurityW(
            path_wide.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            security_descriptor.0.cast(),
        )
    } == 0
    {
        Err(last_windows_error())
    } else {
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn wide_string(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "windows")]
fn utf16_ptr_to_string(ptr: *const u16) -> String {
    let mut len = 0usize;
    while unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(ptr, len) })
}

#[cfg(target_os = "windows")]
fn last_windows_error() -> io::Error {
    let code = unsafe { GetLastError() } as i32;
    if code == 0 {
        io::Error::new(io::ErrorKind::Other, "unknown Windows error")
    } else {
        io::Error::from_raw_os_error(code)
    }
}

#[cfg(target_os = "windows")]
struct OwnedHandle(isize);

#[cfg(target_os = "windows")]
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(target_os = "windows")]
struct LocalBuffer<T>(*mut T);

#[cfg(target_os = "windows")]
impl<T> Drop for LocalBuffer<T> {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0.cast());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{write_atomic_file, write_private_file};
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::process;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_test_dir() -> PathBuf {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);

        let path = std::env::temp_dir().join(format!(
            "keycord-secure-fs-test-{}-{}",
            process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create temporary test directory");
        path
    }

    #[test]
    fn atomic_writes_replace_existing_contents() {
        let dir = temp_test_dir();
        let path = dir.join("entry.gpg");

        write_atomic_file(&path, b"first").expect("write initial contents");
        write_atomic_file(&path, b"second").expect("replace contents");

        assert_eq!(fs::read(&path).expect("read final contents"), b"second");
        assert_eq!(fs::read_dir(&dir).expect("list temp dir").count(), 1);

        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_writes_preserve_existing_permissions() {
        let dir = temp_test_dir();
        let path = dir.join("entry.gpg");

        fs::write(&path, b"first").expect("write initial file");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640))
            .expect("set initial permissions");
        write_atomic_file(&path, b"second").expect("replace contents");

        let mode = fs::metadata(&path)
            .expect("read metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o640);

        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn private_writes_force_private_permissions() {
        let dir = temp_test_dir();
        let path = dir.join("secret.toml");

        write_private_file(&path, b"secret").expect("write private file");

        let mode = fs::metadata(&path)
            .expect("read metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);

        let _ = fs::remove_dir_all(dir);
    }
}
