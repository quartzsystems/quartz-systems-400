// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Running host tools with logging and a useful failure message.

use std::collections::VecDeque;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

use anyhow::{Context, Result, bail};

use crate::ctx::Ctx;

/// Lines kept for the failure report.
const TAIL: usize = 40;

/// A host command with a label used for its log file.
pub struct Cmd {
    inner: Command,
    label: String,
    display: String,
}

impl Cmd {
    /// A new command. `label` becomes `target/logs/<label>.log`.
    pub fn new(program: impl AsRef<OsStr>, label: impl Into<String>) -> Self {
        let program = program.as_ref();
        Self {
            inner: Command::new(program),
            label: label.into(),
            display: program.to_string_lossy().into_owned(),
        }
    }

    /// Append one argument.
    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        let arg = arg.as_ref();
        self.display.push(' ');
        self.display.push_str(&arg.to_string_lossy());
        self.inner.arg(arg);
        self
    }

    /// Append several arguments.
    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for a in args {
            self.arg(a);
        }
        self
    }

    /// Set an environment variable for the child.
    pub fn env(&mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.inner.env(key, value);
        self
    }

    /// Set the working directory.
    pub fn cwd(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        self.inner.current_dir(dir);
        self
    }

    /// Run to completion. Output goes to the log file (and the terminal when
    /// verbose). On failure the error carries the last lines of output.
    pub fn run(&mut self, ctx: &Ctx) -> Result<()> {
        let logs = ctx.paths.logs();
        fs::create_dir_all(&logs)?;
        let log_path = logs.join(format!("{}.log", self.label));
        let mut log = File::create(&log_path)
            .with_context(|| format!("cannot create {}", log_path.display()))?;
        writeln!(log, "$ {}", self.display)?;
        tracing::debug!("$ {}", self.display);

        let mut child = self
            .inner
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("cannot start `{}`", self.display))?;

        let (tx, rx) = mpsc::channel::<String>();
        let readers = [
            child.stdout.take().map(|s| pump(Box::new(s), tx.clone())),
            child.stderr.take().map(|s| pump(Box::new(s), tx.clone())),
        ];
        drop(tx);

        let mut tail = VecDeque::with_capacity(TAIL + 1);
        for line in rx {
            writeln!(log, "{line}")?;
            if ctx.verbose {
                println!("{line}");
            }
            if tail.len() == TAIL {
                tail.pop_front();
            }
            tail.push_back(line);
        }
        for r in readers.into_iter().flatten() {
            let _ = r.join();
        }
        let status = child.wait()?;
        if !status.success() {
            let tail: Vec<String> = tail.into_iter().collect();
            bail!(
                "`{}` failed ({status})\nfull log: {}\nlast {} lines:\n{}",
                self.display,
                log_path.display(),
                tail.len(),
                tail.join("\n")
            );
        }
        Ok(())
    }

    /// Run and return stdout. For small, quick commands.
    pub fn capture(&mut self) -> Result<String> {
        let out = self
            .inner
            .stdin(Stdio::null())
            .output()
            .with_context(|| format!("cannot start `{}`", self.display))?;
        if !out.status.success() {
            bail!(
                "`{}` failed ({}): {}",
                self.display,
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Run and return stdout plus stderr, whatever the exit status.
    pub fn capture_all(&mut self) -> Result<(bool, String)> {
        let out = self
            .inner
            .stdin(Stdio::null())
            .output()
            .with_context(|| format!("cannot start `{}`", self.display))?;
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        Ok((out.status.success(), text))
    }
}

/// Forward lines from a pipe to the channel until EOF.
fn pump(reader: Box<dyn Read + Send>, tx: mpsc::Sender<String>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for chunk in BufReader::new(reader).split(b'\n').map_while(Result::ok) {
            let line = String::from_utf8_lossy(&chunk)
                .trim_end_matches('\r')
                .to_string();
            if tx.send(line).is_err() {
                break;
            }
        }
    })
}

/// Is `program` on `PATH`?
pub fn on_path(program: &str) -> bool {
    which(program).is_some()
}

/// Locate `program` on `PATH` (with `.exe` on Windows).
pub fn which(program: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    let names: &[String] = if cfg!(windows) {
        &[program.to_string(), format!("{program}.exe")]
    } else {
        &[program.to_string()]
    };
    std::env::split_paths(&path)
        .flat_map(|d| names.iter().map(move |n| d.join(n)))
        .find(|p| p.is_file())
}
