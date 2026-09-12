use std::io;

#[cfg(windows)]
pub(crate) fn copy_text(text: &str) -> io::Result<()> {
    copy_native(text).or_else(|_| write_osc52(text))
}

#[cfg(windows)]
fn copy_native(text: &str) -> io::Result<()> {
    use std::mem::size_of;
    use std::ptr::copy_nonoverlapping;
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::{
        Console::GetConsoleWindow,
        DataExchange::{EmptyClipboard, OpenClipboard, SetClipboardData},
        Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
        Ole::CF_UNICODETEXT,
    };

    let mut utf16 = text.encode_utf16().collect::<Vec<_>>();
    utf16.push(0);
    let byte_len = utf16.len().checked_mul(size_of::<u16>()).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "clipboard text is too large")
    })?;
    unsafe {
        if OpenClipboard(GetConsoleWindow()) == 0 {
            return Err(io::Error::last_os_error());
        }
        let _clipboard = ClipboardGuard;
        if EmptyClipboard() == 0 {
            return Err(io::Error::last_os_error());
        }
        let memory = GlobalAlloc(GMEM_MOVEABLE, byte_len);
        if memory.is_null() {
            return Err(io::Error::last_os_error());
        }
        let locked = GlobalLock(memory);
        if locked.is_null() {
            GlobalFree(memory);
            return Err(io::Error::last_os_error());
        }
        copy_nonoverlapping(utf16.as_ptr(), locked.cast::<u16>(), utf16.len());
        GlobalUnlock(memory);
        if SetClipboardData(CF_UNICODETEXT as u32, memory).is_null() {
            GlobalFree(memory);
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(windows)]
struct ClipboardGuard;

#[cfg(windows)]
impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::System::DataExchange::CloseClipboard();
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn copy_text(text: &str) -> io::Result<()> {
    copy_with("pbcopy", &[], text).or_else(|_| write_osc52(text))
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn copy_text(text: &str) -> io::Result<()> {
    let commands: &[(&str, &[&str])] = &[
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
    ];
    for (program, arguments) in commands {
        match copy_with(program, arguments, text) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => return write_osc52(text),
        }
    }
    write_osc52(text)
}

#[cfg(unix)]
fn copy_with(program: &str, arguments: &[&str], text: &str) -> io::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("clipboard command stdin is piped")
        .write_all(text.as_bytes())?;
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "clipboard command exited with {status}"
        )))
    }
}

#[cfg(not(any(windows, unix)))]
pub(crate) fn copy_text(_text: &str) -> io::Result<()> {
    write_osc52(_text)
}

fn write_osc52(text: &str) -> io::Result<()> {
    use std::io::Write;

    let sequence = format!("\x1b]52;c;{}\x07", base64(text.as_bytes()));
    let mut stdout = io::stdout().lock();
    stdout.write_all(sequence.as_bytes())?;
    stdout.flush()
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        encoded.push(ALPHABET[usize::from(first >> 2)] as char);
        encoded.push(ALPHABET[usize::from((first & 0b11) << 4 | second >> 4)] as char);
        if chunk.len() > 1 {
            encoded.push(ALPHABET[usize::from((second & 0b1111) << 2 | third >> 6)] as char);
        } else {
            encoded.push('=');
        }
        if chunk.len() > 2 {
            encoded.push(ALPHABET[usize::from(third & 0b11_1111)] as char);
        } else {
            encoded.push('=');
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::base64;

    #[test]
    fn osc52_payload_uses_standard_base64() {
        assert_eq!(base64(b"hello"), "aGVsbG8=");
        assert_eq!(base64("界".as_bytes()), "55WM");
    }
}
