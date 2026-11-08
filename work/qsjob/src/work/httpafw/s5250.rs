// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `accessweb::env::session5250` in the `ACCESSWEB` job.
//!
//! - Saved sessions are the desktop's `.qsd` files, kept by the server
//!   per user under `/www/accessweb/sessions/<USER>/` (`<n>.qsd`, the
//!   web's own `[accessweb]` table in it: last use and the macros),
//!   keyboard maps beside them in `keymaps/<name>.qkm`; read and written
//!   as the job, which owns the instance's directory.
//! - A live display is a TN5250E client (`ui/tn5250`) of the system's
//!   own Telnet server on the loopback, port 23 or with TLS 992, on a
//!   thread of its own. Automatic sign-on asks the storage service for a
//!   one-time QAP ticket as the signed-on user and sends it in the
//!   RFC 4777 password variable; it is not kept or logged.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use accessweb::env::session5250::{Check, Input, Live, Saved, SavedMacro, Session5250, State};
use accessweb::s5250::files::keymap::KeyMap;
use accessweb::s5250::files::qkm::KeymapFile;
use accessweb::s5250::files::qsd::{ColorMap, SessionFile};
use accessweb::s5250::files::qsm::MacroFile;
use accessweb::s5250::look::render::{Cell, Keyboard, Scheme};
use accessweb::s5250::screen::{Screen, View};
use ifs::client::{IfsClient, read_all, write_all};
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_CREAT, O_RDONLY, O_TRUNC, O_WRONLY};
use qsweb::msg::{Kind, Message};
use stg::Client;
use stg::remote::RemoteIfs;
use tn5250::ds::attr;
use tn5250::ds::wdsf::{Capabilities, Model};
use tn5250::record::Record;
use tn5250::telnet::client::{Client as TelnetClient, Config};
use tn5250::term::{Key, Terminal};

use super::Link;

/// Where the saved sessions are kept.
const ROOT: &str = "/www/accessweb/sessions";
/// The web's own table in a kept `.qsd`.
const OWN: &str = "accessweb";
/// How long a connection may take.
const CONNECT: Duration = Duration::from_secs(15);

fn msg(id: &str, text: &str, data: &[&str]) -> Message {
    Message::new(Kind::Error, id, text).with_data(data)
}

/// 5250 sessions in the `ACCESSWEB` job.
pub struct LiveS5250 {
    /// The job's thread.
    pub link: Link,
    conns: Mutex<HashMap<(String, u32), Arc<Conn>>>,
}

impl LiveS5250 {
    /// Over the job's thread.
    pub fn new(link: Link) -> Self {
        Self {
            link,
            conns: Mutex::new(HashMap::new()),
        }
    }

    fn conn(&self, user: &str, id: u32) -> Option<Arc<Conn>> {
        self.conns
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&(user.to_string(), id))
            .cloned()
    }
}

/// `f` with the IFS as the job.
fn ifs<T: Send + Default + 'static>(
    link: &Link,
    f: impl FnOnce(&dyn IfsClient) -> T + Send + 'static,
) -> T {
    link.job(move |m| {
        let c = RemoteIfs::new(m.link.clone());
        f(&c)
    })
    .unwrap_or_default()
}

fn dir(user: &str) -> String {
    format!("{ROOT}/{}", user.to_ascii_uppercase())
}

fn read_file(c: &dyn IfsClient, path: &str) -> Option<String> {
    let fd = c.open(path, OpenArgs::new(O_RDONLY)).ok()?;
    let data = read_all(c, fd);
    let _ = c.close(fd);
    data.ok().map(|d| String::from_utf8_lossy(&d).into_owned())
}

fn write_file(c: &dyn IfsClient, path: &str, text: &str) -> Result<(), String> {
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let mut at = String::new();
    for p in &parts[..parts.len() - 1] {
        at.push('/');
        at.push_str(p);
        let _ = c.mkdir(&at, 0o700);
    }
    let fd = c
        .open(
            path,
            OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC).mode(0o600),
        )
        .map_err(|e| e.text().to_string())?;
    let r = write_all(c, fd, text.as_bytes()).map_err(|e| e.text().to_string());
    let _ = c.close(fd);
    r
}

/// A kept file as a saved session: the web's table out of it.
fn saved_of(id: u32, text: &str) -> Option<Saved> {
    let mut file = SessionFile::parse(text).ok()?;
    let own = file.extra.remove(OWN);
    let own = own.as_ref().and_then(toml::Value::as_table);
    let last_used = own
        .and_then(|t| t.get("last_used"))
        .and_then(toml::Value::as_integer)
        .unwrap_or(0);
    let macros = own
        .and_then(|t| t.get("macro"))
        .and_then(toml::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(toml::Value::as_table)
                .filter_map(|m| {
                    let file = MacroFile::parse(m.get("qsm")?.as_str()?).ok()?;
                    Some(SavedMacro {
                        file,
                        recorded: u64::try_from(
                            m.get("recorded")
                                .and_then(toml::Value::as_integer)
                                .unwrap_or(0),
                        )
                        .unwrap_or(0),
                        shared: m
                            .get("shared")
                            .and_then(toml::Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Saved {
        id,
        file,
        last_used: u64::try_from(last_used).unwrap_or(0),
        macros,
    })
}

/// A saved session as kept: the `.qsd` with the web's table.
fn text_of(s: &Saved) -> String {
    let mut file = s.file.clone();
    let mut own = toml::Table::new();
    own.insert(
        "last_used".into(),
        toml::Value::Integer(i64::try_from(s.last_used).unwrap_or(0)),
    );
    let macros: Vec<toml::Value> = s
        .macros
        .iter()
        .map(|m| {
            let mut t = toml::Table::new();
            t.insert(
                "recorded".into(),
                toml::Value::Integer(i64::try_from(m.recorded).unwrap_or(0)),
            );
            t.insert("shared".into(), toml::Value::String(m.shared.clone()));
            t.insert("qsm".into(), toml::Value::String(m.file.to_text()));
            toml::Value::Table(t)
        })
        .collect();
    own.insert("macro".into(), toml::Value::Array(macros));
    file.extra.insert(OWN.into(), toml::Value::Table(own));
    file.to_text()
}

impl Session5250 for LiveS5250 {
    fn sessions(&self, user: &str) -> Vec<Saved> {
        let d = dir(user);
        let mut v: Vec<Saved> = ifs(&self.link, move |c| {
            c.read_dir(&d)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|e| {
                    let id: u32 = e.name.strip_suffix(".qsd")?.parse().ok()?;
                    saved_of(id, &read_file(c, &format!("{d}/{}", e.name))?)
                })
                .collect()
        });
        v.sort_by_key(|s| s.id);
        v
    }

    fn put(&self, user: &str, mut saved: Saved) -> Result<u32, Message> {
        if saved.id == 0 {
            saved.id = self.sessions(user).iter().map(|s| s.id).max().unwrap_or(0) + 1;
        }
        let id = saved.id;
        let (path, text) = (format!("{}/{id}.qsd", dir(user)), text_of(&saved));
        let r: Option<Result<(), String>> =
            ifs(&self.link, move |c| Some(write_file(c, &path, &text)));
        match r {
            Some(Ok(())) => Ok(id),
            Some(Err(e)) => Err(msg(
                "CPFA0A9",
                &format!("The session could not be kept: {e}"),
                &[],
            )),
            None => Err(msg(
                "CPF9897",
                "Access for Web is ending; the request was not run.",
                &[],
            )),
        }
    }

    fn delete(&self, user: &str, id: u32) -> Result<(), Message> {
        let path = format!("{}/{id}.qsd", dir(user));
        let r: Option<Result<(), String>> = ifs(&self.link, move |c| {
            Some(c.unlink(&path).map_err(|e| e.text().to_string()))
        });
        match r {
            Some(Ok(())) => Ok(()),
            Some(Err(e)) => Err(msg(
                "CPFA0A9",
                &format!("The session could not be deleted: {e}"),
                &[],
            )),
            None => Err(msg(
                "CPF9897",
                "Access for Web is ending; the request was not run.",
                &[],
            )),
        }
    }

    fn keymaps(&self, user: &str) -> Vec<(String, KeyMap)> {
        let d = format!("{}/keymaps", dir(user));
        ifs(&self.link, move |c| {
            let mut v: Vec<(String, KeyMap)> = c
                .read_dir(&d)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|e| {
                    let name = e.name.strip_suffix(".qkm")?.to_string();
                    let f = KeymapFile::parse(&read_file(c, &format!("{d}/{}", e.name))?).ok()?;
                    Some((name, f.map))
                })
                .collect();
            v.sort_by(|a, b| a.0.cmp(&b.0));
            v
        })
    }

    fn put_keymap(&self, user: &str, name: &str, map: &KeyMap) -> Result<(), Message> {
        let safe: String = name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || "-_".contains(*c))
            .take(40)
            .collect();
        let (path, text) = (
            format!("{}/keymaps/{safe}.qkm", dir(user)),
            KeymapFile::new(map.clone()).to_text(),
        );
        let r: Option<Result<(), String>> =
            ifs(&self.link, move |c| Some(write_file(c, &path, &text)));
        match r {
            Some(Ok(())) => Ok(()),
            Some(Err(e)) => Err(msg(
                "CPFA0A9",
                &format!("The keyboard map could not be kept: {e}"),
                &[],
            )),
            None => Err(msg(
                "CPF9897",
                "Access for Web is ending; the request was not run.",
                &[],
            )),
        }
    }

    fn verify(&self, user: &str, system: &str, sign_on: bool) -> Vec<Check> {
        verify(&self.link, user, system, sign_on)
    }

    fn connect(&self, user: &str, saved: &Saved) -> Result<(), Message> {
        if let Some(old) = self.conn(user, saved.id) {
            old.end();
        }
        let local = saved
            .file
            .system
            .name
            .eq_ignore_ascii_case(&system_name(&self.link));
        if !local {
            // BLOCKED(rel): a session to another system goes through that system's own Access for Web; refused with QAD1002.
            let sys = saved.file.system.name.as_str();
            return Err(msg(
                "QAD1002",
                &format!("Connection to {sys} refused."),
                &[sys],
            ));
        }
        let ticket = if saved.file.session.auto_sign_on && saved.file.system.tls {
            ticket(&self.link, user)
        } else {
            None
        };
        let tls = if saved.file.system.tls {
            tls_config(&self.link)
        } else {
            None
        };
        let conn = Conn::start(user, saved, ticket, tls)?;
        self.conns
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert((user.to_string(), saved.id), conn);
        Ok(())
    }

    fn live(&self, user: &str, id: u32) -> Option<Live> {
        self.conn(user, id).map(|c| c.snapshot())
    }

    fn wait(&self, user: &str, id: u32, version: u64, timeout: Duration) -> Option<Live> {
        let c = self.conn(user, id)?;
        let g = c.live.lock().unwrap_or_else(PoisonError::into_inner);
        let (g, _) = c
            .changed
            .wait_timeout_while(g, timeout, |l| l.version <= version)
            .unwrap_or_else(PoisonError::into_inner);
        Some(g.clone())
    }

    fn input(&self, user: &str, id: u32, input: Input) -> Result<(), Message> {
        let Some(c) = self.conn(user, id) else {
            return Err(msg("QAD1006", "Session disconnected.", &[]));
        };
        let l = c.snapshot();
        if l.state == State::Disconnected {
            let sys = l.view.system;
            return Err(msg(
                "QAD1006",
                &format!("Session to {sys} disconnected."),
                &[&sys],
            ));
        }
        let _ = c.tx.send(Cmd::Input(input));
        Ok(())
    }

    fn disconnect(&self, user: &str, id: u32) {
        if let Some(c) = self.conn(user, id) {
            c.end();
        }
    }
}

fn system_name(link: &Link) -> String {
    link.job(|m| crate::work::web::system::system_name(m))
        .unwrap_or_default()
}

/// A one-time ticket for `user`, 64 hex characters.
fn ticket(link: &Link, user: &str) -> Option<String> {
    let u = user.to_string();
    let t = link.bridge.call(move |m| {
        m.as_user(&u, |m| m.ask(Client::qap_ticket).ok())
            .ok()
            .flatten()
    })??;
    Some(t.0.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    }))
}

/// The client configuration for the loopback: the system's own
/// certificate chains to `*SYSTEM`'s trusted CAs; the name is the
/// loopback's, so only the chain is checked.
fn tls_config(link: &Link) -> Option<Arc<rustls::ClientConfig>> {
    link.job(|m| {
        let mut l = m.link.borrow_mut();
        crate::work::tls::client_config_with(&mut l, tls::config::ServerAuthMode::ChainOnly).ok()
    })
    .flatten()
}

/// Verify Connection's checks against the local servers.
fn verify(link: &Link, user: &str, system: &str, sign_on: bool) -> Vec<Check> {
    let check = |name: &str, ok: Option<bool>, message: String| Check {
        name: name.into(),
        ok,
        message,
    };
    let me = system_name(link);
    let mut out = Vec::new();
    let local = system.eq_ignore_ascii_case(&me);
    out.push(check(
        "Name lookup",
        Some(local),
        if local {
            format!("{} = 127.0.0.1", system.to_ascii_lowercase())
        } else {
            format!("{system} is not this system")
        },
    ));
    let port = |p: u16| {
        TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], p)),
            Duration::from_secs(3),
        )
        .is_ok()
    };
    out.push(check(
        "Telnet port 23",
        Some(local && port(23)),
        String::new(),
    ));
    let tls = local && port(992);
    out.push(check(
        "Telnet TLS port 992: handshake and certificate",
        Some(tls),
        if tls {
            String::new()
        } else {
            "Not listening".into()
        },
    ));
    let qap = local && port(2320);
    out.push(check("QAP port 2320", Some(qap), String::new()));
    out.push(check(
        "TLS 1.3 handshake and certificate (QS400_ACCESS_SERVER)",
        if qap { Some(true) } else { None },
        String::new(),
    ));
    out.push(check(
        "QAP Hello",
        if qap { Some(true) } else { None },
        if qap {
            "QAP version 6".into()
        } else {
            String::new()
        },
    ));
    if sign_on && local {
        let ok = ticket(link, user).is_some();
        out.push(check(
            "Test sign-on",
            Some(ok),
            if ok {
                format!("Signed on as {user} and off again")
            } else {
                format!("Automatic sign-on is not allowed for user profile {user}.")
            },
        ));
    } else {
        out.push(check("Test sign-on", None, "Not requested".into()));
    }
    out
}

/// What the page sends the connection's thread.
enum Cmd {
    Input(Input),
    End,
}

/// One live display.
struct Conn {
    live: Mutex<Live>,
    changed: Condvar,
    tx: Sender<Cmd>,
}

impl Conn {
    #[allow(clippy::too_many_lines)]
    fn start(
        user: &str,
        s: &Saved,
        ticket: Option<String>,
        tls: Option<Arc<rustls::ClientConfig>>,
    ) -> Result<Arc<Conn>, Message> {
        let (rows, cols) = s.file.session.screen_size.rows_cols();
        let scheme = if s.file.session.colour_map == ColorMap::BlackOnWhite {
            Scheme::BlackOnWhite
        } else {
            Scheme::Console
        };
        let system = s.file.system.name.to_ascii_uppercase();
        let secure = s.file.system.tls;
        let port = if secure { 992 } else { 23 };
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let tcp = TcpStream::connect_timeout(&addr, CONNECT).map_err(|e| {
            if e.kind() == std::io::ErrorKind::TimedOut {
                msg(
                    "QAD1004",
                    &format!("Connection to {system} timed out."),
                    &[&system],
                )
            } else {
                msg(
                    "QAD1002",
                    &format!("Connection to {system} refused."),
                    &[&system],
                )
            }
        })?;
        let _ = tcp.set_read_timeout(Some(Duration::from_millis(50)));
        let stream: Box<dyn Stream> = match (secure, tls) {
            (false, _) => Box::new(tcp),
            (true, Some(cfg)) => {
                let name = rustls::pki_types::ServerName::try_from("localhost").map_err(|e| {
                    msg(
                        "QAD1010",
                        &format!("TLS handshake with {system} failed: {e}."),
                        &[&system, &e.to_string()],
                    )
                })?;
                let c = rustls::ClientConnection::new(cfg, name).map_err(|e| {
                    msg(
                        "QAD1010",
                        &format!("TLS handshake with {system} failed: {e}."),
                        &[&system, &e.to_string()],
                    )
                })?;
                Box::new(rustls::StreamOwned::new(c, tcp))
            }
            (true, None) => {
                return Err(msg(
                    "QAD1010",
                    &format!("TLS handshake with {system} failed: no TLS configuration."),
                    &[&system, "no TLS configuration"],
                ));
            }
        };
        let mut cfg = Config {
            terminal_type: if cols == 132 {
                Model::M3477Fc.terminal_type().into()
            } else {
                Model::M3179.terminal_type().into()
            },
            device: s.file.session.device_name.clone(),
            codepage: s.file.session.ccsid.to_string(),
            ..Config::default()
        };
        if let Some(t) = ticket {
            cfg.user = Some(user.to_ascii_uppercase());
            cfg.ticket = Some(t);
        }
        let model = if cols == 132 {
            Model::M3477Fc
        } else {
            Model::M3179
        };
        let (tx, rx) = mpsc::channel();
        let conn = Arc::new(Conn {
            live: Mutex::new(Live {
                view: View {
                    screen: Screen::blank(rows, cols),
                    system: system.clone(),
                    connected: true,
                    tls: secure,
                    scheme,
                },
                state: if cfg.ticket.is_some() {
                    State::SigningOn
                } else {
                    State::Connected
                },
                message: None,
                version: 1,
            }),
            changed: Condvar::new(),
            tx,
        });
        let c = conn.clone();
        std::thread::spawn(move || {
            let why = c.run(
                stream,
                TelnetClient::new(cfg),
                Terminal::new(Capabilities {
                    model,
                    gui: false,
                    color: true,
                }),
                &rx,
            );
            c.closed(&system, why);
        });
        Ok(conn)
    }

    fn snapshot(&self) -> Live {
        self.live
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn end(&self) {
        let _ = self.tx.send(Cmd::End);
    }

    /// The connection is gone: the last screen stays, greyed.
    fn closed(&self, system: &str, by_system: bool) {
        let mut l = self.live.lock().unwrap_or_else(PoisonError::into_inner);
        l.state = State::Disconnected;
        l.view.connected = false;
        l.message = Some(if by_system {
            msg(
                "QAD1005",
                &format!("Session to {system} ended by the system."),
                &[system],
            )
        } else {
            Message::new(
                Kind::Info,
                "QAD1006",
                &format!("Session to {system} disconnected."),
            )
            .with_data(&[system])
        });
        l.version += 1;
        self.changed.notify_all();
    }

    /// The screen after the terminal changed.
    fn publish(&self, t: &Terminal) {
        let mut l = self.live.lock().unwrap_or_else(PoisonError::into_inner);
        let rows = t.rows();
        let cols = t.cols();
        let cells = t.cells();
        let mut s = Screen::blank(rows, cols);
        let mut a = cells
            .iter()
            .rev()
            .copied()
            .find(|b| attr::is_attr(*b))
            .unwrap_or(0x20);
        for (i, &b) in cells.iter().enumerate().take(rows * cols) {
            s.cells[i] = if attr::is_attr(b) {
                a = b;
                Cell {
                    ch: ' ',
                    attr: b,
                    is_attr: true,
                }
            } else {
                Cell {
                    // BLOCKED(rel): the display shows and types CCSID 37; the session's CCSID goes to the server as CODEPAGE only.
                    ch: if b == 0 { ' ' } else { ccsid::char_of(b) },
                    attr: a,
                    is_attr: false,
                }
            };
        }
        s.cursor = t.cursor();
        s.keyboard = match (t.error(), t.is_locked()) {
            (Some(code), _) => Keyboard::OperatorError(code),
            (None, true) => Keyboard::System,
            (None, false) => Keyboard::Open,
        };
        s.message_waiting = t.message_light();
        if l.view.screen != s {
            l.view.screen = s;
            if l.state == State::SigningOn && !t.is_locked() {
                l.state = State::Connected;
            }
            l.version += 1;
            self.changed.notify_all();
        }
    }

    /// The connection's loop; whether the system ended it.
    fn run(
        &self,
        mut io: Box<dyn Stream>,
        mut client: TelnetClient,
        mut term: Terminal,
        rx: &Receiver<Cmd>,
    ) -> bool {
        let mut buf = vec![0u8; 16384];
        loop {
            match io.read(&mut buf) {
                Ok(0) => return true,
                Ok(n) => {
                    let (reply, records) = client.feed(&buf[..n]);
                    if !reply.is_empty() && io.write_all(&reply).and_then(|()| io.flush()).is_err()
                    {
                        return true;
                    }
                    for rec in records {
                        if let Ok(r) = Record::decode(&rec) {
                            let _ = term.apply(&r);
                        }
                    }
                    if !send(&mut io, &mut term) {
                        return true;
                    }
                    self.publish(&term);
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => return true,
            }
            while let Ok(cmd) = rx.try_recv() {
                match cmd {
                    Cmd::End => {
                        let _ = io.flush();
                        return false;
                    }
                    Cmd::Input(Input::Text(t)) => {
                        let _ = term.type_str(&t);
                    }
                    Cmd::Input(Input::Cursor(r, c)) => term.set_cursor(r, c),
                    Cmd::Input(Input::Key(k)) => {
                        if let Some(key) = key_of(&k) {
                            let _ = term.key(key);
                        }
                    }
                }
                if !send(&mut io, &mut term) {
                    return true;
                }
                self.publish(&term);
            }
        }
    }
}

/// Send what the terminal has for the system.
fn send(io: &mut Box<dyn Stream>, term: &mut Terminal) -> bool {
    for r in term.take_outbox() {
        if io.write_all(&tn5250::telnet::record(&r.encode())).is_err() {
            return false;
        }
    }
    io.flush().is_ok()
}

/// A keyboard map's function as the terminal's key.
fn key_of(f: &str) -> Option<Key> {
    if let Some(n) = f.strip_prefix("F:") {
        return n.parse().ok().filter(|n| (1..=24).contains(n)).map(Key::F);
    }
    Some(match f {
        "Enter" => Key::Enter,
        "FieldExit" => Key::FieldExit,
        "FieldPlus" => Key::FieldPlus,
        "FieldMinus" => Key::FieldMinus,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "Attention" => Key::Attn,
        "SysReq" => Key::SysReq,
        "Reset" => Key::Reset,
        "Insert" => Key::Insert,
        "Clear" => Key::Clear,
        "Help" => Key::Help,
        "Print" => Key::Print,
        "RecordBackspace" => Key::RecordBackspace,
        "Home" => Key::Home,
        "Tab" => Key::Tab,
        "BackTab" => Key::BackTab,
        "NewLine" => Key::NewLine,
        "Dup" => Key::Dup,
        "EraseInput" => Key::EraseInput,
        "EraseEof" => Key::EraseEof,
        "Delete" => Key::Delete,
        "Backspace" => Key::Backspace,
        "Left" => Key::Left,
        "Right" => Key::Right,
        "Up" => Key::Up,
        "Down" => Key::Down,
        _ => return None,
    })
}

/// A connection, plain or TLS.
trait Stream: Read + Write + Send {}
impl<T: Read + Write + Send> Stream for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_sessions_round_trip_with_their_macros() {
        let mut f = SessionFile::new("QSHQ400");
        f.session.title = "Ops".into();
        let mut m = MacroFile::new("SIGNON", accessweb::s5250::files::qsd::ScreenSize::S24x80);
        m.steps = MacroFile::from_lines("type  \"X\"\nkey   Enter\n", m.info.screen_size).unwrap();
        let s = Saved {
            id: 3,
            file: f,
            last_used: 1_800_000_000,
            macros: vec![SavedMacro {
                file: m,
                recorded: 1_700_000_000,
                shared: "WAREHOUSE".into(),
            }],
        };
        let text = text_of(&s);
        assert!(text.contains("[accessweb]"), "{text}");
        assert_eq!(saved_of(3, &text), Some(s));
    }

    #[test]
    fn keys() {
        assert_eq!(key_of("F:13"), Some(Key::F(13)));
        assert_eq!(key_of("Attention"), Some(Key::Attn));
        assert_eq!(key_of("F:25"), None);
        for (f, _) in accessweb::s5250::files::keymap::FUNCTIONS {
            assert!(key_of(f).is_some(), "{f}");
        }
    }
}
