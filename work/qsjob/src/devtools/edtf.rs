// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `EDTF` and `DSPF` at the job's display: the stream file (or a
//! database member, by its `/QSYS.LIB` path) read through the storage
//! service's IFS, decoded from its CCSID, the session of `devtools/edtf`
//! drawn on the display, each key read back into it, the file written
//! back on a save with the line end it had and the CCSID it is tagged
//! with (the services display may change both). A member is read and
//! written in text mode, so its records stay records. `DSPF` in a job
//! without a display gives way to the service's text form.

use cmd::{Resolved, Value};
use edtf::msg::Msg;
use edtf::session::{Action, Key, Mode, Session};
use edtf::text::Text;
use ifs::Errno;
use ifs::client::{IfsClient, read_all, write_all};
use ifs::ops::flags::{O_CCSID, O_CREAT, O_RDONLY, O_TEXTDATA, O_TRUNC, O_WRONLY};
use ifs::ops::{Attr, OpenArgs};
use mi::Job;
use stg::remote::RemoteIfs;
use tn5250::ds::aid;
use tn5250::screen::{Read, ScreenModel};

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// The CCSID a member is read and written in (text mode converts).
const MEMBER_CCSID: u16 = 1208;

/// What is being edited.
struct Target {
    /// The path the title shows and the IFS opens.
    path: String,
    /// A database member (text mode).
    member: bool,
}

/// A parameter's text as typed or defaulted.
fn parm(r: &Resolved, kwd: &str) -> String {
    r.get(kwd)
        .and_then(|b| b.value.as_ref())
        .map(|v| match v {
            Value::Char(s) => s.clone(),
            other => other.source(),
        })
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn specified(r: &Resolved, kwd: &str) -> bool {
    r.get(kwd).is_some_and(|b| b.specified)
}

/// A message in a failure, values in `data`.
fn fail(m: &Msg, data: Vec<u8>) -> Fail {
    Fail {
        msgid: m.id.to_string(),
        data,
        text: m.text.clone(),
    }
}

/// An `errno` as the command's escape: IBM's `CPFA0xx`, the path its
/// first value when the message names the object.
fn errno_fail(e: Errno, path: &str) -> Fail {
    let mut data = Vec::new();
    if e.command_message_names_object() {
        let b = ccsid::conv::from_str(37, path).unwrap_or_default();
        data.extend_from_slice(&u32::try_from(b.len()).unwrap_or(0).to_be_bytes());
        data.extend_from_slice(&b);
    }
    Fail {
        msgid: e.command_message().to_string(),
        data,
        text: format!("{path}: {}", e.text()),
    }
}

/// The job's CCSID for text (its default CCSID when it is 65535).
fn job_ccsid(host: &Host) -> u16 {
    let a = host.attrs();
    match u16::try_from(a.ccsid) {
        Ok(65535) | Err(_) => u16::try_from(a.dftccsid).unwrap_or(37),
        Ok(c) => c,
    }
}

/// `STMF`, or `FILE` and `MBR` as a `/QSYS.LIB` path.
#[allow(clippy::many_single_char_names)] // c, r: the client and the command
fn target(host: &Host, c: &dyn IfsClient, r: &Resolved) -> Result<Target, Fail> {
    let stmf = parm(r, "STMF");
    let file = r.get("FILE").and_then(|b| b.value.clone());
    match (stmf.is_empty(), file) {
        (false, Some(_)) => Err(fail(&Msg::new("CPDB627", &[]), Vec::new())),
        (true, None) => Err(fail(&Msg::new("CPDB628", &[]), Vec::new())),
        (false, None) => {
            if specified(r, "MBR") {
                return Err(fail(&Msg::new("CPDB629", &[]), Vec::new()));
            }
            let member = {
                let up = stmf.to_ascii_uppercase();
                up.contains("/QSYS.LIB/") && std::path::Path::new(&up).extension().is_some_and(|e| e.eq_ignore_ascii_case("mbr"))
            };
            Ok(Target { path: stmf, member })
        }
        (true, Some(f)) => {
            let (name, lib) = match &f {
                Value::Qual(parts) => (
                    parts.first().map(Value::source).unwrap_or_default(),
                    parts.get(1).map(Value::source).unwrap_or_default(),
                ),
                other => (other.source(), String::new()),
            };
            let name = name.trim().to_ascii_uppercase();
            let lib = lib.trim().to_ascii_uppercase();
            let libs: Vec<String> = match lib.as_str() {
                "" | "*LIBL" => {
                    let l = host.attrs().libl;
                    let mut v = l.sys.clone();
                    v.extend(l.prd.iter().filter(|n| !n.is_empty()).cloned());
                    v.extend(l.cur.iter().cloned());
                    v.extend(l.usr.iter().cloned());
                    v
                }
                "*CURLIB" => host.attrs().libl.cur.into_iter().collect(),
                other => vec![other.to_string()],
            };
            let file_path = libs
                .iter()
                .map(|l| format!("/QSYS.LIB/{l}.LIB/{name}.FILE"))
                .find(|p| c.stat(p, true).is_ok())
                .ok_or_else(|| errno_fail(Errno::ENOENT, &format!("{lib}/{name}")))?;
            let mbr = parm(r, "MBR").to_ascii_uppercase();
            let mbr = if mbr.is_empty() || mbr == "*FIRST" {
                let mut names: Vec<String> = c
                    .read_dir(&file_path)
                    .map_err(|e| errno_fail(e, &file_path))?
                    .into_iter()
                    .map(|e| e.name)
                    .filter(|n| n.to_ascii_uppercase().ends_with(".MBR"))
                    .collect();
                names.sort();
                names
                    .into_iter()
                    .next()
                    .map(|n| n[..n.len() - 4].to_string())
                    .ok_or_else(|| errno_fail(Errno::ENOENT, &file_path))?
            } else {
                mbr
            };
            Ok(Target {
                path: format!("{file_path}/{mbr}.MBR"),
                member: true,
            })
        }
    }
}

/// The key an attention identifier stands for.
fn key_of(a: u8) -> Option<Key> {
    match a {
        aid::ROLL_UP => Some(Key::PageDown),
        aid::ROLL_DOWN => Some(Key::PageUp),
        other => match ScreenModel::key_of(other) {
            Some(0) => Some(Key::Enter),
            Some(n) => Some(Key::F(n)),
            None => None,
        },
    }
}

/// `EDTF` or `DSPF`. `None` for `DSPF` in a job without a display: the
/// service's text form answers it.
pub fn run(_job: &mut Job, host: &Host, r: &Resolved, _out: &mut Outcome) -> Option<Result<String, Fail>> {
    let mode = if r.name == "EDTF" { Mode::Edit } else { Mode::Browse };
    if !crate::panels::shows_panels(host, r) {
        return match mode {
            Mode::Browse => None,
            Mode::Edit => Some(Err(fail(&Msg::new("CPFB621", &[]), Vec::new()))),
        };
    }
    let c = RemoteIfs::new(host.link.clone());
    Some(session(host, &c, r, mode))
}

/// Open the file and run the session.
fn session(host: &Host, c: &dyn IfsClient, r: &Resolved, mode: Mode) -> Result<String, Fail> {
    let t = target(host, c, r)?;
    let job = job_ccsid(host);
    let (text, ccsid, size, exists) = match c.stat(&t.path, true) {
        Ok(st) => {
            if st.objtype == "*DIR" || st.objtype == "*LIB" || st.objtype == "*FILE" {
                return Err(errno_fail(Errno::EISDIR, &t.path));
            }
            let (args, ccsid) = if t.member {
                (
                    OpenArgs::new(O_RDONLY | O_TEXTDATA | O_CCSID).conv(u32::from(MEMBER_CCSID)),
                    MEMBER_CCSID,
                )
            } else {
                // A binary file shows as the job's characters.
                let c = if st.ccsid == 65535 { job } else { st.ccsid };
                (OpenArgs::new(O_RDONLY), c)
            };
            let fd = c.open(&t.path, args).map_err(|e| errno_fail(e, &t.path))?;
            let data = read_all(c, fd);
            let _ = c.close(fd);
            let data = data.map_err(|e| errno_fail(e, &t.path))?;
            let size = usize::try_from(st.size).unwrap_or(data.len());
            let text = Text::decode(&data, ccsid).ok_or_else(|| {
                let m = Msg::new("CPFB618", &[&ccsid.to_string()]);
                fail(&m, ccsid::conv::from_str(37, &format!("{ccsid:<5}")).unwrap_or_default())
            })?;
            (text, if t.member { st.ccsid } else { ccsid }, size, true)
        }
        Err(Errno::ENOENT) if mode == Mode::Edit && !t.member => (Text::default(), 1208, 0, false),
        Err(e) => return Err(errno_fail(e, &t.path)),
    };
    let mut s = Session::new(mode, &t.path, text, ccsid, job, size);
    if mode == Mode::Browse && s.buf.is_empty() {
        s.message = Some(Msg::new("CPFB619", &[]));
    }
    let mut surface = Surface::new(host.clone());
    let mut st = State {
        c,
        t,
        exists,
        surface: &mut surface,
    };
    st.run(&mut s)
}

/// The session's surroundings.
struct State<'a> {
    c: &'a dyn IfsClient,
    t: Target,
    exists: bool,
    surface: &'a mut Surface,
}

impl State<'_> {
    /// Draw with `f`, wait for a key.
    fn exchange(&mut self, f: impl FnOnce(&mut ScreenModel)) -> Option<u8> {
        self.surface.with_model(f);
        self.surface.flush(Read::MdtFields);
        self.surface.wait().map(|(a, _)| a)
    }

    fn run(&mut self, s: &mut Session) -> Result<String, Fail> {
        loop {
            let Some(a) = self.exchange(|m| edtf::screen::draw(s, m)) else {
                return Ok(String::new());
            };
            let Some(key) = key_of(a) else {
                continue;
            };
            let input = self.surface.with_model(|m| edtf::screen::read(s, m));
            match s.key(key, &input) {
                Action::Stay => {}
                Action::Exit => return Ok(String::new()),
                Action::Save => {
                    self.save(s);
                }
                Action::SaveExit => {
                    if self.save(s) {
                        return Ok(String::new());
                    }
                }
                Action::Services => self.services(s),
            }
        }
    }

    /// The services display until Enter takes it or F3/F12 leaves it.
    fn services(&mut self, s: &mut Session) {
        let mut message: Option<Msg> = None;
        loop {
            let m0 = message.take();
            let Some(a) = self.exchange(|m| edtf::services::draw(s, m, m0.as_ref())) else {
                return;
            };
            match key_of(a) {
                Some(Key::Enter) => {
                    let ch = self.surface.with_model(|m| edtf::services::read(m));
                    match edtf::services::apply(s, &ch) {
                        Ok(()) => return,
                        Err(m) => message = Some(m),
                    }
                }
                Some(Key::F(3 | 12)) => return,
                _ => {}
            }
        }
    }

    /// Write the file; whether it worked (the message says why not).
    fn save(&mut self, s: &mut Session) -> bool {
        let path = self.t.path.clone();
        let r = if self.t.member {
            let text = Text::encode(&s.buf.texts(), edtf::text::Eol::Lf, true, MEMBER_CCSID).unwrap_or_default();
            self.write(&path, &text, OpenArgs::new(O_WRONLY | O_TRUNC | O_TEXTDATA | O_CCSID).conv(u32::from(MEMBER_CCSID)))
        } else {
            let Some(bytes) = s.bytes() else {
                let ccsid = s.new_ccsid.unwrap_or(s.file_ccsid);
                s.message = Some(Msg::new("CPFB618", &[&ccsid.to_string()]));
                return false;
            };
            let args = if self.exists {
                OpenArgs::new(O_WRONLY | O_TRUNC)
            } else {
                let ccsid = s.new_ccsid.unwrap_or(s.file_ccsid);
                OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC | O_CCSID)
                    .mode(0o644)
                    .conv(u32::from(ccsid))
            };
            self.write(&path, &bytes, args).and_then(|()| match s.new_ccsid {
                Some(n) if self.exists => self.c.set_attr(&path, &Attr::Ccsid(n), true),
                _ => Ok(()),
            })
        };
        match r {
            Ok(()) => {
                self.exists = true;
                s.saved();
                true
            }
            Err(e) => {
                s.message = Some(Msg {
                    id: e.command_message(),
                    text: format!("{path}: {}", e.text()),
                });
                false
            }
        }
    }

    fn write(&self, path: &str, data: &[u8], args: OpenArgs) -> Result<(), Errno> {
        let fd = self.c.open(path, args)?;
        let r = write_all(self.c, fd, data);
        let closed = self.c.close(fd);
        r.and(closed)
    }
}
