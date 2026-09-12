use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

#[derive(Debug, Clone)]
pub struct PtyConfig {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: BTreeMap<String, String>,
    pub cols: u16,
    pub rows: u16,
}

pub struct PtySession {
    child: Box<dyn Child + Send>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    output: Receiver<Vec<u8>>,
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
        for (key, value) in &config.env {
            command.env(key, value);
        }
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(PtySessionError::from)?;
        drop(pair.slave);
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(PtySessionError::from)?;
        let writer = pair.master.take_writer().map_err(PtySessionError::from)?;
        let (output_sender, output) = mpsc::channel();
        thread::Builder::new()
            .name("spindle-pty-reader".into())
            .spawn(move || {
                let mut reader = reader;
                let mut buffer = [0_u8; 8192];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(size) => {
                            if output_sender.send(buffer[..size].to_vec()).is_err() {
                                break;
                            }
                        }
                    }
                }
            })
            .map_err(PtySessionError::from)?;
        Ok(Self {
            child,
            master: pair.master,
            writer,
            output,
        })
    }

    pub fn send_input(&mut self, input: &[u8]) -> Result<(), PtySessionError> {
        self.writer.write_all(input)?;
        self.writer.flush()?;
        Ok(())
    }

    pub fn try_read_output(&self) -> Result<Option<Vec<u8>>, PtySessionError> {
        match self.output.try_recv() {
            Ok(output) => Ok(Some(output)),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => Ok(None),
        }
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

    pub fn process_id(&self) -> Option<u32> {
        self.child.process_id()
    }

    pub fn wait(&mut self) -> Result<u32, PtySessionError> {
        Ok(self
            .child
            .wait()
            .map_err(PtySessionError::from)?
            .exit_code())
    }

    pub fn stop(&mut self) -> Result<(), PtySessionError> {
        let _ = self.writer.write_all(b"\x03");
        let _ = self.writer.flush();
        for _ in 0..10 {
            if self.try_wait()?.is_some() {
                return Ok(());
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }
        self.child.kill().map_err(PtySessionError::from)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{PtyConfig, PtySession};
    use std::collections::BTreeMap;

    #[test]
    fn missing_command_returns_an_error() {
        let result = PtySession::spawn(&PtyConfig {
            command: "spindle-command-that-does-not-exist.exe".into(),
            args: Vec::new(),
            cwd: std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            env: BTreeMap::new(),
            cols: 80,
            rows: 24,
        });
        assert!(result.is_err());
    }
}
