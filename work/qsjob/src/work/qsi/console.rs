// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system's console for the console family at ID 0: `slic`, which
//! owns the console, serves one viewer at `wm::dev::QSI_CONSOLE`
//! (`console::qsi::tee`) beside the local display; this is the job's
//! side of it, in the family's terms.

use std::io;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use console::qsi::viewer::{self, Error, Seen};
use qsi::host::console::{Capture, In, Out, msgid};
use qsipart::console::{FromSystem, Grid, ToSystem};

/// Why the console could not be had.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    /// The refusal's identifier.
    pub msgid: &'static str,
    /// What it says.
    pub text: String,
}

impl Refused {
    fn of(e: &Error) -> Self {
        match e {
            Error::Viewed => Self {
                msgid: msgid::VIEWED,
                text: "Another console is viewing the system's console.".to_string(),
            },
            Error::Io(e) => Self {
                msgid: qsi::host::host::msgid::SERVICE_DOWN,
                text: format!("The system's console did not answer: {e}."),
            },
        }
    }
}

/// The console, attached.
pub struct Viewer {
    inner: viewer::Viewer<UnixStream>,
}

/// Attach to the console, taking it over from another viewer when
/// `take_over`.
pub fn attach(take_over: bool) -> Result<Viewer, Refused> {
    attach_at(Path::new(wm::dev::QSI_CONSOLE), take_over)
}

/// The console's screen as it is now.
pub fn screen() -> Result<Capture, Refused> {
    screen_at(Path::new(wm::dev::QSI_CONSOLE))
}

fn connect(path: &Path) -> Result<UnixStream, Refused> {
    UnixStream::connect(path).map_err(|e| Refused::of(&Error::Io(e)))
}

fn attach_at(path: &Path, take_over: bool) -> Result<Viewer, Refused> {
    let inner = viewer::attach(connect(path)?, take_over).map_err(|e| Refused::of(&e))?;
    Ok(Viewer { inner })
}

fn screen_at(path: &Path) -> Result<Capture, Refused> {
    let frame = viewer::screen(connect(path)?).map_err(|e| Refused::of(&Error::Io(e)))?;
    let mut grid = Grid::default();
    grid.apply(&FromSystem::Frame(frame.clone()))
        .map_err(|e| Refused::of(&Error::Io(io::Error::other(e.to_string()))))?;
    Ok(Capture {
        serial: false,
        frame: Some(frame),
        text: grid.text(),
    })
}

impl Viewer {
    /// The next frame, update or end within `timeout`; `None` when
    /// nothing came.
    pub fn recv(&mut self, timeout: Duration) -> io::Result<Option<Out>> {
        Ok(self.inner.recv(timeout)?.map(|s| match s {
            Seen::Frame(f) => Out::Frame(f),
            Seen::Update(u) => Out::Update(u),
            Seen::Ended(why) => Out::Ended(why),
        }))
    }

    /// A key or a refresh to the console; serial bytes and kinds of a
    /// later version are passed over (the console is not a serial port).
    #[allow(clippy::needless_pass_by_value)] // the family's API takes it whole
    pub fn send(&mut self, input: In) -> io::Result<()> {
        match input {
            In::Key(k) => self.inner.send(&ToSystem::Key(k)),
            In::Refresh => self.inner.send(&ToSystem::Refresh),
            In::Bytes(_) | In::Other(_) => Ok(()),
        }
    }
}

impl Drop for Viewer {
    fn drop(&mut self) {
        self.inner.close();
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixListener;
    use std::sync::Arc;
    use std::sync::mpsc;

    use console::qsi::tee::{Ask, Hub, Poke};
    use console::{Attr, Display, Key, Screen, View};
    use qsipart::console::Key as WireKey;

    use super::*;

    /// A console at a socket of its own: a screen drawn to the hub's
    /// display, the keys it was given.
    fn console_at(dir: &Path) -> (std::path::PathBuf, mpsc::Receiver<Key>) {
        let path = dir.join("qsicons.sock");
        let _ = std::fs::remove_file(&path);
        let hub = Hub::new();
        let mut tee = hub.display("test");
        let (tx, rx) = mpsc::channel::<Ask>();
        let (keys_tx, keys) = mpsc::channel();
        std::thread::spawn(move || {
            let mut s = Screen::new();
            s.put(1, 36, "Sign On", Attr::WHITE);
            tee.draw_view(&s, &View::default()).unwrap();
            while let Ok(a) = rx.recv() {
                if let Ask::Key(k) = a {
                    let _ = keys_tx.send(k);
                }
                tee.draw_view(&s, &View::default()).unwrap();
            }
        });
        let listener = UnixListener::bind(&path).unwrap();
        let tx = std::sync::Mutex::new(tx);
        let poke: Poke = Arc::new(move |a| {
            let _ = tx.lock().unwrap().send(a);
        });
        std::thread::spawn(move || {
            for c in listener.incoming() {
                let (hub, poke) = (Arc::clone(&hub), Arc::clone(&poke));
                std::thread::spawn(move || hub.serve(c.unwrap(), &poke));
            }
        });
        (path, keys)
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("qsicons-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_family_sees_frames_and_types() {
        let dir = scratch("family");
        let (path, keys) = console_at(&dir);
        let mut v = attach_at(&path, false).unwrap();
        let first = loop {
            if let Some(o) = v.recv(Duration::from_secs(5)).unwrap() {
                break o;
            }
        };
        let Out::Frame(f) = first else {
            panic!("{first:?}")
        };
        let mut g = Grid::default();
        g.apply(&FromSystem::Frame(f)).unwrap();
        assert!(g.text().starts_with(&format!("{}Sign On", " ".repeat(35))));
        v.send(In::Key(WireKey::Enter)).unwrap();
        v.send(In::Bytes(vec![1])).unwrap();
        assert_eq!(
            keys.recv_timeout(Duration::from_secs(5)).unwrap(),
            Key::Enter
        );
        // A second without take over is refused by name; with it, the
        // first is ended.
        let refused = attach_at(&path, false).err().unwrap();
        assert_eq!(refused.msgid, "QSI0601");
        let _second = attach_at(&path, true).unwrap();
        let ended = loop {
            match v.recv(Duration::from_secs(5)).unwrap() {
                Some(Out::Ended(why)) => break why,
                Some(_) => {}
                None => panic!("not ended"),
            }
        };
        assert!(!ended.is_empty());
        // The screen alone.
        let c = screen_at(&path).unwrap();
        assert!(!c.serial);
        assert!(c.frame.is_some());
        assert!(c.text.contains("Sign On"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_console_is_refused_as_down() {
        let dir = scratch("none");
        let r = attach_at(&dir.join("absent.sock"), false).err().unwrap();
        assert_eq!(r.msgid, "QSI0301");
        let r = screen_at(&dir.join("absent.sock")).err().unwrap();
        assert_eq!(r.msgid, "QSI0301");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
