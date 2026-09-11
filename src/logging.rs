use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize)]
struct LogRecord<'a> {
    timestamp_unix_seconds: u64,
    level: &'a str,
    event: &'a str,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    fields: BTreeMap<String, String>,
}

pub struct Logger {
    file: Mutex<File>,
}

impl Logger {
    pub fn new(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(Self {
            file: Mutex::new(OpenOptions::new().create(true).append(true).open(path)?),
        })
    }

    pub fn info(&self, event: &str, fields: BTreeMap<String, String>) -> io::Result<()> {
        let record = LogRecord {
            timestamp_unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            level: "info",
            event,
            fields,
        };
        let mut file = self.file.lock().expect("logger lock poisoned");
        serde_json::to_writer(&mut *file, &record).map_err(io::Error::other)?;
        file.write_all(b"\n")?;
        file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::Logger;
    use std::collections::BTreeMap;

    #[test]
    fn writes_json_lines_without_raw_output_payloads() {
        let path = std::env::temp_dir().join(format!("spindle-log-{}.jsonl", std::process::id()));
        let logger = Logger::new(&path).unwrap();
        let mut fields = BTreeMap::new();
        fields.insert("pid".into(), "42".into());
        logger.info("server_started", fields).unwrap();
        let record: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(record["event"], "server_started");
        assert_eq!(record["fields"]["pid"], "42");
        assert!(record.get("bytes").is_none());
        std::fs::remove_file(path).unwrap();
    }
}
