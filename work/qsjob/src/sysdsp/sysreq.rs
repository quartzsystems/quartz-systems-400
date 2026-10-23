// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The System Request menu: display file `QSYS/QWSSYSRQ` (the one
//! shipped in `ui/dspf/sys/` when the object cannot be opened), its
//! help in panel group `QWSSYSRQ`. Options 1 to 7, 80 and 90 always
//! show; 13 and 14 for a group job, 15 when a secondary job waits.
//! An option's display runs at the System Request level; leaving it
//! goes back to the interrupted job.

use dspf::rt::Runtime;
use dspf::{CompileOpts, DisplayFile, NoRefs, Source, compile_dspf};
use stg::Client;
use tn5250::ds::{aid, attr};
use tn5250::screen::Read;

use super::keys::{self, Act};
use super::{group, wsusr};
use crate::command::RunMode;
use crate::console::host::Host;
use crate::display::dds;
use crate::display::surface::Surface;

/// The menu's display file: the object, else the shipped source.
fn file(host: &Host) -> Option<DisplayFile> {
    let opened = host
        .link
        .borrow_mut()
        .with(|l| l.dsp_open("QWSSYSRQ".into(), "QSYS".into(), "*JOB".into()))
        .ok()
        .and_then(|(b, _)| dspf::decode(&b).ok());
    opened.or_else(|| {
        let (src, _) = Source::from_text(dspf::sys::QWSSYSRQ);
        compile_dspf(&src, &NoRefs, &CompileOpts::default()).definition
    })
}

/// What the menu shows beyond its first form.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    /// The job is a group job: options 13 and 14.
    pub group: bool,
    /// Another job waits at the display: option 15.
    pub secondary: bool,
}

impl Shown {
    /// The format and its option indicators.
    #[must_use]
    pub fn format(self) -> (&'static str, Vec<u8>) {
        if !self.group && !self.secondary {
            return ("SYSRQ", Vec::new());
        }
        let mut inds = vec![50];
        if self.group {
            inds.push(51);
        }
        if self.secondary {
            inds.push(52);
        }
        ("SYSRQ2", inds)
    }
}

/// What an option typed on the menu leads to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    /// Leave the menu (Enter with nothing typed).
    Leave,
    /// A command at the System Request level, with its mode.
    Command(&'static str, RunMode),
    /// Display Work Station User.
    WsUser,
    /// The secondary job: sign on one, or go to it.
    Secondary,
    /// End the request.
    EndRequest,
    /// Transfer to a group job: `*SELECT` or `*PRV`.
    Group(&'static str),
    /// Disconnect the job.
    Disconnect,
    /// Sign off.
    SignOff,
    /// Not on this menu: the message.
    Refused(String),
}

/// The option typed, as the menu shown takes it.
#[must_use]
pub fn choice(typed: &str, shown: Shown) -> Choice {
    let t = typed.trim();
    if t.is_empty() {
        return Choice::Leave;
    }
    match t.parse::<u8>() {
        Ok(1) => Choice::Secondary,
        Ok(2) => Choice::EndRequest,
        Ok(3) => Choice::Command("DSPJOB", RunMode::Run),
        Ok(4) => Choice::Command("DSPMSG", RunMode::PromptRun),
        Ok(5) => Choice::Command("SNDMSG", RunMode::PromptRun),
        Ok(6) => Choice::Command("DSPMSG MSGQ(QSYSOPR)", RunMode::Run),
        Ok(7) => Choice::WsUser,
        Ok(n @ (10 | 11)) => Choice::Refused(format!(
            "Option {n} is not available: this system has no display station pass-through."
        )),
        Ok(13) if shown.group => Choice::Group("*SELECT"),
        Ok(14) if shown.group => Choice::Group("*PRV"),
        Ok(15) if shown.secondary => Choice::Secondary,
        Ok(80) => Choice::Disconnect,
        Ok(90) => Choice::SignOff,
        _ => Choice::Refused(format!("Option {t} is not valid.")),
    }
}

/// What the menu shows for this job now.
fn shown(host: &Host) -> Shown {
    let me = host.start.number;
    let device = host.start.device.clone();
    let mut l = host.link.borrow_mut();
    let group = l.with(Client::grp_info).is_some();
    let jobs = l.with(|l| l.ws_get(device));
    let side = jobs.iter().find(|j| j.0 == me).map_or(1, |j| j.3);
    Shown {
        group,
        secondary: jobs.iter().any(|j| j.3 != side),
    }
}

/// The System Request menu until an option or a key leaves it.
pub fn run(host: &Host) -> Act {
    let Some(df) = file(host) else {
        return Act::Say("The System Request menu is not available.".into());
    };
    let mut surface = Surface::new(host.clone());
    let wide = surface.wide();
    let mut rt = Runtime::new(df, wide);
    let sysname = dds::system_name(host);
    let env = dds::env_of(host, &sysname);
    let shown = shown(host);
    let (format, inds) = shown.format();
    let Some(rec) = rt.file().record(format).cloned() else {
        return Act::Back;
    };
    let mut message: Option<String> = None;
    loop {
        let mut out = dds::out_buffer(&rec, &[("SYSNAME", &sysname)]);
        for (i, n) in rec.layout.output.indicators.iter().enumerate() {
            if inds.contains(n) {
                out[i] = 0xF1;
            }
        }
        let written = surface.with_model(|m| {
            let r = rt.write(m, &env, format, &out, &[]);
            if let Some(text) = message.take() {
                m.error_line(&text, attr::WHITE);
                m.alarm();
            }
            r
        });
        if written.is_err() {
            return Act::Back;
        }
        surface.flush(Read::MdtFields);
        let Some((a, _)) = surface.wait() else {
            return Act::Gone;
        };
        let typed = surface.with_model(|m| {
            m.fields()
                .iter()
                .find(|f| f.row == 20 && f.col == 7)
                .map(tn5250::screen::Field::text)
                .unwrap_or_default()
        });
        match a {
            aid::ENTER => {}
            aid::HELP => {
                if !super::help(host, &mut surface, "QWSSYSRQ", "HLPSYSRQ") {
                    return Act::Gone;
                }
                continue;
            }
            a if aid::function_of(a) == Some(1) => {
                if !super::help(host, &mut surface, "QWSSYSRQ", "HLPSYSRQ") {
                    return Act::Gone;
                }
                continue;
            }
            a if matches!(aid::function_of(a), Some(3 | 12)) => return Act::Back,
            _ => {
                message = Some("Function key not allowed.".into());
                continue;
            }
        }
        match choice(&typed, shown) {
            Choice::Leave => return Act::Back,
            Choice::Refused(m) => message = Some(m),
            Choice::EndRequest => return Act::EndRequest,
            Choice::SignOff => return Act::SignOff,
            Choice::Disconnect => return group::disconnect(host),
            Choice::Secondary => return group::secondary(host),
            Choice::Group(which) => match group::transfer_to(host, which, "") {
                Ok(act) => return act,
                Err(m) => message = Some(m),
            },
            Choice::WsUser => return wsusr::show(host),
            Choice::Command(text, mode) => {
                let o = keys::command(host, text, mode);
                if keys::unwinding() {
                    return Act::Gone;
                }
                match keys::message_of(&o) {
                    Some(m) if o.escape.is_some() => message = Some(m),
                    _ => return Act::Back,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options() {
        let alone = Shown::default();
        assert_eq!(choice("", alone), Choice::Leave);
        assert_eq!(choice(" 3", alone), Choice::Command("DSPJOB", RunMode::Run));
        assert_eq!(choice("90", alone), Choice::SignOff);
        assert!(matches!(choice("13", alone), Choice::Refused(_)));
        assert!(matches!(choice("10", alone), Choice::Refused(m) if m.contains("pass-through")));
        let grp = Shown {
            group: true,
            secondary: false,
        };
        assert_eq!(choice("14", grp), Choice::Group("*PRV"));
        assert!(matches!(choice("15", grp), Choice::Refused(_)));
        assert_eq!(grp.format(), ("SYSRQ2", vec![50, 51]));
        assert_eq!(alone.format(), ("SYSRQ", vec![]));
    }
}
