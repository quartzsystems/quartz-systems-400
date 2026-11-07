// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The prototype's 5250 sessions: the four saved sessions of
//! `web/5250-config.html`, the macros of `web/5250-macros.html`, the
//! keyboard map `CWELLMAN-laptop`, and live sessions showing the screens
//! the prototype's pictures were drawn from (kept with the goldens,
//! `tools/conformance/golden/accessweb/`).

use std::collections::BTreeMap;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use qsweb::msg::{Kind, Message};

use crate::env::session5250::{Check, Input, Live, Saved, SavedMacro, Session5250, State};
use crate::s5250::files::keymap::KeyMap;
use crate::s5250::files::qkm::KeymapFile;
use crate::s5250::files::qsd::{ColorMap, ScreenSize, SessionFile};
use crate::s5250::files::qsm::MacroFile;
use crate::s5250::look::render::{Cell, Keyboard, Scheme};
use crate::s5250::screen::{Screen, View};
use crate::util::secs;

/// Work with Active Jobs, as the prototype's pictures show it.
pub const WRKACTJOB: &str =
    include_str!("../../../../tools/conformance/golden/accessweb/wrkactjob.txt");
/// The sign-on display.
pub const SIGNON: &str = include_str!("../../../../tools/conformance/golden/accessweb/signon.txt");

/// A local time of the fixture's zone (US Eastern daylight) as seconds.
fn local(mo: u32, d: u32, h: u32, mi: u32) -> u64 {
    secs(2026, mo, d, h + 4, mi, 0)
}

/// The Work with Active Jobs screen at `rows` by `cols`.
pub fn wrkactjob(rows: usize, cols: usize) -> Screen {
    Screen::from_text(WRKACTJOB, rows, cols, (24, 6))
}

/// The sign-on display.
pub fn signon() -> Screen {
    Screen::from_text(SIGNON, 24, 80, (6, 49))
}

fn view(screen: Screen, connected: bool, tls: bool, scheme: Scheme) -> View {
    View {
        screen,
        system: "QSHQ400".into(),
        connected,
        tls,
        scheme,
    }
}

/// Every picture of `shared/img`: its file, the view and the scale.
pub fn pictures() -> Vec<(&'static str, View, usize)> {
    let mut wait = signon();
    wait.keyboard = Keyboard::System;
    vec![
        (
            "5250-24x80.png",
            view(wrkactjob(24, 80), true, true, Scheme::Console),
            1,
        ),
        (
            "5250-24x80-x2.png",
            view(wrkactjob(24, 80), true, true, Scheme::Console),
            2,
        ),
        (
            "5250-24x80-white.png",
            view(wrkactjob(24, 80), true, true, Scheme::BlackOnWhite),
            1,
        ),
        (
            "5250-27x132.png",
            view(wrkactjob(27, 132), true, true, Scheme::Console),
            1,
        ),
        (
            "5250-signon.png",
            view(signon(), true, true, Scheme::Console),
            1,
        ),
        ("5250-wait.png", view(wait, true, true, Scheme::Console), 1),
        (
            "5250-24x80-off.png",
            view(signon(), false, true, Scheme::Console),
            1,
        ),
    ]
}

/// One saved session of the prototype.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
fn saved(
    id: u32,
    title: &str,
    system: &str,
    wide: bool,
    device: &str,
    tls: bool,
    bw: bool,
    keypad: bool,
    auto: (bool, bool),
    last_used: u64,
) -> Saved {
    let mut f = SessionFile::new(system);
    f.system.host = if system == "QSHQ400" {
        "10.0.10.20".into()
    } else {
        "10.0.30.20".into()
    };
    f.system.tls = tls;
    f.system.telnet_port = if tls { 992 } else { 23 };
    f.session.title = title.into();
    f.session.screen_size = if wide {
        ScreenSize::S27x132
    } else {
        ScreenSize::S24x80
    };
    f.session.device_name = device.into();
    f.session.colour_map = if bw {
        ColorMap::BlackOnWhite
    } else {
        ColorMap::Console
    };
    f.session.keypad_visible = keypad;
    f.session.auto_connect = auto.0;
    f.session.auto_sign_on = auto.1;
    Saved {
        id,
        file: f,
        last_used,
        macros: Vec::new(),
    }
}

fn macro_file(name: &str, lines: &str) -> MacroFile {
    let mut m = MacroFile::new(name, ScreenSize::S24x80);
    m.steps = MacroFile::from_lines(lines, ScreenSize::S24x80).unwrap_or_default();
    m
}

/// The prototype's macros of `QSHQ400 operator`.
pub fn macros() -> Vec<SavedMacro> {
    vec![
        SavedMacro {
            file: macro_file(
                "SIGNON-DEV",
                "wait  screen \"Sign On\" at 1,35\ntype  \"CWELLMAN\"\nkey   Tab\nkey   Enter\nwait  screen \"Main Menu\" at 1,36\nkey   F:4   ; pick a screen position with the mouse in the editor\n",
            ),
            recorded: local(10, 20, 9, 12),
            shared: String::new(),
        },
        SavedMacro {
            file: macro_file(
                "WRKACTJOB-HTTP",
                "wait  screen \"===>\" at 22,7\ntype  \"WRKACTJOB SBS(QHTTPSVR)\"\nkey   Enter\n",
            ),
            recorded: local(10, 22, 14, 30),
            shared: "WAREHOUSE".into(),
        },
        SavedMacro {
            file: macro_file(
                "DSPMSG-QSYSOPR",
                "wait  screen \"===>\" at 22,7\ntype  \"DSPMSG QSYSOPR\"\n",
            ),
            recorded: local(10, 22, 14, 33),
            shared: String::new(),
        },
    ]
}

/// The prototype's saved sessions.
pub fn sessions() -> Vec<Saved> {
    let mut v = vec![
        saved(
            1,
            "QSHQ400 operator",
            "QSHQ400",
            false,
            "QPADEV0003",
            true,
            false,
            true,
            (true, true),
            local(10, 29, 13, 10),
        ),
        saved(
            2,
            "QSHQ400 default",
            "QSHQ400",
            false,
            "",
            true,
            true,
            false,
            (false, false),
            local(10, 22, 9, 41),
        ),
        saved(
            3,
            "QSHQ400 wide",
            "QSHQ400",
            true,
            "QPADEV0004",
            true,
            false,
            false,
            (true, true),
            local(10, 14, 18, 40),
        ),
        saved(
            4,
            "QSBR1400 branch",
            "QSBR1400",
            false,
            "",
            false,
            false,
            true,
            (false, false),
            local(10, 15, 16, 2),
        ),
    ];
    v[0].macros = macros();
    v
}

/// The live sessions as the prototype shows them: `QSHQ400 operator`
/// on Work with Active Jobs, `QSHQ400 default` closed by the system on
/// its sign-on display, `QSHQ400 wide` on Work with Active Jobs.
fn lives() -> BTreeMap<(String, u32), Live> {
    let mut m = BTreeMap::new();
    let user = super::USER.to_string();
    let live = |view: View, state: State, message: Option<Message>| Live {
        view,
        state,
        message,
        version: 1,
    };
    m.insert(
        (user.clone(), 1),
        live(
            view(wrkactjob(24, 80), true, true, Scheme::Console),
            State::Connected,
            None,
        ),
    );
    let closed = Message::new(
        Kind::Error,
        "QAD1005",
        "Session to QSHQ400 ended by the system.",
    )
    .with_data(&["QSHQ400"]);
    m.insert(
        (user.clone(), 2),
        live(
            view(signon(), false, true, Scheme::Console),
            State::Disconnected,
            Some(closed),
        ),
    );
    m.insert(
        (user, 3),
        live(
            view(wrkactjob(27, 132), true, true, Scheme::Console),
            State::Connected,
            None,
        ),
    );
    m
}

/// The sample 5250 sessions.
pub struct Fixture5250 {
    /// Saved sessions by user.
    pub saved: Mutex<BTreeMap<String, Vec<Saved>>>,
    /// Keyboard maps by user.
    pub keymaps: Mutex<BTreeMap<String, Vec<(String, KeyMap)>>>,
    /// Live sessions by user and number.
    pub live: Mutex<BTreeMap<(String, u32), Live>>,
    /// Every input sent, in order: user, session, input.
    pub inputs: Mutex<Vec<(String, u32, Input)>>,
    changed: Condvar,
}

impl Default for Fixture5250 {
    fn default() -> Self {
        let laptop = KeymapFile::parse(include_str!("../s5250/files/testdata/operations.qkm"))
            .map(|f| f.map)
            .unwrap_or_default();
        Self {
            saved: Mutex::new(BTreeMap::from([(super::USER.to_string(), sessions())])),
            keymaps: Mutex::new(BTreeMap::from([(
                super::USER.to_string(),
                vec![("CWELLMAN-laptop".to_string(), laptop)],
            )])),
            live: Mutex::new(lives()),
            inputs: Mutex::new(Vec::new()),
            changed: Condvar::new(),
        }
    }
}

impl Fixture5250 {
    /// Change live session `id` of `CWELLMAN` (tests set the other
    /// states the prototype shows).
    pub fn set_live(&self, id: u32, f: impl FnOnce(&mut Live)) {
        let mut m = self.live.lock().expect("live");
        if let Some(l) = m.get_mut(&(super::USER.to_string(), id)) {
            f(l);
            l.version += 1;
        }
        self.changed.notify_all();
    }
}

impl Session5250 for Fixture5250 {
    fn sessions(&self, user: &str) -> Vec<Saved> {
        self.saved
            .lock()
            .expect("saved")
            .get(user)
            .cloned()
            .unwrap_or_default()
    }

    fn put(&self, user: &str, mut saved: Saved) -> Result<u32, Message> {
        let mut m = self.saved.lock().expect("saved");
        let v = m.entry(user.to_string()).or_default();
        if saved.id == 0 {
            saved.id = v.iter().map(|s| s.id).max().unwrap_or(0) + 1;
        }
        let id = saved.id;
        match v.iter_mut().find(|s| s.id == id) {
            Some(s) => *s = saved,
            None => v.push(saved),
        }
        Ok(id)
    }

    fn delete(&self, user: &str, id: u32) -> Result<(), Message> {
        let mut m = self.saved.lock().expect("saved");
        if let Some(v) = m.get_mut(user) {
            v.retain(|s| s.id != id);
        }
        self.live
            .lock()
            .expect("live")
            .remove(&(user.to_string(), id));
        Ok(())
    }

    fn keymaps(&self, user: &str) -> Vec<(String, KeyMap)> {
        self.keymaps
            .lock()
            .expect("keymaps")
            .get(user)
            .cloned()
            .unwrap_or_default()
    }

    fn put_keymap(&self, user: &str, name: &str, map: &KeyMap) -> Result<(), Message> {
        let mut m = self.keymaps.lock().expect("keymaps");
        let v = m.entry(user.to_string()).or_default();
        v.retain(|(n, _)| n != name);
        v.push((name.to_string(), map.clone()));
        Ok(())
    }

    fn verify(&self, user: &str, system: &str, sign_on: bool) -> Vec<Check> {
        let c = |name: &str, ok: Option<bool>, message: &str| Check {
            name: name.into(),
            ok,
            message: message.into(),
        };
        let host = format!("{}.quartz.example", system.to_ascii_lowercase());
        let addr = if system == "QSHQ400" {
            "10.0.10.20"
        } else {
            "10.0.30.20"
        };
        vec![
            c("Name lookup", Some(true), &format!("{host} = {addr}")),
            c("Telnet port 23", Some(true), ""),
            c(
                "Telnet TLS port 992: handshake and certificate",
                Some(true),
                "*.quartz.example, Quartz Internal CA, expires 01/02/28",
            ),
            c("QAP port 2320", Some(true), ""),
            c(
                "TLS 1.3 handshake and certificate (QS400_ACCESS_SERVER)",
                Some(true),
                "",
            ),
            c("QAP Hello", Some(true), "QAP version 6"),
            if sign_on {
                c(
                    "Test sign-on",
                    Some(true),
                    &format!("Signed on as {user} and off again"),
                )
            } else {
                c("Test sign-on", None, "Not requested")
            },
        ]
    }

    fn connect(&self, user: &str, saved: &Saved) -> Result<(), Message> {
        let (rows, cols) = saved.file.session.screen_size.rows_cols();
        let mut screen = if rows == 24 {
            signon()
        } else {
            Screen::blank(rows, cols)
        };
        if saved.file.session.auto_sign_on && saved.file.system.tls {
            screen = wrkactjob(rows, cols);
        }
        let scheme = match saved.file.session.colour_map {
            ColorMap::Console => Scheme::Console,
            ColorMap::BlackOnWhite => Scheme::BlackOnWhite,
        };
        let mut m = self.live.lock().expect("live");
        let version = m
            .get(&(user.to_string(), saved.id))
            .map_or(0, |l| l.version)
            + 1;
        m.insert(
            (user.to_string(), saved.id),
            Live {
                view: View {
                    screen,
                    system: saved.file.system.name.clone(),
                    connected: true,
                    tls: saved.file.system.tls,
                    scheme,
                },
                state: State::Connected,
                message: None,
                version,
            },
        );
        self.changed.notify_all();
        Ok(())
    }

    fn live(&self, user: &str, id: u32) -> Option<Live> {
        self.live
            .lock()
            .expect("live")
            .get(&(user.to_string(), id))
            .cloned()
    }

    fn wait(&self, user: &str, id: u32, version: u64, timeout: Duration) -> Option<Live> {
        let key = (user.to_string(), id);
        let m = self.live.lock().expect("live");
        let (m, _) = self
            .changed
            .wait_timeout_while(m, timeout, |m| {
                m.get(&key).is_some_and(|l| l.version <= version)
            })
            .expect("live");
        m.get(&key).cloned()
    }

    #[allow(clippy::many_single_char_names)]
    fn input(&self, user: &str, id: u32, input: Input) -> Result<(), Message> {
        self.inputs
            .lock()
            .expect("inputs")
            .push((user.to_string(), id, input.clone()));
        let mut m = self.live.lock().expect("live");
        let Some(l) = m.get_mut(&(user.to_string(), id)) else {
            return Err(
                Message::new(Kind::Error, "QAD1006", "Session to QSHQ400 disconnected.")
                    .with_data(&["QSHQ400"]),
            );
        };
        if l.state == State::Disconnected {
            return Err(Message::new(
                Kind::Error,
                "QAD1006",
                &format!("Session to {} disconnected.", l.view.system),
            )
            .with_data(&[&l.view.system]));
        }
        let s = &mut l.view.screen;
        match input {
            Input::Text(t) => {
                for ch in t.chars() {
                    let (r, c) = s.cursor;
                    let i = (r - 1) * s.cols + c - 1;
                    if let Some(cell) = s.cells.get_mut(i) {
                        *cell = Cell { ch, ..*cell };
                    }
                    s.cursor = if c < s.cols {
                        (r, c + 1)
                    } else {
                        (r % s.rows + 1, 1)
                    };
                }
            }
            Input::Cursor(r, c) if r >= 1 && c >= 1 && r <= s.rows && c <= s.cols => {
                s.cursor = (r, c)
            }
            Input::Cursor(..) | Input::Key(_) => {}
        }
        l.version += 1;
        self.changed.notify_all();
        Ok(())
    }

    fn disconnect(&self, user: &str, id: u32) {
        let mut m = self.live.lock().expect("live");
        if let Some(l) = m.get_mut(&(user.to_string(), id)) {
            l.state = State::Disconnected;
            l.view.connected = false;
            l.message = Some(
                Message::new(
                    Kind::Info,
                    "QAD1006",
                    &format!("Session to {} disconnected.", l.view.system),
                )
                .with_data(&[&l.view.system]),
            );
            l.version += 1;
        }
        self.changed.notify_all();
    }
}
