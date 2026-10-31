// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Change Password display: at sign-on when the password has
//! expired, and for `CHGPWD` at a display. From password level 2 its
//! fields are 128 characters and the panel spaces them for it.

use mi::Job;
use stg::Client as _;
use stg::remote::status;
use wm::job::Status;
use wm::panels;

use crate::console::host::Host;

/// Show Change Password until the password changes (true) or the user
/// leaves with F3 or F12 (false). `message` shows first.
pub fn show(job: &mut Job, host: &Host, user: &str, mut message: String) -> bool {
    let long = host
        .link
        .borrow_mut()
        .with(|l| l.sysval("QPWDLVL".into()))
        .and_then(|v| super::telnet::level_of(v.kind, &v.data))
        .is_some_and(|n| n >= 2);
    let text = |id: &str| crate::display::msgline::text(host, id, &[]);
    loop {
        host.show(panels::change_password(user, 0, long, &message));
        status(&host.link, Status::Dspw, "PGM-QSYCHGPW");
        let Some((key, fields)) = host.wait_input(job) else {
            return false;
        };
        if key == 3 || key == 12 {
            return false;
        }
        if key != 0 {
            message = text("CPD6A60");
            continue;
        }
        let field = |i: usize| fields.get(i).map_or("", |f| f.trim()).to_string();
        let (current, new, confirm) = (field(0), field(1), field(2));
        if new != confirm {
            message = text("CPD9BAF");
            continue;
        }
        match super::pwdpgm::validate(job, &host.link, user, &new, &current) {
            Ok(true) => {}
            Ok(false) => {
                message = text("CPD9BB3");
                continue;
            }
            Err(e) => {
                message = crate::display::msgline::text_of(host, &crate::panels::line(&e));
                continue;
            }
        }
        match host.link.borrow_mut().with(|l| l.chg_pwd(current, new, true)) {
            Ok(()) => return true,
            Err(e) => message = crate::display::msgline::text_of(host, &crate::panels::line(&e)),
        }
    }
}
