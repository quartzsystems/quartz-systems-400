// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One LPD connection. "Receive a printer job" names a queue, which is
//! acknowledged (a zero octet) when the [`Spooler`] has it; then files
//! come, each line and each file acknowledged. A job is handed to the
//! spooler as soon as its control file and every data file it prints
//! have arrived, and the acknowledgement of the file that completed it
//! says whether the spooler took it. What is left at the end of the
//! connection is handed over with the data that came. The queue-state
//! commands answer the spooler's text; "print waiting jobs" answers
//! nothing (output queues print through their writers).

use std::io::{self, Read, Write};

use print::sink::lpr::control::ControlFile;

use crate::proto::{self, Command, ProtoError, Sub};

/// A job as received.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Job {
    /// The queue named on "receive a printer job".
    pub queue: String,
    /// The control file's name (`cfA123host`).
    pub control_name: String,
    /// The control file.
    pub control: ControlFile,
    /// The data files it prints, by name, in the order they came.
    pub data: Vec<(String, Vec<u8>)>,
}

/// The output queues an LPD serves.
pub trait Spooler {
    /// Whether `queue` takes jobs.
    fn accepts(&mut self, queue: &str) -> bool;
    /// Spool a job; why not.
    fn submit(&mut self, job: Job) -> Result<(), String>;
    /// The queue state, short or long, for the users or job numbers
    /// listed (all when empty): the text sent back.
    fn state(&mut self, queue: &str, long: bool, list: &[String]) -> String;
    /// Remove the jobs listed, asked by `agent`; the text sent back.
    fn remove(&mut self, queue: &str, agent: &str, list: &[String]) -> String;
}

/// How much a connection may send.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The largest file, bytes.
    pub max_file: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_file: 256 << 20 }
    }
}

/// What a connection did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// The command it began with.
    pub command: Option<Command>,
    /// Jobs spooled.
    pub spooled: usize,
    /// Jobs the spooler refused, with its reasons.
    pub refused: Vec<String>,
}

/// A session failure.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The connection failed.
    #[error("{0}")]
    Io(#[from] io::Error),
    /// A line was not RFC 1179.
    #[error("{0}")]
    Proto(#[from] ProtoError),
}

const ACK: [u8; 1] = [0];
const NAK: [u8; 1] = [1];

/// Serve one connection.
pub fn serve<S: Read + Write>(s: &mut S, spooler: &mut dyn Spooler, limits: Limits) -> Result<Outcome, SessionError> {
    let mut out = Outcome::default();
    let Some(line) = proto::read_line(s)? else {
        return Ok(out);
    };
    let cmd = Command::parse(&line)?;
    out.command = Some(cmd.clone());
    match cmd {
        Command::PrintWaiting { .. } => {}
        Command::ShortState { queue, list } => {
            s.write_all(spooler.state(&queue, false, &list).as_bytes())?;
        }
        Command::LongState { queue, list } => {
            s.write_all(spooler.state(&queue, true, &list).as_bytes())?;
        }
        Command::Remove { queue, agent, list } => {
            s.write_all(spooler.remove(&queue, &agent, &list).as_bytes())?;
        }
        Command::Receive { queue } => {
            if !spooler.accepts(&queue) {
                s.write_all(&NAK)?;
                return Ok(out);
            }
            s.write_all(&ACK)?;
            receive(s, spooler, limits, &queue, &mut out)?;
        }
    }
    s.flush()?;
    Ok(out)
}

/// The files of one "receive a printer job".
#[derive(Default)]
struct Pending {
    controls: Vec<(String, ControlFile)>,
    data: Vec<(String, Vec<u8>)>,
}

impl Pending {
    /// The data files a control file prints or unlinks.
    fn wanted(cf: &ControlFile) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for (c, v) in &cf.lines {
            if (c.is_ascii_lowercase() || *c == b'U') && !names.contains(v) {
                names.push(v.clone());
            }
        }
        names
    }

    /// The first job whose files are all here, taken out.
    fn complete(&mut self, queue: &str) -> Option<Job> {
        let at = self.controls.iter().position(|(_, cf)| {
            let wanted = Self::wanted(cf);
            !wanted.is_empty() && wanted.iter().all(|n| self.data.iter().any(|(d, _)| d == n))
        })?;
        let (name, cf) = self.controls.remove(at);
        let wanted = Self::wanted(&cf);
        Some(self.take(queue, name, cf, &wanted))
    }

    fn take(&mut self, queue: &str, name: String, cf: ControlFile, wanted: &[String]) -> Job {
        let mut data = Vec::new();
        let mut i = 0;
        while i < self.data.len() {
            if wanted.contains(&self.data[i].0) {
                data.push(self.data.remove(i));
            } else {
                i += 1;
            }
        }
        Job {
            queue: queue.to_string(),
            control_name: name,
            control: cf,
            data,
        }
    }

    /// At the end of the connection: each control file with the data
    /// that came for it, or with every unclaimed data file when none of
    /// its names came.
    fn rest(&mut self, queue: &str) -> Vec<Job> {
        let mut jobs = Vec::new();
        while let Some((name, cf)) = (!self.controls.is_empty()).then(|| self.controls.remove(0)) {
            let mut wanted = Self::wanted(&cf);
            if !wanted.iter().any(|n| self.data.iter().any(|(d, _)| d == n)) {
                wanted = self.data.iter().map(|(d, _)| d.clone()).collect();
            }
            let job = self.take(queue, name, cf, &wanted);
            if !job.data.is_empty() {
                jobs.push(job);
            }
        }
        jobs
    }
}

/// Read exactly `n` bytes.
fn exact(s: &mut impl Read, n: u64) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    s.take(n).read_to_end(&mut buf)?;
    if (buf.len() as u64) < n {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "file cut short"));
    }
    Ok(buf)
}

fn hand_over(spooler: &mut dyn Spooler, job: Job, out: &mut Outcome) -> bool {
    match spooler.submit(job) {
        Ok(()) => {
            out.spooled += 1;
            true
        }
        Err(e) => {
            tracing::info!("LPD job refused: {e}");
            out.refused.push(e);
            false
        }
    }
}

fn receive<S: Read + Write>(
    s: &mut S,
    spooler: &mut dyn Spooler,
    limits: Limits,
    queue: &str,
    out: &mut Outcome,
) -> Result<(), SessionError> {
    let mut p = Pending::default();
    while let Some(line) = proto::read_line(s)? {
        match Sub::parse(&line)? {
            Sub::Abort => {
                p = Pending::default();
                s.write_all(&ACK)?;
            }
            Sub::Control { count, name } | Sub::Data { count, name } if count > limits.max_file => {
                tracing::info!("LPD file {name} of {count} bytes is over the limit");
                s.write_all(&NAK)?;
                return Ok(());
            }
            Sub::Control { count, name } => {
                s.write_all(&ACK)?;
                let body = exact(s, count)?;
                exact(s, 1)?;
                p.controls.push((name, ControlFile::parse(&body)));
                let ok = p.complete(queue).is_none_or(|job| hand_over(spooler, job, out));
                s.write_all(if ok { &ACK } else { &NAK })?;
            }
            Sub::Data { count: 0, name } => {
                // Up to the end of the connection; no acknowledgement follows.
                s.write_all(&ACK)?;
                let mut body = Vec::new();
                s.take(limits.max_file).read_to_end(&mut body)?;
                p.data.push((name, body));
                break;
            }
            Sub::Data { count, name } => {
                s.write_all(&ACK)?;
                let body = exact(s, count)?;
                exact(s, 1)?;
                p.data.push((name, body));
                let ok = p.complete(queue).is_none_or(|job| hand_over(spooler, job, out));
                s.write_all(if ok { &ACK } else { &NAK })?;
            }
        }
        s.flush()?;
    }
    while let Some(job) = p.complete(queue) {
        hand_over(spooler, job, out);
    }
    for job in p.rest(queue) {
        hand_over(spooler, job, out);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A connection: what the client sends, what the server writes.
    struct Duplex {
        input: io::Cursor<Vec<u8>>,
        output: Vec<u8>,
    }

    impl Read for Duplex {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            self.input.read(b)
        }
    }

    impl Write for Duplex {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.output.write(b)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct Mem {
        jobs: Vec<Job>,
        refuse: bool,
    }

    impl Spooler for Mem {
        fn accepts(&mut self, queue: &str) -> bool {
            queue == "QPRINT"
        }
        fn submit(&mut self, job: Job) -> Result<(), String> {
            if self.refuse {
                return Err("full".into());
            }
            self.jobs.push(job);
            Ok(())
        }
        fn state(&mut self, queue: &str, long: bool, _: &[String]) -> String {
            format!("{queue} {} {}\n", if long { "long" } else { "short" }, self.jobs.len())
        }
        fn remove(&mut self, _: &str, agent: &str, list: &[String]) -> String {
            format!("{agent} removed {}\n", list.join(","))
        }
    }

    fn file(code: u8, name: &str, body: &[u8]) -> Vec<u8> {
        let mut v = format!("{}{} {name}\n", char::from(code), body.len()).into_bytes();
        v.extend_from_slice(body);
        v.push(0);
        v
    }

    fn run(input: Vec<u8>, sp: &mut Mem) -> (Outcome, Vec<u8>) {
        let mut d = Duplex {
            input: io::Cursor::new(input),
            output: Vec::new(),
        };
        let o = serve(&mut d, sp, Limits::default()).unwrap();
        (o, d.output)
    }

    const CF: &[u8] = b"Hhost\nPanne\nldfA001host\nUdfA001host\nNreport.txt\n";

    #[test]
    fn data_then_control() {
        let mut input = b"\x02QPRINT\n".to_vec();
        input.extend(file(3, "dfA001host", b"hello\n"));
        input.extend(file(2, "cfA001host", CF));
        let mut sp = Mem::default();
        let (o, acks) = run(input, &mut sp);
        assert_eq!(acks, [0, 0, 0, 0, 0]);
        assert_eq!(o.spooled, 1);
        let j = &sp.jobs[0];
        assert_eq!((j.queue.as_str(), j.control_name.as_str()), ("QPRINT", "cfA001host"));
        assert_eq!(j.data, vec![("dfA001host".to_string(), b"hello\n".to_vec())]);
        assert_eq!(j.control.get(b'P'), Some("anne"));
    }

    #[test]
    fn control_then_data_and_refusals() {
        let mut input = b"\x02QPRINT\n".to_vec();
        input.extend(file(2, "cfA001host", CF));
        input.extend(file(3, "dfA001host", b"x"));
        let mut sp = Mem {
            refuse: true,
            ..Mem::default()
        };
        let (o, acks) = run(input, &mut sp);
        // The data file completed the job, which the spooler refused.
        assert_eq!(acks, [0, 0, 0, 0, 1]);
        assert_eq!(o.refused, ["full"]);
        // An unknown queue.
        let (o, acks) = run(b"\x02NOSUCH\n".to_vec(), &mut Mem::default());
        assert_eq!((acks, o.spooled), (vec![1], 0));
    }

    #[test]
    fn abort_and_data_to_the_end() {
        let mut input = b"\x02QPRINT\n".to_vec();
        input.extend(file(3, "dfA001host", b"gone"));
        input.extend(b"\x01\n");
        input.extend(file(2, "cfA002host", b"Hh\nPp\nldfA002h\n"));
        input.extend(b"\x030 dfA002h\n");
        input.extend(b"to the end");
        let mut sp = Mem::default();
        let (o, _) = run(input, &mut sp);
        assert_eq!(o.spooled, 1);
        assert_eq!(sp.jobs[0].data[0].1, b"to the end");
    }

    #[test]
    fn queue_state_and_remove() {
        let mut sp = Mem::default();
        let (_, text) = run(b"\x03QPRINT\n".to_vec(), &mut sp);
        assert_eq!(text, b"QPRINT short 0\n");
        let (_, text) = run(b"\x04QPRINT anne\n".to_vec(), &mut sp);
        assert_eq!(text, b"QPRINT long 0\n");
        let (_, text) = run(b"\x05QPRINT root 12 13\n".to_vec(), &mut sp);
        assert_eq!(text, b"root removed 12,13\n");
        let (o, text) = run(b"\x01QPRINT\n".to_vec(), &mut sp);
        assert!(text.is_empty() && o.command.is_some());
    }

    #[test]
    fn oversized_files_are_refused() {
        let mut d = Duplex {
            input: io::Cursor::new(b"\x02QPRINT\n\x0399 dfA1h\n".to_vec()),
            output: Vec::new(),
        };
        let o = serve(&mut d, &mut Mem::default(), Limits { max_file: 10 }).unwrap();
        assert_eq!(d.output, [0, 1]);
        assert_eq!(o.spooled, 0);
    }
}
