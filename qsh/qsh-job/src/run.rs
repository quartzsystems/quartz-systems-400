// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shell on a worker thread while the job's thread serves it.
//!
//! - [`with_shell`]: any shell work, to its end.
//! - [`batch`]: `QSH CMD()`, its output collected for the caller to
//!   route (`STDOUT` is the caller's; the files are the shell's).
//! - [`invoke`]: `qsh` with arguments over given standard streams (a
//!   child job running the shell, an SSH session).
//! - [`Session`]: the line-fed terminal session behind `STRQSH`, kept
//!   between displays: it survives F12 and resumes at the next `STRQSH`.

use std::io;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use qsh::session::batch::{BatchResult, run_cmd};
use qsh::session::{self, Feed, Interactive, Invocation};
use qsh::sys::{Buffer, Fd, Stream};

use crate::broker::{self, Desk};
use crate::side::Side;
use crate::sys::JobSys;
use crate::sys::Setup;

/// How often the job's thread looks at the worker while serving.
const TICK: Duration = Duration::from_millis(20);
/// The worker threads' stack.
const STACK: usize = 16 * 1024 * 1024;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Run `f` with a `JobSys` on a worker thread, serving its tasks on
/// this thread until it returns.
///
/// # Panics
///
/// The worker thread cannot be started, or `f` panicked.
pub fn with_shell<T: Send + 'static>(
    side: &mut dyn Side,
    setup: Setup,
    f: impl FnOnce(JobSys) -> T + Send + 'static,
) -> T {
    let (b, desk) = broker::pair();
    let sys = JobSys::new(b, setup);
    let h = std::thread::Builder::new()
        .name("qsh".into())
        .stack_size(STACK)
        .spawn(move || f(sys))
        .expect("a thread for the shell");
    desk.serve_until(side, TICK, &mut || h.is_finished());
    desk.serve(side, Duration::ZERO);
    h.join().expect("the shell's thread")
}

/// What `QSH CMD()` did.
#[derive(Clone, Debug)]
pub struct Batch {
    /// The status and the message to send.
    pub result: BatchResult,
    /// What went to `STDOUT` (empty for `NONE` and the files).
    pub output: Vec<u8>,
}

/// `QSH CMD(cmd)`: output routing and the ending message by the
/// environment's `QIBM_QSH_CMD_OUTPUT` and `QIBM_QSH_CMD_ESCAPE_MSG`.
pub fn batch(side: &mut dyn Side, setup: Setup, cmd: &str) -> Batch {
    let cmd = cmd.to_string();
    with_shell(side, setup, move |sys| {
        let out = Buffer::new();
        let result = run_cmd(
            Arc::new(sys),
            &cmd,
            Some(qsh::sys::Null::fd()),
            Some(out.fd()),
        );
        Batch {
            result,
            output: out.take(),
        }
    })
}

/// `qsh` with arguments (`argv[0]` first) over the given standard
/// streams; the exit status. A bad argument is status 2.
pub fn invoke(
    side: &mut dyn Side,
    setup: Setup,
    argv: Vec<String>,
    stdio: [Option<Fd>; 3],
) -> i32 {
    with_shell(side, setup, move |sys| {
        let [i, o, e] = stdio;
        match Invocation::parse(&argv) {
            Ok(inv) => session::run(Arc::new(sys), &inv, i, o, e),
            Err(m) => {
                if let Some(e) = &e {
                    let _ = qsh::sys::stream::write_all(
                        &**e,
                        format!("qsh: 001-0062 {m}\n").as_bytes(),
                    );
                }
                2
            }
        }
    })
}

/// Where a session stands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    /// A command is running.
    pub running: bool,
    /// The running command is reading its standard input.
    pub wants_input: bool,
    /// The shell ended with this status.
    pub exited: Option<i32>,
}

/// What the terminal shows: the shell's output, text only.
#[derive(Default)]
struct Shared {
    out: Mutex<Vec<u8>>,
    state: Mutex<State>,
    changed: Condvar,
}

/// The session's terminal output.
struct TermOut(Arc<Shared>);

impl Stream for TermOut {
    fn read(&self, _buf: &mut [u8]) -> io::Result<usize> {
        Ok(0)
    }
    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        lock(&self.0.out).extend_from_slice(buf);
        self.0.changed.notify_all();
        Ok(buf.len())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// The session's terminal input: lines typed while a command runs.
struct TermIn {
    shared: Arc<Shared>,
    lines: Arc<Mutex<Receiver<Option<String>>>>,
    held: Mutex<Vec<u8>>,
}

impl Stream for TermIn {
    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let mut held = lock(&self.held);
        if held.is_empty() {
            lock(&self.shared.state).wants_input = true;
            self.shared.changed.notify_all();
            let next = lock(&self.lines).recv().ok().flatten();
            lock(&self.shared.state).wants_input = false;
            match next {
                Some(l) => {
                    held.extend_from_slice(l.as_bytes());
                    held.push(b'\n');
                }
                None => return Ok(0),
            }
        }
        let n = held.len().min(buf.len());
        buf[..n].copy_from_slice(&held[..n]);
        held.drain(..n);
        Ok(n)
    }
    fn write(&self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::PermissionDenied.into())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// The terminal session: an interactive login shell on a worker thread,
/// fed a line at a time.
pub struct Session {
    shared: Arc<Shared>,
    lines: Sender<Option<String>>,
    desk: Desk,
    worker: Option<JoinHandle<()>>,
}

impl Session {
    /// Start a session: `/etc/profile`, `$HOME/.profile`, then the first
    /// prompt in the output.
    ///
    /// # Panics
    ///
    /// The worker thread cannot be started.
    #[must_use]
    pub fn start(setup: Setup) -> Self {
        let (b, desk) = broker::pair();
        let sys = JobSys::new(b, setup);
        let shared = Arc::new(Shared::default());
        let (tx, rx) = mpsc::channel::<Option<String>>();
        let rx = Arc::new(Mutex::new(rx));
        let sh2 = shared.clone();
        let worker = std::thread::Builder::new()
            .name("qsh-session".into())
            .stack_size(STACK)
            .spawn(move || session_loop(sys, &sh2, &rx))
            .expect("a thread for the session");
        lock(&shared.state).running = true;
        Self {
            shared,
            lines: tx,
            desk,
            worker: Some(worker),
        }
    }

    /// Serve the shell's tasks for up to `wait`, returning early when
    /// output arrives or the state changes.
    pub fn serve(&self, side: &mut dyn Side, wait: Duration) {
        let until = std::time::Instant::now() + wait;
        let before = (lock(&self.shared.out).len(), self.state());
        loop {
            self.desk.serve(side, TICK);
            let now = (lock(&self.shared.out).len(), self.state());
            if now != before || std::time::Instant::now() >= until {
                return;
            }
        }
    }

    /// Type a line: a command when the shell waits for one, else input
    /// for the running command.
    pub fn send(&self, line: &str) {
        let st = self.state();
        if !st.running {
            let mut out = lock(&self.shared.out);
            out.extend_from_slice(b"> ");
            out.extend_from_slice(line.as_bytes());
            out.push(b'\n');
            drop(out);
            lock(&self.shared.state).running = true;
        } else if st.wants_input {
            let mut out = lock(&self.shared.out);
            out.extend_from_slice(line.as_bytes());
            out.push(b'\n');
        }
        let _ = self.lines.send(Some(line.to_string()));
    }

    /// End of input for the running command (or the shell).
    pub fn eof(&self) {
        let _ = self.lines.send(None);
    }

    /// Whether output is waiting.
    pub fn has_output(&self) -> bool {
        !lock(&self.shared.out).is_empty()
    }

    /// The output since the last call.
    pub fn take_output(&self) -> Vec<u8> {
        std::mem::take(&mut *lock(&self.shared.out))
    }

    /// Where the session stands.
    pub fn state(&self) -> State {
        lock(&self.shared.state).clone()
    }

    /// Whether the shell has ended.
    pub fn ended(&self) -> bool {
        self.state().exited.is_some()
    }

    /// End the session: the shell sees end of input and its thread is
    /// joined once it stops (serving `side` meanwhile).
    pub fn end(mut self, side: &mut dyn Side) {
        self.eof();
        if let Some(w) = self.worker.take() {
            let mut waited = 0;
            while !w.is_finished() && waited < 250 {
                self.desk.serve(side, TICK);
                waited += 1;
            }
            if w.is_finished() {
                let _ = w.join();
            }
        }
    }
}

fn session_loop(sys: JobSys, shared: &Arc<Shared>, rx: &Arc<Mutex<Receiver<Option<String>>>>) {
    let out: Fd = Arc::new(TermOut(shared.clone()));
    let input: Fd = Arc::new(TermIn {
        shared: shared.clone(),
        lines: rx.clone(),
        held: Mutex::new(Vec::new()),
    });
    let inv = Invocation::session();
    let sh = session::shell_for(
        Arc::new(sys),
        &inv,
        Some(input),
        Some(out.clone()),
        Some(out.clone()),
    );
    let mut term = Interactive::new(sh);
    let say = |text: &str| {
        let _ = out.write(text.as_bytes());
    };
    loop {
        let notes = term.notices();
        if !notes.is_empty() {
            say(&notes);
        }
        let prompt = term.prompt();
        say(&format!("{}\n", prompt.trim_end()));
        lock(&shared.state).running = false;
        shared.changed.notify_all();
        let next = match lock(rx).recv_timeout(Duration::from_secs(3600)) {
            Ok(l) => l,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => None,
        };
        let fed = match next {
            Some(line) => term.feed_line(&line),
            None => term.feed_eof(),
        };
        if let Feed::Exit(n) = fed {
            let mut st = lock(&shared.state);
            st.exited = Some(n);
            st.running = false;
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ifs::client::{IfsClient, Unavailable};

    struct S(Unavailable);

    impl Side for S {
        fn ifs(&self) -> &dyn IfsClient {
            &self.0
        }
    }

    fn side() -> S {
        S(Unavailable(ifs::Errno::ENOENT))
    }

    #[test]
    fn batch_collects_stdout_and_the_ending_message() {
        let b = batch(&mut side(), Setup::default(), "echo hi; exit 3");
        assert_eq!(b.output, b"hi\n");
        assert_eq!(b.result.status, 3);
        assert_eq!(b.result.message.id, "QSH0005");
    }

    #[test]
    fn a_session_echoes_runs_and_ends() {
        let mut s = side();
        let sess = Session::start(Setup::default());
        while sess.state().running {
            sess.serve(&mut s, Duration::from_millis(50));
        }
        assert_eq!(String::from_utf8(sess.take_output()).unwrap(), "$\n");
        sess.send("echo one");
        loop {
            sess.serve(&mut s, Duration::from_millis(50));
            if !sess.state().running {
                break;
            }
        }
        assert_eq!(
            String::from_utf8(sess.take_output()).unwrap(),
            "> echo one\none\n$\n"
        );
        sess.send("read x; echo got $x");
        while !sess.state().wants_input {
            sess.serve(&mut s, Duration::from_millis(50));
        }
        sess.send("abc");
        while sess.state().running {
            sess.serve(&mut s, Duration::from_millis(50));
        }
        let t = String::from_utf8(sess.take_output()).unwrap();
        assert!(t.ends_with("abc\ngot abc\n$\n"), "{t}");
        sess.send("exit 4");
        while !sess.ended() {
            sess.serve(&mut s, Duration::from_millis(50));
        }
        assert_eq!(sess.state().exited, Some(4));
        sess.end(&mut s);
    }
}
