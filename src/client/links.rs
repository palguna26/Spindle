use std::io;
use unicode_width::UnicodeWidthChar;

pub(crate) fn web_url_at_cell(screen: &str, row: u16, col: u16) -> Option<String> {
    let line = screen.lines().nth(usize::from(row))?;
    let cells = line
        .char_indices()
        .scan(0u16, |next_col, (byte, ch)| {
            let width = ch.width().unwrap_or(0) as u16;
            let start_col = if width == 0 {
                next_col.saturating_sub(1)
            } else {
                *next_col
            };
            if width > 0 {
                *next_col = next_col.saturating_add(width);
            }
            Some((byte, ch, start_col, next_col.saturating_sub(1)))
        })
        .collect::<Vec<_>>();

    for start in 0..cells.len() {
        let prefix = cells[start..]
            .iter()
            .map(|(_, ch, _, _)| *ch)
            .take(8)
            .collect::<String>();
        let prefix_len = if prefix.starts_with("https://") {
            8
        } else if prefix.starts_with("http://") {
            7
        } else {
            continue;
        };
        let mut end = start + prefix_len - 1;
        while end + 1 < cells.len() && !cells[end + 1].1.is_whitespace() {
            end += 1;
        }
        while end >= start && trim_url_tail(&cells, start, end) {
            if end == start {
                break;
            }
            end -= 1;
        }
        if cells[start].2 <= col && col <= cells[end].3 {
            let start_byte = cells[start].0;
            let end_byte = cells[end + 1..].first().map_or(line.len(), |cell| cell.0);
            let url = line.get(start_byte..end_byte)?;
            if is_safe_web_url(url) {
                return Some(url.to_owned());
            }
        }
    }
    None
}

fn trim_url_tail(cells: &[(usize, char, u16, u16)], start: usize, end: usize) -> bool {
    match cells[end].1 {
        '"' | '\'' | '`' | '.' | ',' | ';' | ':' | '!' | '?' => true,
        ')' => unmatched_closer(cells, start, end, '(', ')'),
        ']' => unmatched_closer(cells, start, end, '[', ']'),
        '}' => unmatched_closer(cells, start, end, '{', '}'),
        _ => false,
    }
}

fn unmatched_closer(
    cells: &[(usize, char, u16, u16)],
    start: usize,
    end: usize,
    opener: char,
    closer: char,
) -> bool {
    let opens = cells[start..=end]
        .iter()
        .filter(|cell| cell.1 == opener)
        .count();
    let closes = cells[start..=end]
        .iter()
        .filter(|cell| cell.1 == closer)
        .count();
    closes > opens
}

pub(crate) fn is_safe_web_url(url: &str) -> bool {
    (url.starts_with("http://") || url.starts_with("https://"))
        && !url.chars().any(char::is_control)
}

pub(crate) fn open_web_url(url: &str) -> io::Result<()> {
    if !is_safe_web_url(url) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "only http and https URLs can be opened",
        ));
    }

    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};

        let operation = "open\0".encode_utf16().collect::<Vec<_>>();
        let target = url
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if result as isize <= 32 {
            return Err(io::Error::other("Windows could not open the URL"));
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let opener = "xdg-open";
    #[cfg(not(windows))]
    {
        std::process::Command::new(opener).arg(url).spawn()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{open_web_url, web_url_at_cell};

    #[test]
    fn ctrl_click_targets_visible_http_urls_and_trims_sentence_punctuation() {
        let screen = "See https://example.test/a(b).\nnot a link";
        assert_eq!(
            web_url_at_cell(screen, 0, 18).as_deref(),
            Some("https://example.test/a(b)")
        );
        assert_eq!(web_url_at_cell(screen, 0, 3), None);
        assert_eq!(web_url_at_cell(screen, 1, 5), None);
    }

    #[test]
    fn link_hit_testing_uses_terminal_columns_for_wide_characters() {
        let screen = "界 https://example.test";
        assert_eq!(
            web_url_at_cell(screen, 0, 4).as_deref(),
            Some("https://example.test")
        );
        assert_eq!(web_url_at_cell(screen, 0, 2), None);
    }

    #[test]
    fn only_http_and_https_targets_can_open() {
        assert!(open_web_url("file:///tmp/report").is_err());
        assert!(open_web_url("javascript:alert(1)").is_err());
        assert!(open_web_url("https://example.test\nstart calc").is_err());
    }
}
