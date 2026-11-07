// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `accessweb::env::print::Print` over the job's thread as the
//! signed-on user: the spooled files `WRKSPLF` lists, their data as
//! `DSPSPLF` reads it (decoded from SCS, the PDF from work/spool's
//! writer), the actions as the commands themselves (`HLDSPLF`,
//! `RLSSPLF`, `DLTSPLF`, `CHGSPLFA OUTQ()`), the writers of `WRKWTR`.

use accessweb::env::print::{Act, Print, Printer, Splf};
use qsweb::msg::{Kind, Message};
use stg::Client;
use wm::obj::splf::SplfAttrs;

use super::Link;
use crate::work::web::bridge::Main;
use crate::work::web::system::Core;

/// Printer output as the signed-on users see it.
pub struct LivePrint {
    /// The job's thread.
    pub link: Link,
    /// Special authorities.
    pub core: std::sync::Arc<Core>,
}

fn error(e: &str) -> Message {
    let id = e
        .split_whitespace()
        .next()
        .filter(|w| w.len() == 7 && w.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("CPF9898");
    Message::new(Kind::Error, id, e.strip_prefix(id).unwrap_or(e).trim())
}

fn gone() -> Message {
    Message::new(Kind::Error, "CPF22E9", "Sign on again.")
}

fn splf(a: &SplfAttrs) -> Splf {
    Splf {
        job: format!("{:06}/{}/{}", a.job.2, a.job.1, a.job.0),
        file: a.name.clone(),
        number: a.number,
        user: a.job.1.clone(),
        user_data: a.usrdta.trim().to_string(),
        status: a.status.code().to_string(),
        pages: a.pages,
        current_page: a.last_page,
        copies: u32::from(a.copies.max(1)),
        form_type: if a.formtype.trim().is_empty() {
            "*STD".into()
        } else {
            a.formtype.trim().to_string()
        },
        outq: a.outq.clone(),
        created: a.created / 1_000_000,
        priority: a.outpty,
        device_type: if a.devtype.trim().is_empty() {
            "*SCS".into()
        } else {
            a.devtype.trim().to_string()
        },
        page_len: u32::from(a.page_len),
        page_width: u32::from(a.page_width),
        lpi: u32::from(a.lpi),
        cpi: u32::from(a.cpi),
        overflow: u32::from(a.ovrflw),
        hold: a.status == wm::obj::splf::Status::Hld,
        save: a.save,
        record_len: u32::from(a.page_width) + 1,
        records: a.records,
        printer: "*OUTQ".into(),
        writer: a.writer.trim().to_string(),
    }
}

impl LivePrint {
    /// Whether `user` sees other users' files: `*SPLCTL` or `*JOBCTL`
    /// (`*ALLOBJ` has both); the commands check the queue's authority
    /// themselves.
    fn sees_all(&self, user: &str) -> bool {
        self.core
            .special(user)
            .iter()
            .any(|s| s == "*SPLCTL" || s == "*JOBCTL" || s == "*ALLOBJ")
    }

    fn data(&self, user: &str, f: &Splf) -> Result<Vec<u8>, Message> {
        let (file, job, nbr) = (f.file.clone(), f.job.clone(), f.number.to_string());
        self.link
            .read(user, move |m: &mut Main| {
                Some(
                    m.ask(|l| l.splf_fetch(file, job, nbr))
                        .map(|x| x.1)
                        .map_err(|e| error(&e)),
                )
            })
            .unwrap_or_else(|| Err(gone()))
    }
}

impl Print for LivePrint {
    fn spooled(&self, user: &str, owner: Option<&str>) -> Result<Vec<Splf>, Message> {
        let all = self.sees_all(user);
        if owner.is_some_and(|o| o != user) && !all {
            let o = owner.unwrap_or_default();
            return Err(Message::new(
                Kind::Error,
                "CPF3303",
                &format!("{user} is not authorized to the spooled files of {o}."),
            ));
        }
        let who = match owner {
            Some(o) => o.to_string(),
            None if all => String::new(),
            None => user.to_string(),
        };
        let mut v: Vec<Splf> = self
            .link
            .read(user, move |m: &mut Main| {
                m.ask(|l| l.splf_list(who, String::new()))
            })
            .iter()
            .map(splf)
            .collect();
        v.sort_by_key(|f| std::cmp::Reverse(f.created));
        Ok(v)
    }

    fn users(&self, user: &str) -> Vec<String> {
        if !self.sees_all(user) {
            return Vec::new();
        }
        let files: Vec<SplfAttrs> = self.link.read(user, |m: &mut Main| {
            m.ask(|l| l.splf_list(String::new(), String::new()))
        });
        let mut users: Vec<String> = files.iter().map(|f| f.job.1.clone()).collect();
        users.sort();
        users.dedup();
        users
    }

    fn queues(&self, user: &str) -> Vec<String> {
        let mut v: Vec<String> = self.link.read(user, |m: &mut Main| {
            m.ask(|l| l.list_objects("*ALL".into(), "*ALL".into(), "*OUTQ".into()))
                .into_iter()
                .map(|o| format!("{}/{}", o.lib, o.name))
                .collect()
        });
        v.sort();
        v
    }

    fn find(&self, user: &str, id: &str) -> Result<Splf, Message> {
        let owner = id.split('-').nth(1).unwrap_or("").to_string();
        self.spooled(user, Some(&owner))?
            .into_iter()
            .find(|f| f.id() == id)
            .ok_or_else(|| {
                Message::new(Kind::Error, "CPF3309", &format!("No spooled file {id}."))
                    .with_data(&[id])
            })
    }

    fn text(&self, user: &str, f: &Splf) -> Result<Vec<String>, Message> {
        let data = self.data(user, f)?;
        let (pages, _) = spool::scs::decode(&data);
        Ok(pages
            .iter()
            .map(|p| spool::view::text(std::slice::from_ref(p)))
            .collect())
    }

    fn pdf(&self, user: &str, f: &Splf) -> Result<Vec<u8>, Message> {
        let data = self.data(user, f)?;
        let (pages, fmt) = spool::scs::decode(&data);
        Ok(spool::pdf::render(&pages, fmt))
    }

    fn act(&self, user: &str, f: &Splf, act: &Act) -> Result<(), Message> {
        let cmd = match act {
            Act::Hold { page_end } => format!(
                "HLDSPLF {} OPTION({})",
                f.parms(),
                if *page_end { "*PAGEEND" } else { "*IMMED" }
            ),
            Act::Release => format!("RLSSPLF {}", f.parms()),
            Act::Delete => format!("DLTSPLF {}", f.parms()),
            Act::Move { top: true, .. } => {
                // BLOCKED(rel): no command or service request a web job may use puts a spooled file at the top of a queue (QAP's move does, on its own connection job only); refused with XWA0099.
                let text = accessweb::msgs::second_level(
                    accessweb::msgs::NOT_AVAILABLE,
                    &["Move to the top of a queue"],
                )
                .map(|s| s.text)
                .unwrap_or_default();
                return Err(
                    Message::new(Kind::Error, accessweb::msgs::NOT_AVAILABLE, &text)
                        .with_data(&["Move to the top of a queue"]),
                );
            }
            Act::Move { outq, .. } => format!("CHGSPLFA {} OUTQ({outq})", f.parms()),
        };
        self.link.run(user, &cmd).map(|_| ())
    }

    fn printers(&self, user: &str) -> Vec<Printer> {
        self.link.read(user, |m: &mut Main| {
            m.ask(Client::wtr_list)
                .into_iter()
                .map(|(name, dev, status, _form, outq, _)| {
                    let device = if dev.is_empty() { name.clone() } else { dev };
                    let d = device.clone();
                    let panel = m.ask(|l| l.devd_panel(d)).unwrap_or_default();
                    let field = |label: &str| {
                        panel
                            .iter()
                            .find(|(k, _)| k.trim().eq_ignore_ascii_case(label))
                            .map(|(_, v)| v.trim().to_string())
                            .unwrap_or_default()
                    };
                    Printer {
                        name: device,
                        started: status != "END",
                        outq,
                        location: field("Text 'description'"),
                        kind: field("Device type"),
                    }
                })
                .collect()
        })
    }
}
