// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The legacy scp protocol (the `rcp` dialect: `scp -t` receives, `scp
//! -f` sends) over an [`IfsClient`], for clients run with `scp -O` and
//! older ones. Current OpenSSH `scp` uses the SFTP subsystem instead.

use std::io::{Read, Write};

use ifs::client::{CHUNK, IfsClient};
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_CREAT, O_RDONLY, O_TRUNC, O_WRONLY, S_IFDIR};

use crate::session::ChannelIo;
use crate::sftp::names::absolute;

/// An `scp` command line the server answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// `-t`: receive into `paths[0]`; otherwise `-f`: send `paths`.
    pub to: bool,
    /// `-r`.
    pub recursive: bool,
    /// `-p`: times kept.
    pub preserve: bool,
    /// `-d`: the target must be a directory.
    pub dir_target: bool,
    /// The paths.
    pub paths: Vec<String>,
}

/// Split a command line into words: quotes and backslashes as `sh`.
fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut any = false;
    let mut chars = s.chars();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"') | None, '\\') => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
                any = true;
            }
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                any = true;
            }
            (None, c) if c.is_whitespace() => {
                if any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            (None, c) => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(cur);
    }
    out
}

impl Args {
    /// Parse an `exec` command; `None` when it is not `scp -t`/`-f`.
    #[must_use]
    pub fn parse(cmd: &str) -> Option<Self> {
        let w = words(cmd);
        let (first, rest) = w.split_first()?;
        if first != "scp" && !first.ends_with("/scp") {
            return None;
        }
        let mut a = Self {
            to: false,
            recursive: false,
            preserve: false,
            dir_target: false,
            paths: Vec::new(),
        };
        let mut mode = None;
        let mut opts = true;
        for word in rest {
            if opts && word == "--" {
                opts = false;
            } else if opts && word.starts_with('-') && word.len() > 1 {
                for f in word[1..].chars() {
                    match f {
                        't' => mode = Some(true),
                        'f' => mode = Some(false),
                        'r' => a.recursive = true,
                        'p' => a.preserve = true,
                        'd' => a.dir_target = true,
                        'v' | 'q' | 'E' => {}
                        _ => return None,
                    }
                }
            } else {
                a.paths.push(word.clone());
            }
        }
        a.to = mode?;
        if a.paths.is_empty() || (a.to && a.paths.len() != 1) {
            return None;
        }
        Some(a)
    }
}

struct Session<'a> {
    ifs: &'a dyn IfsClient,
    input: &'a mut dyn Read,
    output: &'a mut dyn Write,
    errors: bool,
}

impl Session<'_> {
    fn send(&mut self, b: &[u8]) -> bool {
        self.output.write_all(b).and_then(|()| self.output.flush()).is_ok()
    }

    fn ack(&mut self) -> bool {
        self.send(&[0])
    }

    fn error(&mut self, text: &str) -> bool {
        self.errors = true;
        self.send(format!("\x01scp: {text}\n").as_bytes())
    }

    fn line(&mut self) -> Option<String> {
        let mut out = Vec::new();
        let mut b = [0u8; 1];
        loop {
            match self.input.read(&mut b) {
                Ok(1) if b[0] == b'\n' => return Some(String::from_utf8_lossy(&out).into_owned()),
                Ok(1) => {
                    out.push(b[0]);
                    if out.len() > 4096 {
                        return None;
                    }
                }
                _ => return if out.is_empty() { None } else { Some(String::from_utf8_lossy(&out).into_owned()) },
            }
        }
    }

    /// Wait for the peer's answer: true for OK.
    fn wait_ack(&mut self) -> bool {
        let mut b = [0u8; 1];
        if self.input.read_exact(&mut b).is_err() {
            return false;
        }
        if b[0] == 0 {
            return true;
        }
        let _ = self.line();
        self.errors = true;
        false
    }

    fn is_dir(&self, path: &str) -> bool {
        self.ifs
            .stat(path, true)
            .is_ok_and(|st| st.mode & 0o170_000 == S_IFDIR)
    }

    fn name(p: &str) -> &str {
        p.trim_end_matches('/').rsplit('/').next().unwrap_or(p)
    }

    /// Receive (`scp -t`).
    fn sink(&mut self, target: &str, a: &Args) {
        if a.dir_target && !self.is_dir(target) {
            self.error(&format!("{target}: Not a directory"));
            return;
        }
        let mut stack = vec![target.to_string()];
        let target_is_dir = self.is_dir(target);
        let mut times: Option<(i64, i64)> = None;
        if !self.ack() {
            return;
        }
        while let Some(line) = self.line() {
            let Some(kind) = line.chars().next() else {
                break;
            };
            let rest = &line[1..];
            match kind {
                'T' => {
                    let f: Vec<i64> = rest.split(' ').filter_map(|x| x.parse().ok()).collect();
                    if f.len() == 4 {
                        times = Some((f[2], f[0]));
                    }
                    self.ack();
                }
                'C' | 'D' => {
                    let mut it = rest.splitn(3, ' ');
                    let mode = it.next().and_then(|m| u32::from_str_radix(m, 8).ok());
                    let size = it.next().and_then(|s| s.parse::<u64>().ok());
                    let name = it.next().unwrap_or("");
                    let (Some(mode), Some(size)) = (mode, size) else {
                        self.error("protocol error: bad header");
                        return;
                    };
                    if name.is_empty() || name.contains('/') || name == ".." {
                        self.error(&format!("{name}: name not valid"));
                        return;
                    }
                    let top = stack.last().cloned().unwrap_or_default();
                    let path = if stack.len() > 1 || target_is_dir {
                        format!("{}/{name}", top.trim_end_matches('/'))
                    } else {
                        top
                    };
                    if kind == 'D' {
                        if !a.recursive {
                            self.error("received a directory without -r");
                            return;
                        }
                        if !self.is_dir(&path)
                            && let Err(e) = self.ifs.mkdir(&path, mode & 0o7777)
                        {
                            self.error(&format!("{path}: {}", e.text()));
                            return;
                        }
                        stack.push(path);
                        self.ack();
                    } else {
                        self.receive_file(&path, mode & 0o7777, size, times.take());
                    }
                }
                'E' => {
                    if stack.len() > 1 {
                        stack.pop();
                    }
                    self.ack();
                }
                '\x01' => self.errors = true,
                _ => return,
            }
        }
    }

    fn receive_file(&mut self, path: &str, mode: u32, size: u64, times: Option<(i64, i64)>) {
        let fd = self
            .ifs
            .open(path, OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC).mode(mode));
        if !self.ack() {
            return;
        }
        let mut left = size;
        let mut failure = fd.as_ref().err().map(|e| e.text().to_string());
        let mut buf = vec![0u8; 64 * 1024];
        while left > 0 {
            let n = usize::try_from(left.min(buf.len() as u64)).unwrap_or(buf.len());
            if self.input.read_exact(&mut buf[..n]).is_err() {
                return;
            }
            if let (Ok(fd), None) = (&fd, &failure) {
                let mut done = 0;
                while done < n {
                    match self.ifs.write(*fd, &buf[done..n]) {
                        Ok(0) => {
                            failure = Some(ifs::Errno::EIO.text().to_string());
                            break;
                        }
                        Ok(k) => done += k,
                        Err(e) => {
                            failure = Some(e.text().to_string());
                            break;
                        }
                    }
                }
            }
            left -= n as u64;
        }
        if let Ok(fd) = fd {
            let _ = self.ifs.close(fd);
        }
        let mut end = [0u8; 1];
        let _ = self.input.read_exact(&mut end);
        if let (Some((at, mt)), None) = (times, &failure) {
            let _ = self
                .ifs
                .utime(path, Some((at * 1_000_000_000, mt * 1_000_000_000)));
        }
        match failure {
            Some(text) => {
                self.error(&format!("{path}: {text}"));
            }
            None => {
                self.ack();
            }
        }
    }

    /// Send (`scp -f`).
    fn source(&mut self, paths: &[String], a: &Args) {
        if !self.wait_ack() {
            return;
        }
        for p in paths {
            if !self.send_path(p, a) {
                return;
            }
        }
    }

    fn send_path(&mut self, path: &str, a: &Args) -> bool {
        let st = match self.ifs.stat(path, true) {
            Ok(st) => st,
            Err(e) => return self.error(&format!("{path}: {}", e.text())),
        };
        let secs = |ns: i64| ns.div_euclid(1_000_000_000);
        if a.preserve {
            let t = format!("T{} 0 {} 0\n", secs(st.mtime), secs(st.atime));
            if !self.send(t.as_bytes()) || !self.wait_ack() {
                return false;
            }
        }
        let mode = st.mode & 0o7777;
        if st.mode & 0o170_000 == S_IFDIR {
            if !a.recursive {
                return self.error(&format!("{path}: not a regular file"));
            }
            let head = format!("D{mode:04o} 0 {}\n", Self::name(path));
            if !self.send(head.as_bytes()) || !self.wait_ack() {
                return false;
            }
            let mut names: Vec<String> = match self.ifs.read_dir(path) {
                Ok(v) => v.into_iter().map(|e| e.name).collect(),
                Err(e) => return self.error(&format!("{path}: {}", e.text())),
            };
            names.sort();
            for n in names {
                if !self.send_path(&format!("{}/{n}", path.trim_end_matches('/')), a) {
                    return false;
                }
            }
            return self.send(b"E\n") && self.wait_ack();
        }
        let fd = match self.ifs.open(path, OpenArgs::new(O_RDONLY)) {
            Ok(fd) => fd,
            Err(e) => return self.error(&format!("{path}: {}", e.text())),
        };
        let head = format!("C{mode:04o} {} {}\n", st.size, Self::name(path));
        if !self.send(head.as_bytes()) || !self.wait_ack() {
            let _ = self.ifs.close(fd);
            return false;
        }
        let mut left = st.size;
        let mut ok = true;
        while left > 0 {
            let want = usize::try_from(left.min(CHUNK as u64)).unwrap_or(CHUNK);
            match self.ifs.read(fd, want) {
                Ok(d) if !d.is_empty() => {
                    if !self.send(&d) {
                        let _ = self.ifs.close(fd);
                        return false;
                    }
                    left -= d.len() as u64;
                }
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        let _ = self.ifs.close(fd);
        if !ok {
            // The size was promised; pad, then report.
            let pad = vec![0u8; usize::try_from(left).unwrap_or(0)];
            self.send(&pad);
            return self.error(&format!("{path}: read error")) && self.wait_ack();
        }
        self.ack() && self.wait_ack()
    }
}

/// Run an scp command over the session's streams; the exit status.
pub fn run(ifs: &dyn IfsClient, home: &str, a: &Args, io: &mut ChannelIo) -> u32 {
    let mut s = Session {
        ifs,
        input: &mut io.stdin,
        output: &mut io.stdout,
        errors: false,
    };
    let paths: Vec<String> = a.paths.iter().map(|p| absolute(home, p)).collect();
    if a.to {
        s.sink(&paths[0], a);
    } else {
        s.source(&paths, a);
    }
    u32::from(s.errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lines() {
        let a = Args::parse("scp -t -- '/home/my file'").unwrap();
        assert!(a.to);
        assert_eq!(a.paths, ["/home/my file"]);
        let b = Args::parse("scp -r -p -f a b").unwrap();
        assert!(!b.to && b.recursive && b.preserve);
        assert_eq!(b.paths, ["a", "b"]);
        assert!(Args::parse("ls -l").is_none());
        assert!(Args::parse("scp a b").is_none());
        assert!(Args::parse("scp -t a b").is_none());
    }
}
