// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Display Work Station User (System Request option 7, `DSPWSUSR`):
//! the user, the device and the interactive jobs at it as the session
//! holding it reports them (job A the primary, B the secondary).

use stg::Client;

use super::keys::Act;
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::Response;

/// An object's text.
fn text_of(host: &Host, name: &str, ty: &str) -> String {
    host.link
        .borrow_mut()
        .with(|l| l.object("QSYS".into(), name.into(), ty.into()))
        .map(|o| o.text)
        .unwrap_or_default()
}

/// Fill and show the panel; the act that left it.
pub fn show(host: &Host) -> Act {
    let Ok(mut d) = crate::panels::dialog(host, "QWCDSPWSU") else {
        return Act::Say("Display Work Station User is not available.".into());
    };
    let device = host.start.device.clone();
    let mut jobs = host
        .link
        .borrow_mut()
        .with(|l| l.ws_get(device.clone()));
    let me = host.start.number;
    // Without the session's word, this job is the only one.
    if !jobs.iter().any(|j| j.0 == me) {
        jobs.push((me, host.start.user.clone(), host.start.name.clone(), 1));
    }
    let env = crate::display::dds::env_of(host, "");
    let (year, month, day) = env.date;
    let (hour, minute, second) = env.time;
    d.set(
        "*DATETIME",
        &format!("{month:02}/{day:02}/{:02}  {hour:02}:{minute:02}:{second:02}", year % 100),
    );
    let user = host.start.user.clone();
    d.set("USER", &user);
    d.set("USRTEXT", &text_of(host, &user, "*USRPRF"));
    d.set("WSNAME", &device);
    d.set("WSTEXT", &text_of(host, &device, "*DEVD"));
    let sides = [1u8, 2u8].map(|side| jobs.iter().find(|j| j.3 == side));
    let count = sides.iter().flatten().count();
    d.set("NBRJOBS", &count.to_string());
    let mine = jobs.iter().find(|j| j.0 == me).map_or(1, |j| j.3);
    d.set("CURJOB", if mine == 2 { "B" } else { "A" });
    for (var, j) in [("JOBA", sides[0]), ("JOBB", sides[1])] {
        let v = j.map_or_else(|| "*NONE".to_string(), |j| format!("{:06}/{}/{}", j.0, j.1, j.2));
        d.set(var, &v);
    }
    d.copyright = true;
    let mut surface = Surface::new(host.clone());
    match d.show(&mut surface, "DSPWSUSR") {
        Ok(Response::Gone) => Act::Gone,
        _ => Act::Back,
    }
}

/// `DSPWSUSR`.
#[allow(clippy::unnecessary_wraps)] // the commands' signature
pub fn dspwsusr(host: &Host) -> Result<String, Fail> {
    if show(host) == Act::Gone {
        super::keys::gone();
    }
    Ok(String::new())
}
