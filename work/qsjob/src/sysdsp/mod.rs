// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system displays a job shows wherever it waits for its display:
//! the keys that are the system's (`keys`: System Request, Attention,
//! Print), the System Request menu (`sysreq`), the attention program
//! (`attn`), the screen print (`print`), Display Program Messages
//! (`msgs`), the About panel of the Information Assistant (`about`),
//! secondary and group jobs (`group`), and the commands behind them.

pub mod about;
pub mod attn;
pub mod group;
pub mod keys;
pub mod msgs;
pub mod print;
pub mod sysreq;
mod wsusr;

use cmd::{Resolved, Value};
use mi::Job;
use tn5250::ds::aid;
use tn5250::screen::Read;
use uim::render::Options;

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// The programs of the system displays: `QUSCMDLN` (the command-line
/// window) and `QUIABOUT` (About QS/400).
pub fn install(job: &mut Job, host: &Host) {
    let h = host.clone();
    job.natives.insert(
        "QUSCMDLN".into(),
        std::rc::Rc::new(move |_job: &mut Job, _args: &[u64]| {
            if attn::cmdline(&h) == keys::Act::Gone {
                keys::gone();
            }
        }),
    );
    let h = host.clone();
    job.natives.insert(
        "QUIABOUT".into(),
        std::rc::Rc::new(move |_job: &mut Job, _args: &[u64]| about::show(&h)),
    );
}

/// The commands of the system displays, run in the job.
pub fn run(job: &mut Job, host: &Host, r: &Resolved, _out: &mut Outcome) -> Option<Result<String, Fail>> {
    let _ = job;
    Some(match r.name.as_str() {
        "SETATNPGM" => attn::setatnpgm(host, &parm(r, "PGM"), &parm(r, "SET")),
        "TFRSECJOB" => group::tfrsecjob(host),
        "CHGGRPA" => group::chggrpa(host, &parm(r, "GRPJOB"), &parm_text(r, "GRPJOBTXT")),
        "TFRGRPJOB" => group::tfrgrpjob(host, &parm(r, "GRPJOB"), &parm(r, "INLGRPPGM")),
        "ENDGRPJOB" => group::endgrpjob(host, &parm(r, "GRPJOB")),
        "DSPWSUSR" => wsusr::dspwsusr(host),
        _ => return None,
    })
}

/// A parameter's value as typed or defaulted, upper case; a qualified
/// name as `lib/name`.
fn parm(r: &Resolved, kwd: &str) -> String {
    let v = r.get(kwd).and_then(|b| b.value.as_ref());
    let text = match v {
        Some(Value::Qual(parts)) => {
            let names: Vec<String> = parts.iter().map(Value::source).collect();
            match names.as_slice() {
                [name, lib] if !lib.is_empty() => format!("{lib}/{name}"),
                [name, ..] => name.clone(),
                [] => String::new(),
            }
        }
        Some(v) => v.source(),
        None => String::new(),
    };
    text.trim_matches('\'').trim().to_ascii_uppercase()
}

/// A text parameter as typed.
fn parm_text(r: &Resolved, kwd: &str) -> String {
    r.get(kwd)
        .and_then(|b| b.value.as_ref())
        .map(Value::source)
        .unwrap_or_default()
        .trim_matches('\'')
        .trim()
        .to_string()
}

/// Show help module `module` of system panel group `group` until F3,
/// F12 or Enter; `false` when the job is to end.
pub fn help(host: &Host, surface: &mut Surface, group: &str, module: &str) -> bool {
    let Ok(g) = crate::panels::group(host, "QSYS", group) else {
        return true;
    };
    let mut page = 0;
    loop {
        let opts = Options {
            wide: surface.wide(),
            page,
            ..Options::default()
        };
        let Ok(screen) = uim::render_help(&g, module, &opts) else {
            return true;
        };
        surface.with_model(|m| crate::display::uim::apply(&screen, m));
        surface.flush(Read::MdtFields);
        let Some((a, _)) = surface.wait() else {
            return false;
        };
        match a {
            aid::ROLL_UP if screen.more.is_some_and(|m| !m.bottom) => page += 1,
            aid::ROLL_DOWN => page = page.saturating_sub(1),
            aid::ROLL_UP => {}
            _ => return true,
        }
    }
}
