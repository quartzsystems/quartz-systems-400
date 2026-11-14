// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/power`: power down the system or IPL it (`PWRDWNSYS`). The form
//! chooses restart or power off, how jobs end and the IPL source; a
//! confirmation names what will happen, and OK runs the command as the
//! user (special authority `*JOBCTL`).

use std::fmt::Write as _;

use qsweb::html::esc;
use qsweb::http::{Form, Response};
use qsweb::pages::confirm_body;
use qsweb::partials::{Button, Prop, PropGroup, input, props, radio, select};

use super::GROUP;
use crate::env::{Need, Target};
use crate::routes::Rq;
use crate::routes::work::reply;

const SOURCES: &[&str] = &["*PANEL", "A", "B"];
const CRUMB: &str = "Power down / IPL";

/// What the form chose.
#[derive(Debug, PartialEq, Eq)]
struct Choice {
    restart: bool,
    immediate: bool,
    delay: u32,
    source: String,
}

impl Choice {
    /// The posted choice; a delay outside 1 to 99999 seconds is refused.
    fn of(f: &Form) -> Result<Self, String> {
        let delay = if f.str("o") == "*IMMED" {
            0
        } else {
            let d = f.str("d").trim();
            d.parse::<u32>()
                .ok()
                .filter(|n| (1..=99_999).contains(n))
                .ok_or_else(|| format!("The delay {d} is not 1 to 99999 seconds."))?
        };
        Ok(Self {
            restart: f.str("a") != "off",
            immediate: f.str("o") == "*IMMED",
            delay,
            source: SOURCES
                .iter()
                .find(|s| **s == f.str("s"))
                .unwrap_or(&SOURCES[0])
                .to_string(),
        })
    }

    fn command(&self) -> String {
        let how = if self.immediate {
            "OPTION(*IMMED)".to_string()
        } else {
            format!("OPTION(*CNTRLD) DELAY({})", self.delay)
        };
        let restart = if self.restart {
            format!("RESTART(*YES) IPLSRC({})", self.source)
        } else {
            "RESTART(*NO)".into()
        };
        format!("PWRDWNSYS {how} {restart} CONFIRM(*NO)")
    }

    /// What will happen, in a sentence.
    fn sentence(&self) -> String {
        let when = if self.immediate {
            "at once, without waiting for jobs to end".to_string()
        } else {
            format!(
                "after giving jobs {} seconds to end (controlled)",
                self.delay
            )
        };
        let then = if self.restart {
            match self.source.as_str() {
                "A" => "then IPLs from slot A".to_string(),
                "B" => "then IPLs from slot B".to_string(),
                _ => "then IPLs from the image the system would start next".to_string(),
            }
        } else {
            "then powers off; it stays off until started again".to_string()
        };
        format!(
            "Press OK to end every job and power down the system {when}; it {then}. Every user, including you, is disconnected."
        )
    }

    /// The choice as hidden fields, carried to OK.
    fn hidden(&self) -> String {
        let mut h = String::new();
        for (n, v) in [
            ("a", if self.restart { "ipl" } else { "off" }),
            ("o", if self.immediate { "*IMMED" } else { "*CNTRLD" }),
            ("d", &self.delay.to_string()),
            ("s", &self.source),
            ("go", "1"),
        ] {
            let _ = write!(
                h,
                "<input type=\"hidden\" name=\"{n}\" value=\"{}\">",
                esc(v)
            );
        }
        h
    }
}

/// `/power`.
pub fn page(rq: &Rq<'_>) -> Response {
    if rq.post() {
        return post(rq);
    }
    let st = rq.env().system.status(rq.user(), false);
    let last = if st.ipl == 0 {
        "Not known".to_string()
    } else if st.ipl_kind.is_empty() {
        rq.when(st.ipl)
    } else {
        format!("{} ({})", rq.when(st.ipl), esc(&st.ipl_kind))
    };
    let groups = [
        PropGroup {
            title: None,
            rows: vec![Prop::new("Last IPL", last)],
        },
        PropGroup {
            title: Some("Power down".into()),
            rows: vec![
                Prop::new(
                    "After ending",
                    format!(
                        "{} IPL the system (restart) &nbsp; {} Power off",
                        radio("a", "ipl", true),
                        radio("a", "off", false)
                    ),
                ),
                Prop::new(
                    "How jobs end",
                    format!(
                        "{} Controlled, within {} seconds &nbsp; {} Immediately",
                        radio("o", "*CNTRLD", true),
                        input("d", "60", 5),
                        radio("o", "*IMMED", false)
                    ),
                ),
                Prop::new(
                    "IPL source",
                    format!(
                        "{} <span class=\"small grey\">*PANEL starts the image the system would start next (after APYPTF, the new one); A and B name a slot.</span>",
                        select("s", SOURCES, "*PANEL")
                    ),
                ),
            ],
        },
    ];
    let body = props(
        &rq.ctx,
        &groups,
        &[
            Button::new("next", "Power down..."),
            Button::new("cancel", "Cancel"),
        ],
    );
    super::page(
        rq,
        &[CRUMB],
        CRUMB,
        Some(
            "End every job and power the system down, then restart it (an IPL) or leave it off (PWRDWNSYS).",
        ),
        body,
        "power",
        Some("/power"),
    )
}

fn post(rq: &Rq<'_>) -> Response {
    let f = rq.form();
    if f.has("cancel") {
        return rq.redirect("/home");
    }
    let c = match Choice::of(&f) {
        Ok(c) => c,
        Err(e) => {
            return reply(
                rq,
                Err(qsweb::msg::Message::new(
                    qsweb::msg::Kind::Error,
                    "CPD0084",
                    &e,
                )),
                "/power",
            );
        }
    };
    let target = Target::obj("QSYS", "PWRDWNSYS", "*CMD");
    if let Err(r) = rq.require(Need::JobCtl, "Power down system", &target, &[GROUP, CRUMB]) {
        return r;
    }
    if f.has("ok") && f.str("go") == "1" {
        return reply(rq, rq.env().config.run(rq.user(), &c.command()), "/home");
    }
    // The confirmation: what will happen, and the command.
    let note = format!(
        "{}<p class=\"small grey\">Equivalent command: <span class=\"mono\">{}</span></p>",
        c.hidden(),
        esc(&c.command())
    );
    let body = confirm_body(&rq.ctx, &c.sentence(), "", &note);
    let title = if c.restart {
        "Confirm IPL"
    } else {
        "Confirm Power Down"
    };
    super::page(
        rq,
        &[CRUMB, title],
        title,
        None,
        body,
        "power",
        Some("/power"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(pairs: &[(&str, &str)]) -> Form {
        Form::new(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        )
    }

    #[test]
    fn commands() {
        let c = Choice::of(&form(&[
            ("a", "ipl"),
            ("o", "*CNTRLD"),
            ("d", "60"),
            ("s", "B"),
        ]))
        .unwrap();
        assert_eq!(
            c.command(),
            "PWRDWNSYS OPTION(*CNTRLD) DELAY(60) RESTART(*YES) IPLSRC(B) CONFIRM(*NO)"
        );
        assert!(c.sentence().contains("slot B"));
        let c = Choice::of(&form(&[("a", "off"), ("o", "*IMMED"), ("s", "A")])).unwrap();
        assert_eq!(
            c.command(),
            "PWRDWNSYS OPTION(*IMMED) RESTART(*NO) CONFIRM(*NO)"
        );
        assert!(Choice::of(&form(&[("o", "*CNTRLD"), ("d", "0")])).is_err());
        assert!(Choice::of(&form(&[("o", "*CNTRLD"), ("d", "x")])).is_err());
        let back = Choice::of(&form(&[
            ("a", "ipl"),
            ("o", "*CNTRLD"),
            ("d", "90"),
            ("s", "Z"),
        ]))
        .unwrap();
        assert_eq!(back.source, "*PANEL");
        assert!(back.hidden().contains("name=\"go\" value=\"1\""));
    }
}
