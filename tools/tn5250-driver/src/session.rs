// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One connection: the socket, the negotiator, the terminal, and the
//! pump that moves records between them. Keys go through here so the
//! outbox is flushed and pub400's throttle is honored.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use tn5250::ds::wdsf::{Capabilities, Model};
use tn5250::record::Record;
use tn5250::telnet;
use tn5250::telnet::client::{Client, Config, Encoding};
use tn5250::term::{Key, KeyResult, Terminal};

use crate::env::Credentials;
use crate::trace::Trace;
use crate::{ModelArg, RunArgs, Target};

/// A live session.
pub struct Session {
    stream: TcpStream,
    client: Client,
    /// The terminal.
    pub term: Terminal,
    /// The trace sink.
    pub trace: Trace,
    throttle: Duration,
    last_send: Instant,
    /// The TN5250E start-up response, decoded, once it arrived.
    pub startup: Option<String>,
    /// Records from the host so far.
    pub records_in: usize,
}

impl Session {
    /// Connect and negotiate.
    pub fn connect(args: &RunArgs, creds: &Credentials) -> Result<Self> {
        let addr = (creds.host.as_str(), creds.port)
            .to_socket_addrs()
            .with_context(|| format!("resolving {}", creds.host))?
            .next()
            .context("no address")?;
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(15))
            .with_context(|| format!("connecting to {addr}"))?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_millis(100)))?;
        let model = match args.model {
            ModelArg::M3179 => Model::M3179,
            ModelArg::M3477 => Model::M3477Fc,
        };
        let user = args.user.clone().unwrap_or_else(|| creds.user.clone());
        let password = args
            .password
            .clone()
            .unwrap_or_else(|| creds.password.clone());
        let cfg = Config {
            terminal_type: model.terminal_type().into(),
            device: args.device.clone(),
            user: if args.auto_signon { Some(user) } else { None },
            password: if args.auto_signon {
                Some(password)
            } else {
                None
            },
            encoding: match args.encoding {
                crate::Encoding::Des => Encoding::Des,
                crate::Encoding::Sha => Encoding::Sha,
            },
            ..Config::default()
        };
        let throttle = if args.target == Target::Pub400 {
            Duration::from_millis(500)
        } else {
            Duration::ZERO
        };
        let mut s = Self {
            stream,
            client: Client::new(cfg),
            term: Terminal::new(Capabilities {
                model,
                gui: args.gui,
                color: true,
            }),
            trace: Trace::new(args.trace, args.hex),
            throttle,
            last_send: Instant::now(),
            startup: None,
            records_in: 0,
        };
        let deadline = Instant::now() + Duration::from_secs(20);
        while !s.client.negotiated {
            if Instant::now() > deadline {
                bail!("negotiation did not complete: {:?}", s.client.trace);
            }
            s.pump()?;
        }
        for line in std::mem::take(&mut s.client.trace) {
            s.trace.negotiation(&line);
        }
        Ok(s)
    }

    /// Read once (up to the socket timeout), answer, apply. Whether
    /// anything arrived.
    pub fn pump(&mut self) -> Result<bool> {
        let mut buf = vec![0u8; 65536];
        let n = match self.stream.read(&mut buf) {
            Ok(0) => bail!("the host closed the connection"),
            Ok(n) => n,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Ok(false);
            }
            Err(e) => return Err(e.into()),
        };
        let (reply, records) = self.client.feed(&buf[..n]);
        for line in std::mem::take(&mut self.client.trace) {
            self.trace.negotiation(&line);
        }
        if !reply.is_empty() {
            self.stream.write_all(&reply)?;
        }
        for bytes in records {
            self.records_in += 1;
            let rec = match Record::decode(&bytes) {
                Ok(r) => r,
                Err(e) => {
                    self.trace.note(&format!("bad record: {e}"));
                    continue;
                }
            };
            self.trace.inbound_record(&rec, &bytes);
            let first = rec.data.first();
            if first.is_some()
                && first != Some(&tn5250::ds::ESC)
                && self.startup.is_none()
                && self.records_in <= 2
            {
                let text = ccsid::from_ebcdic(&rec.data);
                let decoded = format!("startup response: {}", text.trim());
                self.trace.note(&decoded);
                self.startup = Some(text);
                continue;
            }
            match self.term.apply(&rec) {
                Ok(cmds) => self.trace.commands(&cmds),
                Err(e) => self.trace.note(&format!("apply: {e}")),
            }
            self.flush()?;
        }
        Ok(true)
    }

    /// Send what the terminal queued.
    pub fn flush(&mut self) -> Result<()> {
        for rec in self.term.take_outbox() {
            let wait = self.throttle.saturating_sub(self.last_send.elapsed());
            if !wait.is_zero() {
                std::thread::sleep(wait);
            }
            let bytes = rec.encode();
            let hidden = self.term.hidden_fields();
            self.trace.outbound_record(&rec, &bytes, &hidden);
            self.stream.write_all(&telnet::record(&bytes))?;
            self.last_send = Instant::now();
        }
        Ok(())
    }

    /// Pump until the predicate holds or the time is up.
    pub fn wait_for(
        &mut self,
        timeout: Duration,
        pred: impl Fn(&Terminal) -> bool,
    ) -> Result<bool> {
        let deadline = Instant::now() + timeout;
        loop {
            if pred(&self.term) {
                return Ok(true);
            }
            if Instant::now() > deadline {
                return Ok(false);
            }
            self.pump()?;
        }
    }

    /// Pump until the keyboard is free and the host has nothing more
    /// in flight (no record for `settle`).
    pub fn wait_unlocked(&mut self, timeout: Duration) -> Result<bool> {
        if !self.wait_for(timeout, |t| !t.is_locked())? {
            return Ok(false);
        }
        self.settle(Duration::from_millis(150))?;
        Ok(!self.term.is_locked())
    }

    /// Pump until nothing arrives for `quiet`.
    pub fn settle(&mut self, quiet: Duration) -> Result<()> {
        let mut last = Instant::now();
        loop {
            if self.pump()? {
                last = Instant::now();
            } else if last.elapsed() >= quiet {
                return Ok(());
            }
        }
    }

    /// Press a key and send what it produced.
    pub fn key(&mut self, k: Key) -> Result<KeyResult> {
        let r = self.term.key(k);
        self.flush()?;
        Ok(r)
    }

    /// Type text.
    pub fn type_str(&mut self, s: &str) -> Result<KeyResult> {
        let r = self.term.type_str(s);
        self.flush()?;
        Ok(r)
    }

    /// Close.
    pub fn disconnect(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

/// `run`: connect, run the scripts in order, report.
pub fn run(args: &RunArgs, creds: &Credentials) -> Result<()> {
    let mut s = Session::connect(args, creds)?;
    let mut ctx = crate::script::Context::new(args, creds);
    let mut failures = 0;
    for path in &args.scripts {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let name = path
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        ctx.script.clone_from(&name);
        match crate::script::run(&mut s, &mut ctx, &text) {
            Ok(()) => println!("{name}: ok"),
            Err(e) => {
                failures += 1;
                println!("{name}: FAILED: {e:#}");
                println!("--- screen ---");
                for row in s.term.text_rows() {
                    println!("|{row}|");
                }
                println!("--------------");
                if !ctx.continue_on_error {
                    break;
                }
            }
        }
    }
    s.disconnect();
    ctx.finish()?;
    if failures > 0 {
        bail!("{failures} script(s) failed");
    }
    Ok(())
}
