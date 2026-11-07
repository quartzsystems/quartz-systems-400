// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `navigator::env::sw::Software` as the user, over the data the 5250
//! displays read: `GO LICPGM` (`licpgm_panel INSTALLED`, `SFWRSC`,
//! `LICINF`, `MEDIA` of a catalog's device), `WRKIMGCLG`
//! (`panel2_data IMGCLG`), `DSPLICKEY`, `DSPPTF`, `WRKPTFGRP` and
//! `DSPPTFCVR` (`sys2_panel`), the update attributes (data area
//! `QSYS/QSUPDATTR`). The fixes are what the last `QSCHKUPD` run here
//! said; changes are commands (`run`).

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use navigator::env::sw::{
    Fix, ImageCatalog, LicenseKey, Product, Ptf, PtfGroup, PtfStatus, Software, UpdateAttrs,
};
use qsweb::msg::{Kind, Message};
use stg::Client;

use super::Link;
use crate::work::web::bridge::Main;

/// The service a system talks to when `QSCHGUPDA` named none.
const DEFAULT_URL: &str = "https://updates.quartz.systems/qs400";

/// Software.
pub struct LiveSoftware {
    link: Link,
    /// The fixes the last `QSCHKUPD` found.
    fixes: Arc<Mutex<Vec<Fix>>>,
}

impl LiveSoftware {
    /// Over the job's thread.
    pub fn new(link: Link) -> Self {
        Self {
            link,
            fixes: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

fn lp_rows(m: &Main, what: &str, args: &[String]) -> Vec<Vec<String>> {
    let (w, a) = (what.to_string(), args.to_vec());
    m.ask(|l| l.licpgm_panel(w, a)).unwrap_or_default()
}

/// A display's list of `sys2_panel` data: its variables and the rows.
/// A display's variables or one list entry, by variable.
type Vars = HashMap<String, String>;

fn sys2(m: &Main, what: &str, args: &[&str], list: &str) -> Result<(Vars, Vec<Vars>), String> {
    let (w, a) = (
        what.to_string(),
        args.iter().map(|s| (*s).to_string()).collect(),
    );
    let (vars, lists) = m.ask(|l| l.sys2_panel(w, a))?;
    let rows = lists
        .into_iter()
        .filter(|(n, _)| n == list)
        .flat_map(|(_, r)| r)
        .map(|r| r.into_iter().collect())
        .collect();
    Ok((vars.into_iter().collect(), rows))
}

fn get(r: &HashMap<String, String>, k: &str) -> String {
    r.get(k).map(|v| v.trim().to_string()).unwrap_or_default()
}

/// `*COMPATIBLE` as `GO LICPGM` 10 shows it: `Compatible`.
fn status_word(s: &str) -> String {
    let w = s.trim().trim_start_matches('*').to_ascii_lowercase();
    let mut c = w.chars();
    c.next()
        .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
        .unwrap_or_default()
}

/// A usage limit as the pages show it.
fn limit(l: &str) -> String {
    match l.trim() {
        "*NOMAX" => "Unlimited".into(),
        x => x.to_string(),
    }
}

fn products(m: &Main) -> Vec<Product> {
    let libs: HashMap<(String, String), String> = lp_rows(m, "SFWRSC", &[])
        .into_iter()
        .filter(|r| r.len() > 5)
        .map(|r| ((r[0].clone(), r[1].clone()), r[5].clone()))
        .collect();
    let keys: HashMap<String, String> = lp_rows(m, "LICINF", &[])
        .into_iter()
        .filter(|r| r.len() > 4)
        .map(|r| (r[0].clone(), limit(&r[4])))
        .collect();
    lp_rows(m, "INSTALLED", &[])
        .into_iter()
        .filter(|r| r.len() >= 5)
        .map(|r| {
            let (id, option) = (r[0].trim().to_string(), r[1].trim().to_string());
            Product {
                license: if option == "*BASE" {
                    keys.get(&id).cloned().unwrap_or_default()
                } else {
                    String::new()
                },
                library: libs
                    .get(&(id.clone(), option.clone()))
                    .cloned()
                    .unwrap_or_default(),
                installed: true,
                status: status_word(&r[2]),
                text: r[3].trim().to_string(),
                release: r[4].trim().to_string(),
                installed_at: 0,
                id,
                option,
            }
        })
        .collect()
}

fn catalogs(m: &Main) -> Vec<ImageCatalog> {
    let args = vec!["*ALL".to_string(), "*ALL".to_string()];
    let rows = m
        .ask(|l| l.panel2_data("IMGCLG".into(), args))
        .map(|(_, r)| r)
        .unwrap_or_default();
    rows.into_iter()
        .map(|r| {
            let r: HashMap<String, String> = r.into_iter().collect();
            ImageCatalog {
                name: get(&r, "CLG"),
                device: get(&r, "CLGDEV"),
                text: get(&r, "CLGTEXT"),
            }
        })
        .collect()
}

fn catalog_products(m: &Main, name: &str) -> Result<Vec<Product>, Message> {
    let Some(c) = catalogs(m).into_iter().find(|c| c.name == name) else {
        return Err(Message::new(
            Kind::Error,
            "CPF9897",
            &format!("Image catalog {name} was not found."),
        ));
    };
    if c.device.is_empty() {
        return Err(Message::new(
            Kind::Error,
            "CPF9897",
            &format!("Image catalog {name} is not loaded in a virtual device (LODIMGCLG)."),
        ));
    }
    let rows = m
        .ask(|l| l.licpgm_panel("MEDIA".into(), vec![c.device.clone()]))
        .map_err(|e| super::msgs::failure(&e))?;
    let have = products(m);
    let mut seen = BTreeMap::new();
    for r in rows.into_iter().filter(|r| r.len() >= 6) {
        let (id, option) = (r[0].trim().to_string(), r[1].trim().to_string());
        let mine = have.iter().find(|p| p.id == id && p.option == option);
        seen.entry((id.clone(), option.clone()))
            .or_insert_with(|| Product {
                release: r[4].trim().to_string(),
                text: r[5].trim().to_string(),
                installed: mine.is_some(),
                status: mine.map(|p| p.status.clone()).unwrap_or_default(),
                license: mine.map(|p| p.license.clone()).unwrap_or_default(),
                library: mine.map(|p| p.library.clone()).unwrap_or_default(),
                installed_at: 0,
                id,
                option,
            });
    }
    Ok(seen.into_values().collect())
}

fn license_keys(m: &Main) -> Vec<LicenseKey> {
    let Ok((_, rows)) = sys2(m, "LICKEY", &["*ALL", "*ALL", "*ALL", "*ALL"], "KEYS") else {
        return Vec::new();
    };
    rows.iter()
        .map(|r| LicenseKey {
            product: get(r, "PRDID"),
            term: get(r, "LICTRM"),
            feature: get(r, "FEATURE"),
            usage_limit: get(r, "USGLMT"),
            expires: get(r, "EXPDATE"),
            processor_group: get(r, "PRCGRP"),
        })
        .collect()
}

/// `DSPPTF`'s status as the pages' four; `None` for a PTF that is not
/// on the system.
fn ptf_status(s: &str) -> Option<PtfStatus> {
    match s {
        "Temporarily applied" => Some(PtfStatus::Temporary),
        "Permanently applied" => Some(PtfStatus::Permanent),
        "Superseded" => Some(PtfStatus::Superseded),
        "Not loaded" | "Permanently removed" => None,
        _ => Some(PtfStatus::NotApplied),
    }
}

/// Every cover letter, by PTF.
fn letters(m: &Main, select: &str) -> HashMap<String, String> {
    sys2(m, "PTFCVR", &["*ALL", select, "", ""], "CVRS")
        .map(|(_, rows)| {
            rows.iter()
                .map(|r| {
                    (
                        get(r, "PTFID"),
                        r.get("LETTER").cloned().unwrap_or_default(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn ptfs(m: &Main) -> Vec<Ptf> {
    let mut ids: Vec<String> = products(m).into_iter().map(|p| p.id).collect();
    ids.sort();
    ids.dedup();
    let letters = letters(m, "");
    let mut out = Vec::new();
    for id in ids {
        let Ok((vars, rows)) = sys2(m, "PTF", &[&id, ""], "PTFS") else {
            continue;
        };
        let release = get(&vars, "RELEASE");
        for r in rows {
            let ptf = get(&r, "PTFID");
            let Some(status) = ptf_status(&get(&r, "PSTS")) else {
                continue;
            };
            let delayed =
                sys2(m, "PTFD", &[&ptf], "").is_ok_and(|(v, _)| get(&v, "DELAYED") == "*YES");
            let pending = get(&r, "PACT") != "None";
            let text = letters
                .get(&ptf)
                .and_then(|l| {
                    l.lines()
                        .map(str::trim)
                        .find(|l| !l.is_empty())
                        .map(str::to_string)
                })
                .unwrap_or_default();
            out.push(Ptf {
                ipl_action: if pending || (delayed && status == PtfStatus::NotApplied) {
                    "Yes - delayed".into()
                } else {
                    "None".into()
                },
                id: ptf,
                product: id.clone(),
                release: release.clone(),
                status,
                text,
                loaded: 0,
            });
        }
    }
    out
}

fn ptf_groups(m: &Main) -> Vec<PtfGroup> {
    let Ok((_, rows)) = sys2(m, "PTFGRP", &["*ALL"], "GRPS") else {
        return Vec::new();
    };
    let mut out: Vec<PtfGroup> = rows
        .iter()
        .map(|r| PtfGroup {
            name: get(r, "GRPNAME"),
            level: get(r, "GRPLVL").parse().unwrap_or(0),
            status: get(r, "GRPSTS"),
            text: get(r, "GRPTEXT"),
        })
        .collect();
    // The cumulative package first.
    out.sort_by_key(|g| !g.name.contains("-CUM-"));
    out
}

/// `key=value` lines of `QSYS/QSUPDATTR`.
fn update_attrs(m: &Main) -> UpdateAttrs {
    let area = m
        .ask(|l| l.api_dtaara_read("QSYS".into(), "QSUPDATTR".into()))
        .ok();
    let text = area
        .map(|a| ccsid::from_ebcdic(&a.value))
        .unwrap_or_default();
    let kv: HashMap<String, String> = text
        .split(['\n', '\u{85}'])
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let v = |k: &str| kv.get(k).cloned().unwrap_or_default();
    UpdateAttrs {
        url: kv.get("url").cloned().unwrap_or_else(|| DEFAULT_URL.into()),
        proxy: v("proxy"),
        schedule: kv
            .get("schedule")
            .cloned()
            .unwrap_or_else(|| "*NONE".into()),
        time: v("time"),
        last: v("last").parse().unwrap_or(0),
    }
}

/// What `QSCHKUPD` said, as fixes: newer group levels, HIPER PTFs
/// missing and a cumulative package.
fn fixes_of(text: &str, now: u64) -> Vec<Fix> {
    let fix = |id: &str, kind: &str, text: String| Fix {
        id: id.to_string(),
        kind: kind.to_string(),
        product: release::PRODUCT_ID.to_string(),
        released: now,
        size: 0,
        text,
        on_system: String::new(),
    };
    let mut out = Vec::new();
    for l in text.lines().map(str::trim) {
        if let Some(r) = l.strip_prefix("PTF group ") {
            let (name, rest) = r.split_once(':').unwrap_or((r, ""));
            out.push(fix(name.trim(), "PTF group", format!("PTF group:{rest}")));
        } else if let Some(r) = l.strip_prefix("HIPER PTF ") {
            let id = r.split_whitespace().next().unwrap_or_default();
            out.push(fix(id, "PTF", "HIPER PTF not on this system".into()));
        } else if let Some(r) = l.strip_prefix("Cumulative package ") {
            let id = r.split_whitespace().next().unwrap_or_default();
            out.push(fix(
                id,
                "Cumulative package",
                "Cumulative PTF package".into(),
            ));
        }
    }
    out
}

/// The command's name, upper case.
fn verb(command: &str) -> String {
    command
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase()
}

impl Software for LiveSoftware {
    fn products(&self, user: &str) -> Vec<Product> {
        self.link.read(user, |m| products(m))
    }

    fn catalogs(&self, user: &str) -> Vec<ImageCatalog> {
        self.link.read(user, |m| catalogs(m))
    }

    fn catalog_products(&self, user: &str, catalog: &str) -> Result<Vec<Product>, Message> {
        let c = catalog.trim().to_ascii_uppercase();
        self.link
            .read(user, move |m| Some(catalog_products(m, &c)))
            .unwrap_or_else(|| {
                Err(Message::new(
                    Kind::Error,
                    "CPF9897",
                    "Navigator could not read the catalog.",
                ))
            })
    }

    fn license_keys(&self, user: &str) -> Vec<LicenseKey> {
        self.link.read(user, |m| license_keys(m))
    }

    fn ptfs(&self, user: &str) -> Vec<Ptf> {
        self.link.read(user, |m| ptfs(m))
    }

    fn ptf_groups(&self, user: &str) -> Vec<PtfGroup> {
        self.link.read(user, |m| ptf_groups(m))
    }

    fn cover_letter(&self, user: &str, id: &str) -> Option<String> {
        let id = id.trim().to_ascii_uppercase();
        self.link.read(user, move |m| letters(m, &id).remove(&id))
    }

    fn fixes(&self, _user: &str) -> Vec<Fix> {
        self.fixes.lock().map(|f| f.clone()).unwrap_or_default()
    }

    fn update_attrs(&self, user: &str) -> UpdateAttrs {
        self.link.read(user, |m| update_attrs(m))
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        if verb(command) != "QSCHKUPD" {
            return self.link.run(user, command);
        }
        // The check's lines are the fixes until the next check.
        let (u, line) = (user.to_string(), command.to_string());
        let r = self
            .link
            .bridge
            .call(move |m| {
                let now = m.ask(Client::now).unix;
                crate::work::web::run_text_as(m, Some(&u), &line)
                    .map(|(r, text)| (r, fixes_of(&text, now)))
            })
            .unwrap_or_else(|| Err(super::gone()));
        match r {
            Ok((r, found)) => {
                if r.is_ok()
                    && let Ok(mut f) = self.fixes.lock()
                {
                    *f = found;
                }
                r
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::db::utc_secs;
    use super::*;

    #[test]
    fn words() {
        assert_eq!(status_word("*COMPATIBLE"), "Compatible");
        assert_eq!(limit("*NOMAX"), "Unlimited");
        assert_eq!(ptf_status("Loaded"), Some(PtfStatus::NotApplied));
        assert_eq!(ptf_status("Not loaded"), None);
        assert_eq!(verb("qschkupd"), "QSCHKUPD");
    }

    #[test]
    fn check_lines() {
        let now = utc_secs(2026, 11, 1, 0, 0, 0);
        let f = fixes_of(
            "PTF group QS400-CUM-26300: level 2 available, 1 installed\nHIPER PTF QS10500 is not on this system\n\
             Cumulative package QS400-CUM-26300 is available\nCPI3E01 Updates are available",
            now,
        );
        assert_eq!(f.len(), 3);
        assert_eq!(
            (f[0].id.as_str(), f[0].kind.as_str()),
            ("QS400-CUM-26300", "PTF group")
        );
        assert_eq!(f[1].id, "QS10500");
        assert_eq!(f[2].kind, "Cumulative package");
        assert_eq!(f[2].released, now);
    }
}
