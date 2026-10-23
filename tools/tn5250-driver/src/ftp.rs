// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A small FTP client (RFC 959, passive mode) for the oracle's server:
//! listing, text and binary transfers, and `quote` for `SITE` and
//! `RCMD`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::env::Credentials;

/// The control connection.
pub struct Ftp {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    /// Every reply line, for traces.
    pub log: Vec<String>,
}

impl Ftp {
    /// Connect and log in.
    pub fn connect(host: &str, user: &str, password: &str) -> Result<Self> {
        let addr = (host, 21)
            .to_socket_addrs()?
            .next()
            .with_context(|| format!("resolving {host}"))?;
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(15))?;
        stream.set_read_timeout(Some(Duration::from_secs(60)))?;
        let writer = stream.try_clone()?;
        let mut f = Self {
            reader: BufReader::new(stream),
            writer,
            log: Vec::new(),
        };
        f.reply()?;
        let (code, _) = f.command(&format!("USER {user}"))?;
        if code == 331 {
            let (code, text) = f.command(&format!("PASS {password}"))?;
            if code != 230 {
                bail!("login refused: {code} {text}");
            }
        } else if code != 230 {
            bail!("USER refused: {code}");
        }
        Ok(f)
    }

    /// Read one reply (multi-line aware): its code and text.
    fn reply(&mut self) -> Result<(u16, String)> {
        let mut text = String::new();
        let mut code;
        loop {
            let mut line = String::new();
            if self.reader.read_line(&mut line)? == 0 {
                bail!("the server closed the control connection");
            }
            let line = line.trim_end().to_string();
            self.log.push(line.clone());
            if line.len() >= 4 && line[..3].chars().all(|c| c.is_ascii_digit()) {
                code = line[..3].parse().unwrap_or(0);
                text.push_str(&line[4..]);
                text.push('\n');
                if &line[3..4] == " " {
                    break;
                }
            } else {
                text.push_str(&line);
                text.push('\n');
            }
        }
        Ok((code, text.trim_end().to_string()))
    }

    /// Send a command, read its reply.
    pub fn command(&mut self, cmd: &str) -> Result<(u16, String)> {
        self.writer.write_all(format!("{cmd}\r\n").as_bytes())?;
        self.reply()
    }

    fn passive(&mut self) -> Result<TcpStream> {
        let (code, text) = self.command("PASV")?;
        if code != 227 {
            bail!("PASV: {code} {text}");
        }
        let start = text.find('(').context("PASV reply")?;
        let end = text[start..].find(')').context("PASV reply")? + start;
        let nums: Vec<u16> = text[start + 1..end]
            .split(',')
            .map(|n| n.trim().parse::<u16>())
            .collect::<Result<_, _>>()?;
        if nums.len() != 6 {
            bail!("PASV reply {text}");
        }
        let host = format!("{}.{}.{}.{}", nums[0], nums[1], nums[2], nums[3]);
        let port = nums[4] * 256 + nums[5];
        let s = TcpStream::connect_timeout(
            &(host.as_str(), port)
                .to_socket_addrs()?
                .next()
                .context("data address")?,
            Duration::from_secs(15),
        )?;
        s.set_read_timeout(Some(Duration::from_secs(120)))?;
        Ok(s)
    }

    fn transfer_in(&mut self, cmd: &str, binary: bool) -> Result<Vec<u8>> {
        self.command(if binary { "TYPE I" } else { "TYPE A" })?;
        let mut data = self.passive()?;
        let (code, text) = self.command(cmd)?;
        if code >= 400 {
            bail!("{cmd}: {code} {text}");
        }
        let mut out = Vec::new();
        data.read_to_end(&mut out)?;
        drop(data);
        let (code, text) = self.reply()?;
        if code >= 400 {
            bail!("{cmd}: {code} {text}");
        }
        Ok(out)
    }

    /// `LIST path` as text.
    pub fn list(&mut self, path: &str) -> Result<String> {
        let cmd = if path.is_empty() {
            "LIST".to_string()
        } else {
            format!("LIST {path}")
        };
        Ok(String::from_utf8_lossy(&self.transfer_in(&cmd, false)?).into_owned())
    }

    /// `RETR path`.
    pub fn get(&mut self, path: &str, binary: bool) -> Result<Vec<u8>> {
        self.transfer_in(&format!("RETR {path}"), binary)
    }

    /// `STOR path`.
    pub fn put(&mut self, path: &str, bytes: &[u8], binary: bool) -> Result<()> {
        self.command(if binary { "TYPE I" } else { "TYPE A" })?;
        let mut data = self.passive()?;
        let (code, text) = self.command(&format!("STOR {path}"))?;
        if code >= 400 {
            bail!("STOR {path}: {code} {text}");
        }
        data.write_all(bytes)?;
        drop(data);
        let (code, text) = self.reply()?;
        if code >= 400 {
            bail!("STOR {path}: {code} {text}");
        }
        Ok(())
    }

    /// Say goodbye.
    pub fn quit(&mut self) {
        let _ = self.command("QUIT");
    }
}

/// `ftp` from the command line: `op` and its operands.
pub fn run(op: &str, ops: &[String], creds: &Credentials) -> Result<()> {
    let mut f = Ftp::connect(&creds.host, &creds.user, &creds.password)?;
    let arg =
        |i: usize| -> Result<&str> { ops.get(i).map(String::as_str).context("missing operand") };
    match op {
        "ls" => print!("{}", f.list(ops.first().map_or("", String::as_str))?),
        "get" | "bget" => {
            let data = f.get(arg(0)?, op == "bget")?;
            std::fs::write(arg(1)?, data)?;
        }
        "put" | "bput" => {
            let data = std::fs::read(arg(0)?)?;
            f.put(arg(1)?, &data, op == "bput")?;
        }
        "quote" => {
            let (code, text) = f.command(&ops.join(" "))?;
            println!("{code} {text}");
        }
        other => bail!("unknown ftp operation {other}"),
    }
    f.quit();
    Ok(())
}
