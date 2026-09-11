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
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            return if frame.is_empty() {
                Err(FrameError::Empty)
            } else {
                break;
            };
        }
        let consumed = chunk
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|position| position + 1)
            .unwrap_or(chunk.len());
        if frame.len() + consumed > MAX_FRAME_BYTES {
            return Err(FrameError::TooLarge);
        }
        let has_newline = chunk[..consumed].contains(&b'\n');
        frame.extend_from_slice(&chunk[..consumed]);
        reader.consume(consumed);
        if has_newline {
            break;
        }
    }
    if frame.is_empty() || frame == b"\n" {
        return Err(FrameError::Empty);
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

    #[test]
    fn oversized_unterminated_frames_are_rejected_while_reading() {
        let oversized = vec![b'x'; MAX_FRAME_BYTES + 1];
        assert!(matches!(
            read_frame(&mut Cursor::new(oversized)),
            Err(FrameError::TooLarge)
        ));
    }
}
