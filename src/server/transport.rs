//! Platform-specific local control transport.

#[cfg(windows)]
use std::fs::File;
#[cfg(windows)]
use std::io;
#[cfg(windows)]
use std::os::windows::io::FromRawHandle;
#[cfg(windows)]
use std::path::Path;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{
    GetLastError, LocalFree, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE,
};
#[cfg(windows)]
use windows_sys::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
#[cfg(windows)]
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
};
#[cfg(windows)]
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};

#[cfg(windows)]
const ERROR_PIPE_CONNECTED: u32 = 535;

#[cfg(windows)]
pub fn endpoint(state_dir: &Path) -> String {
    let id = state_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("default");
    format!(r"\\.\pipe\spindle-{id}")
}

#[cfg(windows)]
pub fn accept(name: &str) -> io::Result<File> {
    let wide = wide(name);
    let security = security_attributes()?;
    let handle = unsafe {
        CreateNamedPipeW(
            wide.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            1024 * 1024,
            1024 * 1024,
            0,
            &security,
        )
    };
    unsafe { LocalFree(security.lpSecurityDescriptor) };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }

    let connected = unsafe { ConnectNamedPipe(handle, std::ptr::null_mut()) } != 0;
    if !connected && unsafe { GetLastError() } != ERROR_PIPE_CONNECTED {
        let error = io::Error::last_os_error();
        unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
        return Err(error);
    }

    Ok(unsafe { File::from_raw_handle(handle as _) })
}

#[cfg(windows)]
fn security_attributes() -> io::Result<SECURITY_ATTRIBUTES> {
    let descriptor = wide("D:(A;;GA;;;OW)");
    let mut security_descriptor = std::ptr::null_mut();
    let mut descriptor_size = 0;
    let result = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            descriptor.as_ptr(),
            1,
            &mut security_descriptor,
            &mut descriptor_size,
        )
    };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: security_descriptor,
        bInheritHandle: 0,
    })
}

#[cfg(windows)]
pub fn connect(name: &str) -> io::Result<File> {
    let wide = wide(name);
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_handle(handle as _) })
    }
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
