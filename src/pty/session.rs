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
    #[cfg(windows)]
    job: Option<WindowsJob>,
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
        #[cfg(windows)]
        let job = child
            .process_id()
            .and_then(|process_id| WindowsJob::attach(process_id).ok());
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
            #[cfg(windows)]
            job,
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
        #[cfg(windows)]
        if let Some(job) = &self.job {
            if job.terminate() {
                return Ok(());
            }
        }
        self.child.kill().map_err(PtySessionError::from)?;
        Ok(())
    }
}

#[cfg(windows)]
struct WindowsJob(windows_sys::Win32::Foundation::HANDLE);

// The handle is owned by one PtySession and all access is synchronized by the
// session manager. Moving the owning handle with the PTY is safe.
#[cfg(windows)]
unsafe impl Send for WindowsJob {}
#[cfg(windows)]
unsafe impl Sync for WindowsJob {}

#[cfg(windows)]
impl WindowsJob {
    fn attach(process_id: u32) -> Result<Self, ()> {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
        };

        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(());
        }
        let mut limits = unsafe { std::mem::zeroed::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast::<std::ffi::c_void>(),
                std::mem::size_of_val(&limits) as u32,
            )
        } != 0;
        let process = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA | PROCESS_TERMINATE,
                0,
                process_id,
            )
        };
        let assigned = configured
            && !process.is_null()
            && unsafe { AssignProcessToJobObject(job, process) } != 0;
        if !process.is_null() {
            unsafe {
                CloseHandle(process);
            }
        }
        if !assigned {
            unsafe {
                CloseHandle(job);
            }
            return Err(());
        }
        Ok(Self(job))
    }

    fn terminate(&self) -> bool {
        unsafe { windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0, 1) != 0 }
    }
}

#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
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
