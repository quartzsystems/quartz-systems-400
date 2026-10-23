// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `STRSEU` at the job's display: the member opened through the
//! storage service (locked `*EXCL` to edit, `*SHRRD` to browse), the
//! session of `devtools/seu` drawn through SEU's display files, each
//! key read back into it, the displays around it (Exit, session
//! defaults, find and browse options, the system command window,
//! Select Prompt), CL prompted by the command prompter and checked by
//! the analyzer, DDS checked by its compiler, the member written back.

mod clprompt;
mod dialogs;
mod print;

use std::cell::RefCell;

use cmd::{Resolved, Value};
use dspf::rt::Env;
use mi::Job;
use seu::display::{Screens, read_session, show_session};
use seu::session::{Action, Key};
use seu::srctype::{Checker, Family};
use seu::{Buffer, Mode, Session};
use stg::Client;
use stg::proto::SrcMember;
use tn5250::ds::aid;
use tn5250::screen::{Read, ScreenModel};

use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;

thread_local! {
    /// The member last edited or browsed, for `*PRV`.
    static PREVIOUS: RefCell<Option<(String, String, String)>> = const { RefCell::new(None) };
}

/// The text of a parameter's value.
fn parm(r: &Resolved, kwd: &str) -> String {
    r.get(kwd)
        .and_then(|b| b.value.as_ref())
        .map(Value::source)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Today as `yymmdd`.
fn today(env: &Env) -> u32 {
    let (y, m, d) = env.date;
    u32::from(y % 100) * 10_000 + u32::from(m) * 100 + u32::from(d)
}

/// The key a display's attention identifier stands for.
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

/// Everything one `STRSEU` holds.
pub(crate) struct Ctx<'a> {
    pub job: &'a mut Job,
    pub host: &'a Host,
    pub surface: Surface,
    pub screens: Screens,
    pub env: Env,
    /// The member as opened: real library, file, member.
    pub at: (String, String, String),
    /// The member exists (else it is added when saved).
    pub exists: bool,
}

impl Ctx<'_> {
    /// Draw with `f` and wait for a key: the key and the cursor.
    pub fn exchange(&mut self, f: impl FnOnce(&mut Screens, &mut ScreenModel, &Env)) -> Option<(u8, (usize, usize))> {
        let env = self.env.clone();
        let screens = &mut self.screens;
        self.surface.with_model(|m| f(screens, m, &env));
        self.surface.flush(Read::MdtFields);
        let (a, (r, c)) = self.surface.wait()?;
        Some((a, (usize::from(r), usize::from(c))))
    }

    /// Read the model with `f`.
    pub fn read<T>(&mut self, f: impl FnOnce(&ScreenModel) -> T) -> T {
        self.surface.with_model(|m| f(m))
    }

    /// Release the member.
    pub fn close(&mut self) {
        let (lib, file, member) = self.at.clone();
        self.host
            .link
            .borrow_mut()
            .with(|l| l.seu_close(lib, file, member));
    }

    /// Write the session's records to a member (the opened one unless
    /// the operator named another).
    pub fn save(&mut self, s: &mut Session, to: (&str, &str, &str)) -> Result<(), String> {
        let (lib, file, member) = to;
        let records = s.records();
        let srctype = s.member_type.clone();
        let text = s.text.clone();
        self.host
            .link
            .borrow_mut()
            .with(|l| {
                l.seu_save(
                    lib.into(),
                    file.into(),
                    member.into(),
                    records,
                    srctype,
                    text,
                )
            })?;
        if (lib, file, member) == (self.at.0.as_str(), self.at.1.as_str(), self.at.2.as_str()) {
            s.buf.saved();
            self.exists = true;
        }
        Ok(())
    }
}

/// `STRSEU SRCFILE SRCMBR TYPE OPTION TEXT`.
pub fn run(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    if !crate::panels::shows_panels(host, r) {
        return Err(Fail::new("CPF9871", "SEU needs a display: run STRSEU from an interactive job."));
    }
    let (lib, file, member) = target(r)?;
    if member == "*SELECT" {
        return Err(Fail::new("CPF9871", "The member list comes with PDM: name the member."));
    }
    let option = parm(r, "OPTION");
    let mode = match option.as_str() {
        "5" | "6" => Mode::Browse,
        _ => Mode::Edit,
    };
    let opened: SrcMember = host
        .link
        .borrow_mut()
        .with(|l| l.seu_open(lib.clone(), file.clone(), member.clone(), mode == Mode::Edit))
        .map_err(|e| fail_of(&e))?;
    PREVIOUS.with(|p| *p.borrow_mut() = Some((opened.lib.clone(), opened.file.clone(), opened.member.clone())));
    let sysname = crate::display::dds::system_name(host);
    let env = crate::display::dds::env_of(host, &sysname);
    let mut surface = Surface::new(host.clone());
    let wide = surface.wide();
    let mut ctx = Ctx {
        job,
        host,
        surface,
        screens: Screens::new(wide),
        env,
        at: (opened.lib.clone(), opened.file.clone(), opened.member.clone()),
        exists: opened.exists,
    };
    if option == "6" {
        let r = print::member(&mut ctx, &opened);
        ctx.close();
        return r;
    }
    let ty = parm(r, "TYPE");
    let srctype = if ty.is_empty() || ty == "*SAME" {
        opened.srctype.clone()
    } else {
        ty
    };
    let buf = Buffer::new(opened.width as usize, opened.records.clone());
    let day = today(&ctx.env);
    let mut s = Session::new(mode, buf, &srctype, (&opened.lib, &opened.file, &opened.member), day);
    s.member_type = srctype;
    s.text = match parm(r, "TEXT").as_str() {
        "" | "*BLANK" | "*SAME" => opened.text.clone(),
        t => t.trim_matches('\'').to_string(),
    };
    if !opened.exists {
        s.message = Some(format!("Member {} is new; it is added when saved.", opened.member));
    }
    let r = edit(&mut ctx, &mut s);
    ctx.close();
    r
}

/// `SRCFILE` and `SRCMBR`, `*PRV` from the last session.
fn target(r: &Resolved) -> Result<(String, String, String), Fail> {
    let prv = PREVIOUS.with(|p| p.borrow().clone());
    let srcfile = parm(r, "SRCFILE");
    let (lib, file) = if srcfile.is_empty() || srcfile.eq_ignore_ascii_case("*PRV") {
        let Some((l, f, _)) = prv.clone() else {
            return Err(Fail::new("CPF9871", "No member was edited before: name the source file."));
        };
        (l, f)
    } else {
        let (l, f) = wm::obj::params::qualified(&srcfile);
        (if l.is_empty() { "*LIBL".into() } else { l }, f)
    };
    let mbr = parm(r, "SRCMBR").to_ascii_uppercase();
    let member = if mbr.is_empty() || mbr == "*PRV" {
        match prv {
            Some((_, f, m)) if f == file => m,
            _ => return Err(Fail::new("CPF9871", "No member of this file was edited before: name the member.")),
        }
    } else {
        mbr
    };
    Ok((lib, file, member))
}

/// A service refusal as a failure: its identifier leads the text.
fn fail_of(e: &str) -> Fail {
    let (id, text) = match e.split_once(' ') {
        Some((id, rest)) if id.len() == 7 && id.chars().all(|c| c.is_ascii_alphanumeric()) => (id, rest),
        _ => ("CPF9898", e),
    };
    Fail::new(id, text.to_string())
}

/// The edit or browse display until the session ends.
fn edit(ctx: &mut Ctx<'_>, s: &mut Session) -> Result<String, Fail> {
    loop {
        let v = s.view();
        let Some((a, cursor)) = ctx.exchange(|sc, m, env| show_session(sc, m, env, &v)) else {
            return Ok(String::new());
        };
        let Some(key) = key_of(a) else {
            s.message = Some("That key is not used here.".into());
            continue;
        };
        let input = ctx.read(|m| read_session(m, &v, cursor));
        let action = s.key(key, &input);
        check(ctx, s);
        let file_too = matches!(action, Action::File(_));
        match action {
            Action::Show => {}
            Action::Cancel => return Ok(String::new()),
            Action::Exit => {
                if dialogs::exit(ctx, s)? {
                    return Ok(String::new());
                }
            }
            Action::Save(to) | Action::File(to) => {
                let lib = if to.lib.is_empty() { ctx.at.0.clone() } else { to.lib };
                let file = if to.file.is_empty() { ctx.at.1.clone() } else { to.file };
                let member = if to.member.is_empty() { ctx.at.2.clone() } else { to.member };
                match ctx.save(s, (&lib, &file, &member)) {
                    Ok(()) if file_too => return Ok(String::new()),
                    Ok(()) => s.message = Some(format!("Member {member} saved.")),
                    Err(e) => s.message = Some(e),
                }
            }
            Action::ClPrompt(t, _) => clprompt::prompt(ctx, s, t),
            Action::Defaults => dialogs::defaults(ctx, s),
            Action::FindOptions => dialogs::find(ctx, s),
            Action::BrowseOptions => dialogs::browse(ctx, s),
            Action::SystemCommand => dialogs::command(ctx, s),
            Action::SelectPrompt(t) => dialogs::select(ctx, s, t),
        }
    }
}

/// Syntax checking of the records the last key added or changed, when
/// the session checks and they are in its range.
pub(crate) fn check(ctx: &mut Ctx<'_>, s: &mut Session) {
    if !s.defaults.syntax || s.touched.is_empty() || s.mode == Mode::Browse {
        return;
    }
    let st = s.srctype();
    let in_range = |seq: u32| {
        s.defaults.syntax_from.is_none_or(|f| seq >= f) && s.defaults.syntax_to.is_none_or(|t| seq <= t)
    };
    let touched: Vec<(u64, usize)> = s
        .touched
        .iter()
        .filter_map(|id| s.buf.index_of(*id).map(|i| (*id, i)))
        .filter(|(_, i)| in_range(s.buf.records()[*i].seq))
        .collect();
    if touched.is_empty() {
        return;
    }
    for (id, _) in &touched {
        s.errors.remove(id);
    }
    match st.checker() {
        Checker::Cl => {
            let recs: Vec<String> = s.buf.records().iter().map(|r| r.data.clone()).collect();
            for (id, i) in touched {
                let Some(stmt) = seu::cl::statement_at(&recs, i) else {
                    continue;
                };
                if stmt.command.trim().is_empty() {
                    continue;
                }
                if let Err(msg) = clprompt::analyze(ctx, &stmt.command, st.family == Family::Cmd) {
                    s.syntax_error(id, &msg);
                    return;
                }
            }
        }
        Checker::Dds => {
            let recs = s.records();
            let idx: Vec<usize> = touched.iter().map(|(_, i)| *i).collect();
            if let Some((i, msg)) = seu::display::check::dds(&st.name, &recs, &idx) {
                let id = s.buf.records()[i].id;
                s.syntax_error(id, &msg);
            }
        }
        Checker::Rpg => {
            for (id, i) in touched {
                if let Some(msg) = seu::display::check::rpg(&s.buf.records()[i].data) {
                    s.syntax_error(id, &msg);
                    return;
                }
            }
        }
        Checker::None => {}
    }
}
