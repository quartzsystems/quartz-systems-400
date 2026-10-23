// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The displays SEU opens from the edit display: Exit, the session
//! defaults, the find and browse options, the system command window
//! and Select Prompt.

use seu::defaults::Reseq;
use seu::display::{BrowseForm, Screens};
use seu::exit::ExitForm;
use seu::session::{Action, Key, Target};
use seu::{Buffer, Mode, Session};
use stg::Client;
use tn5250::ds::attr;

use super::{Ctx, key_of};
use crate::command::Fail;

/// The Exit display: whether SEU ends (else back to editing).
pub fn exit(ctx: &mut Ctx<'_>, s: &mut Session) -> Result<bool, Fail> {
    let changed = s.buf.changed() || !ctx.exists;
    let reseq = !matches!(s.defaults.reseq, Reseq::No);
    let errors = s.has_errors();
    let mut form = ExitForm::new(&ctx.at.2, &ctx.at.1, &ctx.at.0, &s.text, changed, reseq, errors);
    let mut message: Option<String> = errors.then(|| "Records of the member are in error; it can be saved as it is.".to_string());
    loop {
        let shown = form.clone();
        let msg = message.take();
        let Some((a, _)) = ctx.exchange(|sc, m, env| {
            sc.show_exit(m, env, &shown);
            if let Some(t) = &msg {
                m.error_line(t, attr::WHITE);
            }
        }) else {
            return Ok(true);
        };
        match key_of(a) {
            Some(Key::F(12)) => return Ok(false),
            Some(Key::F(5)) => continue,
            Some(Key::Enter | Key::F(3)) => {}
            _ => {
                message = Some("That key is not used here.".into());
                continue;
            }
        }
        form = ctx.read(Screens::read_exit);
        let choice = match form.choice() {
            Ok(c) => c,
            Err(e) => {
                message = Some(e.text);
                continue;
            }
        };
        s.text.clone_from(&choice.text);
        if choice.save {
            if let Some((start, inc)) = choice.reseq
                && let Err(e) = s.buf.resequence(start, inc)
            {
                message = Some(format!("{e}: choose a smaller start or increment."));
                continue;
            }
            if let Err(e) = ctx.save(s, (&choice.lib, &choice.file, &choice.member)) {
                message = Some(e);
                continue;
            }
        }
        if choice.print
            && let Err(e) = super::print::records(ctx, &choice.member, &s.records())
        {
            message = Some(e.text);
            continue;
        }
        if choice.ret {
            s.message = Some(if choice.save {
                format!("Member {} saved.", choice.member)
            } else {
                "Back to editing; nothing was saved.".into()
            });
            return Ok(false);
        }
        // `Go to member list` is PDM's (`WRKMBRPDM`); SEU ends either way.
        return Ok(true);
    }
}

/// The session defaults: two pages in edit mode, one in browse mode.
pub fn defaults(ctx: &mut Ctx<'_>, s: &mut Session) {
    let mode = s.mode;
    let mut page = 0usize;
    let mut message: Option<String> = None;
    loop {
        let d = s.defaults.clone();
        let msg = message.take();
        let Some((a, _)) = ctx.exchange(|sc, m, env| {
            sc.show_defaults(m, env, mode, page, &d);
            if let Some(t) = &msg {
                m.error_line(t, attr::WHITE);
            }
        }) else {
            return;
        };
        let key = key_of(a);
        if matches!(key, Some(Key::F(3 | 12))) {
            return;
        }
        if key == Some(Key::F(5)) {
            continue;
        }
        // Take the page's values before moving on.
        let before = s.defaults.srctype.clone();
        let taken = if page == 0 {
            let p = ctx.read(|m| Screens::read_page1(m, mode, &d));
            s.defaults.set_page1(&p)
        } else {
            let p = ctx.read(|m| Screens::read_page2(m, mode, &d));
            s.defaults.set_page2(&p)
        };
        if let Err(e) = taken {
            message = Some(e.text);
            continue;
        }
        let after = std::mem::replace(&mut s.defaults.srctype, before);
        let forced = s.set_srctype(&after);
        if let Some(date) = s.defaults.set_date.take() {
            s.buf.date_all(date);
        }
        match key {
            Some(Key::PageDown) if mode == Mode::Edit => page = 1,
            Some(Key::PageUp) => page = 0,
            Some(Key::F(14)) => return find(ctx, s),
            Some(Key::F(15)) => return browse(ctx, s),
            _ => {
                s.message = forced;
                return;
            }
        }
    }
}

/// The find (and change) options; `F16`/`F17` search at once.
pub fn find(ctx: &mut Ctx<'_>, s: &mut Session) {
    let mode = s.mode;
    let width = s.buf.width;
    let mut message: Option<String> = None;
    loop {
        let o = s.find.clone();
        let msg = message.take();
        let Some((a, _)) = ctx.exchange(|sc, m, env| {
            sc.show_find(m, env, mode, &o, width);
            if let Some(t) = &msg {
                m.error_line(t, attr::WHITE);
            }
        }) else {
            return;
        };
        let key = key_of(a);
        match key {
            Some(Key::F(3 | 12)) => return,
            Some(Key::F(5)) => continue,
            Some(Key::F(13)) => return defaults(ctx, s),
            Some(Key::F(15)) => return browse(ctx, s),
            _ => {}
        }
        match ctx.read(|m| Screens::read_find(m, mode, &o)) {
            Ok(n) => s.set_find(n),
            Err(e) => {
                message = Some(e);
                continue;
            }
        }
        let f = match key {
            Some(Key::F(16)) => 16,
            Some(Key::F(17)) if mode == Mode::Edit => 17,
            _ => return,
        };
        let _ = s.key(Key::F(f), &seu::session::Input::default());
        super::check(ctx, s);
        return;
    }
}

/// Browse (and copy) options: the member case browses another member
/// over the display, or copies all of it after the last record.
pub fn browse(ctx: &mut Ctx<'_>, s: &mut Session) {
    let mode = s.mode;
    let user = ctx.host.start.user.clone();
    let mut form = BrowseForm::new(&ctx.at.2, &ctx.at.1, &ctx.at.0, &user);
    let mut message: Option<String> = None;
    loop {
        let shown = form.clone();
        let msg = message.take();
        let Some((a, _)) = ctx.exchange(|sc, m, env| {
            sc.show_browse(m, env, mode, &shown);
            if let Some(t) = &msg {
                m.error_line(t, attr::WHITE);
            }
        }) else {
            return;
        };
        match key_of(a) {
            Some(Key::F(3 | 12)) => return,
            Some(Key::F(5)) => continue,
            Some(Key::F(13)) => return defaults(ctx, s),
            Some(Key::F(14)) => return find(ctx, s),
            Some(Key::Enter) => {}
            _ => {
                message = Some("That key is not used here.".into());
                continue;
            }
        }
        form = ctx.read(|m| Screens::read_browse(m, mode));
        if form.selection.trim() != "1" {
            message = Some("Spooled files and output queues are browsed from WRKSPLF and WRKOUTQ.".into());
            continue;
        }
        let (lib, file, member) = (
            form.lib.trim().to_ascii_uppercase(),
            form.file.trim().to_ascii_uppercase(),
            form.member.trim().to_ascii_uppercase(),
        );
        let other = if (lib.as_str(), file.as_str(), member.as_str())
            == (ctx.at.0.as_str(), ctx.at.1.as_str(), ctx.at.2.as_str())
        {
            // The member being edited: what the session holds.
            Ok((s.records(), s.member_type.clone(), s.buf.width))
        } else {
            let got = ctx
                .host
                .link
                .borrow_mut()
                .with(|l| l.seu_open(lib.clone(), file.clone(), member.clone(), false));
            got.map(|o| {
                ctx.host
                    .link
                    .borrow_mut()
                    .with(|l| l.seu_close(o.lib.clone(), o.file.clone(), o.member.clone()));
                (o.records, o.srctype, o.width as usize)
            })
        };
        let (records, srctype, width) = match other {
            Ok(x) => x,
            Err(e) => {
                message = Some(e);
                continue;
            }
        };
        if mode == Mode::Edit && form.copy_all.trim().eq_ignore_ascii_case("Y") {
            let lines: Vec<String> = records.into_iter().map(|r| r.2).collect();
            let n = lines.len();
            let after = s.buf.len().checked_sub(1);
            match s.buf.insert(after, &lines, s.defaults.increment.max(100), s.today) {
                Ok(_) => s.message = Some(format!("{n} records copied from {member}.")),
                Err(e) => s.message = Some(format!("{e}: resequence the member first.")),
            }
            return;
        }
        // Browse the other member over the display; F3 or F12 comes back.
        let today = s.today;
        let mut b = Session::new(Mode::Browse, Buffer::new(width, records), &srctype, (&lib, &file, &member), today);
        browse_only(ctx, &mut b);
        return;
    }
}

/// A browse session inside an edit session.
fn browse_only(ctx: &mut Ctx<'_>, b: &mut Session) {
    loop {
        let v = b.view();
        let Some((a, cursor)) = ctx.exchange(|sc, m, env| seu::display::show_session(sc, m, env, &v)) else {
            return;
        };
        let Some(key) = key_of(a) else {
            continue;
        };
        let input = ctx.read(|m| seu::display::read_session(m, &v, cursor));
        match b.key(key, &input) {
            Action::Cancel | Action::Exit => return,
            Action::FindOptions => find(ctx, b),
            Action::Defaults => defaults(ctx, b),
            _ => {}
        }
    }
}

/// The system command window: a command run from SEU.
pub fn command(ctx: &mut Ctx<'_>, s: &mut Session) {
    let mut cmd = String::new();
    let mut message: Option<String> = None;
    loop {
        let v = s.view();
        let shown = cmd.clone();
        let msg = message.take();
        let Some((a, _)) = ctx.exchange(|sc, m, env| {
            seu::display::show_session(sc, m, env, &v);
            sc.show_window(m, env, &shown, msg.as_deref());
        }) else {
            return;
        };
        let typed = ctx.read(Screens::read_window);
        match key_of(a) {
            Some(Key::F(3 | 12)) => return,
            Some(Key::F(9)) => {
                cmd = s.history.last().cloned().unwrap_or_default();
                continue;
            }
            Some(Key::Enter | Key::F(4)) => {}
            _ => {
                message = Some("That key is not used here.".into());
                continue;
            }
        }
        if typed.trim().is_empty() {
            return;
        }
        let text = if key_of(a) == Some(Key::F(4)) {
            format!("?{}", typed.trim())
        } else {
            typed.trim().to_string()
        };
        let out = crate::command::run(ctx.job, ctx.host, &text);
        cmd = typed;
        match out.escape {
            Some(f) => message = Some(if f.text.is_empty() { f.msgid } else { f.text }),
            None => return,
        }
    }
}

/// Select Prompt: the formats of the session's type; the one chosen
/// prompts the line.
pub fn select(ctx: &mut Ctx<'_>, s: &mut Session, t: Target) {
    let st = s.srctype();
    let lines = seu::srctype::select_lines(&st);
    let mut message: Option<String> = None;
    loop {
        let msg = message.take();
        let Some((a, _)) = ctx.exchange(|sc, m, env| sc.show_select(m, env, &lines, msg.as_deref())) else {
            return;
        };
        match key_of(a) {
            Some(Key::F(3 | 12)) => return,
            Some(Key::Enter) => {}
            _ => {
                message = Some("That key is not used here.".into());
                continue;
            }
        }
        let fmt = ctx.read(Screens::read_select);
        if fmt.is_empty() {
            return;
        }
        if !st.prompt_formats().contains(&fmt.as_str()) {
            message = Some(format!("{fmt} is not a prompt of source type {}.", st.name));
            continue;
        }
        if let Action::ClPrompt(t, _) = s.start_prompt(t, Some(&fmt)) {
            super::clprompt::prompt(ctx, s, t);
        }
        return;
    }
}
