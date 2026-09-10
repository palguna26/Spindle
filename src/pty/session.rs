use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{self, Read, Write};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct PtyConfig {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
}

pub struct PtySession {
    child: Box<dyn Child + Send>,
    master: Box<dyn MasterPty + Send>,
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
}

#[derive(Debug)]
pub enum PtySessionError {
    Error(anyhow::Error),
}

impl From<io::Error> for PtySessionError {
    fn from(error: io::Error) -> Self {
        Self::Error(error.into())
    }
}

impl From<anyhow::Error> for PtySessionError {
    fn from(error: anyhow::Error) -> Self {
        Self::Error(error)
    }
}

impl PtySession {
    pub fn spawn(config: &PtyConfig) -> Result<Self, PtySessionError> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: config.rows,
                cols: config.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(PtySessionError::from)?;
        let mut command = CommandBuilder::new(&config.command);
        command.args(&config.args);
        command.cwd(Path::new(&config.cwd));
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(PtySessionError::from)?;
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(PtySessionError::from)?;
        let writer = pair.master.take_writer().map_err(PtySessionError::from)?;
        Ok(Self {
            child,
            master: pair.master,
            reader,
            writer,
        })
    }

    pub fn send_input(&mut self, input: &[u8]) -> Result<(), PtySessionError> {
        self.writer.write_all(input)?;
        self.writer.flush()?;
        Ok(())
    }

    pub fn read_output(&mut self, buffer: &mut [u8]) -> Result<usize, PtySessionError> {
        Ok(self.reader.read(buffer)?)
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), PtySessionError> {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(PtySessionError::from)?;
        Ok(())
    }

    pub fn try_wait(&mut self) -> Result<Option<u32>, PtySessionError> {
        Ok(self
            .child
            .try_wait()
            .map_err(PtySessionError::from)?
            .map(|status| status.exit_code()))
    }

    pub fn wait(&mut self) -> Result<u32, PtySessionError> {
        Ok(self
            .child
            .wait()
            .map_err(PtySessionError::from)?
            .exit_code())
    }

    pub fn stop(&mut self) -> Result<(), PtySessionError> {
        self.child.kill().map_err(PtySessionError::from)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{PtyConfig, PtySession};

    #[test]
    fn missing_command_returns_an_error() {
        let result = PtySession::spawn(&PtyConfig {
            command: "spindle-command-that-does-not-exist.exe".into(),
            args: vec![
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-Command".into(),
                "exit 0".into(),
            ],
            cwd: std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            cols: 80,
            rows: 24,
        });
        assert!(result.is_err());
    }
}
