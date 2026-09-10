use std::io::{self, BufRead, Write};

pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
pub enum FrameError {
    Io(io::Error),
    TooLarge,
    Empty,
}

impl From<io::Error> for FrameError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn read_frame<R: BufRead>(reader: &mut R) -> Result<Vec<u8>, FrameError> {
    let mut frame = Vec::new();
    let read = reader.read_until(b'\n', &mut frame)?;
    if read == 0 || frame.is_empty() || frame == b"\n" {
        return Err(FrameError::Empty);
    }
    if frame.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge);
    }
    if frame.last() == Some(&b'\n') {
        frame.pop();
    }
    if frame.last() == Some(&b'\r') {
        frame.pop();
    }
    Ok(frame)
}

pub fn write_frame<W: Write>(writer: &mut W, frame: &[u8]) -> Result<(), FrameError> {
    if frame.is_empty() {
        return Err(FrameError::Empty);
    }
    if frame.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge);
    }
    writer.write_all(frame)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{read_frame, write_frame, FrameError, MAX_FRAME_BYTES};
    use std::io::Cursor;

    #[test]
    fn frames_are_newline_delimited() {
        let mut output = Vec::new();
        write_frame(&mut output, br#"{"op":"ping"}"#).unwrap();
        assert_eq!(output, b"{\"op\":\"ping\"}\n");
        assert_eq!(
            read_frame(&mut Cursor::new(output)).unwrap(),
            br#"{"op":"ping"}"#
        );
    }

    #[test]
    fn frames_are_bounded() {
        let oversized = vec![b'x'; MAX_FRAME_BYTES + 1];
        assert!(matches!(
            write_frame(&mut Vec::new(), &oversized),
            Err(FrameError::TooLarge)
        ));
    }
}
