// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A minimal QMP client over a Unix socket: capabilities, screendump
//! and the link of a network device.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

/// A connected QMP session.
pub struct Qmp {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Qmp {
    /// Connect and negotiate capabilities.
    pub fn connect(path: &Path) -> Result<Self, String> {
        let stream = UnixStream::connect(path)
            .map_err(|e| format!("QMP connect {}: {e}", path.display()))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .map_err(|e| e.to_string())?;
        let writer = stream.try_clone().map_err(|e| e.to_string())?;
        let mut q = Self {
            reader: BufReader::new(stream),
            writer,
        };
        q.read_line()?; // greeting
        q.execute(&json!({"execute": "qmp_capabilities"}))?;
        Ok(q)
    }

    /// Take a screenshot. PNG when this QEMU supports it, else PPM.
    /// Returns the format written.
    pub fn screendump(&mut self, file: &Path) -> Result<&'static str, String> {
        let png = json!({
            "execute": "screendump",
            "arguments": {"filename": file.to_string_lossy(), "format": "png"}
        });
        if self.execute(&png).is_ok() {
            return Ok("png");
        }
        self.execute(&json!({
            "execute": "screendump",
            "arguments": {"filename": file.to_string_lossy()}
        }))?;
        Ok("ppm")
    }

    /// Set a network device's link up or down (`set_link`): the guest
    /// sees its carrier come and go, as with a pulled cable.
    pub fn set_link(&mut self, name: &str, up: bool) -> Result<(), String> {
        self.execute(&json!({
            "execute": "set_link",
            "arguments": {"name": name, "up": up}
        }))
        .map(|_| ())
    }

    /// A human monitor command (`hostfwd_add ...`); its output.
    pub fn hmp(&mut self, line: &str) -> Result<String, String> {
        let v = self.execute(&json!({
            "execute": "human-monitor-command",
            "arguments": {"command-line": line}
        }))?;
        Ok(v.as_str().unwrap_or_default().to_string())
    }

    /// Send a command and wait for its return value.
    fn execute(&mut self, cmd: &Value) -> Result<Value, String> {
        let text = cmd.to_string();
        self.writer
            .write_all(format!("{text}\n").as_bytes())
            .map_err(|e| format!("QMP write: {e}"))?;
        loop {
            let line = self.read_line()?;
            let v: Value = serde_json::from_str(&line).map_err(|e| format!("QMP parse: {e}"))?;
            if let Some(err) = v.get("error") {
                return Err(format!("QMP error: {err}"));
            }
            if v.get("return").is_some() {
                return Ok(v["return"].clone());
            }
            // Asynchronous events are skipped.
        }
    }

    fn read_line(&mut self) -> Result<String, String> {
        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .map_err(|e| format!("QMP read: {e}"))?;
        if line.is_empty() {
            return Err("QMP connection closed".into());
        }
        Ok(line)
    }
}
