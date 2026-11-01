// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sample software: the product directory, two image catalogs, 118 PTFs
//! at cumulative level QS400-CUM-26290 and the update service's last
//! answer. The commands change it as the system would.

use std::sync::Mutex;

use qsweb::msg::{Kind, Message};

use super::text;
use super::work::parse;
use crate::env::sw::{Fix, ImageCatalog, LicenseKey, Product, Ptf, PtfGroup, PtfStatus, Software, UpdateAttrs};
use crate::fixture::PROFILES;
use crate::util::{days, secs};

/// The sample system's software.
pub struct FixtureSoftware {
    /// Everything the pages show and the commands change.
    pub state: Mutex<State>,
    /// Commands run, as typed.
    pub ran: Mutex<Vec<String>>,
}

/// The sample software, changeable by tests.
pub struct State {
    /// The product directory.
    pub products: Vec<Product>,
    /// The PTFs.
    pub ptfs: Vec<Ptf>,
    /// The update attributes.
    pub attrs: UpdateAttrs,
}

impl Default for FixtureSoftware {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                products: products(),
                ptfs: ptfs(),
                attrs: UpdateAttrs {
                    url: "https://update.quartz.systems/qs400".into(),
                    proxy: String::new(),
                    schedule: "*WEEKLY".into(),
                    time: "040000".into(),
                    last: secs(2026, 10, 29, 4, 0, 12),
                },
            }),
            ran: Mutex::new(Vec::new()),
        }
    }
}

/// The product directory: id, option, text, status, license.
const PRODUCTS: &[(&str, &str, &str, &str, &str)] = &[
    ("QS00SS1", "*BASE", "QS/400 Operating System", "Compatible", "Unlimited"),
    ("QS00SS1", "1", "Extended Base Support", "Compatible", ""),
    ("QS00SS1", "3", "Extended Base Directory Support", "Compatible", ""),
    ("QS00SS1", "12", "Host Servers", "Compatible", ""),
    ("QS00SS1", "30", "Integrated File System Shares (SMB)", "Compatible", ""),
    ("QS00SS1", "33", "Portable Application Solutions Environment", "Compatible", ""),
    ("QS00SS1", "34", "Certificate Manager", "Compatible", ""),
    ("QS00TC1", "*BASE", "TCP/IP Connectivity Utilities", "Compatible", "Unlimited"),
    ("QS00DG1", "*BASE", "QS/400 HTTP Server", "Compatible", "Unlimited"),
    ("QS00NV1", "*BASE", "Navigator for QS/400", "Compatible", "Unlimited"),
    ("QS00XW1", "*BASE", "QS/400 Access for Web", "Compatible", "Unlimited"),
    ("QS00WDS", "*BASE", "Development Tools", "Compatible", "Unlimited"),
    ("QS00WDS", "31", "ILE RPG Compiler", "Compatible", ""),
    ("QS00WDS", "51", "ILE COBOL Compiler", "Installed", ""),
    ("QS00WDS", "56", "ILE C Compiler", "Compatible", ""),
    ("QS00ST1", "*BASE", "SQL Development Kit", "Compatible", "Unlimited"),
    ("QS00JV1", "*BASE", "Developer Kit for Java", "Compatible", "Unlimited"),
    ("QS00BR1", "*BASE", "Backup Recovery and Media Services", "Installed", "Unlimited"),
    ("QS00SM1", "*BASE", "Performance Tools", "Compatible", "Unlimited"),
    ("QS00HA1", "*BASE", "High Availability Switchable Resources", "", "Unlimited"),
];

fn products() -> Vec<Product> {
    PRODUCTS
        .iter()
        .map(|(id, opt, text, status, lic)| Product {
            id: (*id).into(),
            option: (*opt).into(),
            release: "V1R0M0".into(),
            text: (*text).into(),
            installed: !status.is_empty(),
            status: (*status).into(),
            license: (*lic).into(),
            library: format!("QS{}", &id[4..]),
            installed_at: if status.is_empty() { 0 } else { secs(2026, 9, 1, 10, 0, 0) },
        })
        .collect()
}

/// The ten PTFs the numbering repeats: product, status, IPL action, text.
const PATTERN: [(&str, PtfStatus, &str, &str); 10] = [
    ("QS00SS1", PtfStatus::Permanent, "None", "Security fix for TLS handshake"),
    ("QS00SS1", PtfStatus::Permanent, "Yes - delayed", "Database: index rebuild after restore"),
    ("QS00DG1", PtfStatus::Temporary, "None", "HTTP server: log rotation at month end"),
    ("QS00TC1", PtfStatus::Temporary, "None", "FTP: passive mode port range"),
    ("QS00SS1", PtfStatus::NotApplied, "Yes - delayed", "Save/restore: SAVACT lock timeout"),
    ("QS00NV1", PtfStatus::Superseded, "None", "Navigator: spooled file PDF viewer"),
    ("QS00XW1", PtfStatus::Permanent, "None", "Access for Web: 5250 field exit key"),
    ("QS00SS1", PtfStatus::Temporary, "Yes - immediate", "Work management: job queue priority"),
    ("QS00WDS", PtfStatus::Permanent, "None", "RPG compiler: %SCANRPL"),
    ("QS00SS1", PtfStatus::NotApplied, "None", "Kernel: IPL time with large ASP"),
];

/// The 118 PTFs: QS10420 every third number, one loaded a day from
/// 10/01/26.
pub fn ptfs() -> Vec<Ptf> {
    let first = days(2026, 10, 1);
    (0..118u32)
        .map(|i| {
            let (p, st, ipl, text) = PATTERN[i as usize % 10];
            Ptf {
                id: format!("QS{}", 10_420 + 3 * i),
                product: p.into(),
                release: "V1R0M0".into(),
                status: st,
                ipl_action: ipl.into(),
                text: text.into(),
                loaded: u64::try_from(first + i64::from(i)).unwrap_or(0) * 86_400 + 9 * 3600,
            }
        })
        .collect()
}

fn msg(kind: Kind, id: &str, data: &[&str]) -> Message {
    let t = text::second_level(id, data).map(|s| s.text).unwrap_or_default();
    Message::new(kind, id, &t).with_data(data)
}

#[allow(clippy::unnecessary_wraps)]
fn done(id: &str, data: &[&str]) -> Result<Message, Message> {
    Ok(msg(Kind::Done, id, data))
}

fn fail(id: &str, data: &[&str]) -> Result<Message, Message> {
    Err(msg(Kind::Error, id, data))
}

fn all_obj(user: &str) -> bool {
    PROFILES
        .iter()
        .find(|p| p.name == user)
        .is_some_and(|p| p.special.contains(&"*ALLOBJ"))
}

impl State {
    #[allow(clippy::too_many_lines)]
    fn run(&mut self, user: &str, command: &str) -> Result<Message, Message> {
        let (name, p) = parse(command);
        let get = |k: &str| p.get(k).map(String::as_str).unwrap_or_default().to_string();
        let changes = !matches!(name.as_str(), "CHKPRDOPT" | "QSCHKUPD" | "DSPPTF" | "DSPLICKEY");
        if changes && !all_obj(user) {
            return fail("CPF2207", &[&name, "QSYS", "*CMD"]);
        }
        match name.as_str() {
            "APYPTF" | "RMVPTF" | "LODPTF" => {
                let product = get("LICPGM");
                let ids: Vec<String> = get("SELECT").split_whitespace().map(ToString::to_string).collect();
                let mut last = None;
                for id in &ids {
                    let Some(f) = self.ptfs.iter_mut().find(|f| f.id == *id && f.product == product) else {
                        return fail("CPF0C1F", &[]);
                    };
                    let (to, id_) = match name.as_str() {
                        "APYPTF" if get("APY") == "*PERM" => (PtfStatus::Permanent, "CPC3606"),
                        "APYPTF" => (PtfStatus::Temporary, "CPC3606"),
                        "RMVPTF" => (PtfStatus::NotApplied, "CPC3608"),
                        _ => (f.status, "CPC3604"),
                    };
                    f.status = to;
                    last = Some(msg(Kind::Done, id_, &[id, &product]));
                }
                last.ok_or_else(|| msg(Kind::Error, "CPF0C1F", &[]))
            }
            "INSPTF" => {
                for f in self.ptfs.iter_mut().filter(|f| f.status == PtfStatus::NotApplied) {
                    f.status = PtfStatus::Temporary;
                }
                done("CPC3606", &["*ALL", "*ALL"])
            }
            "DLTLICPGM" | "RSTLICPGM" => {
                let (id, opt) = (get("LICPGM"), get("OPTION"));
                let opt = if opt.is_empty() { "*BASE".to_string() } else { opt };
                let Some(pr) = self.products.iter_mut().find(|x| x.id == id && x.option == opt) else {
                    return fail("CPF0C1F", &[]);
                };
                if name == "DLTLICPGM" {
                    pr.installed = false;
                    pr.status = String::new();
                    done("CPC3D82", &[&id, &opt])
                } else {
                    pr.installed = true;
                    pr.status = "Installed".into();
                    done("CPC3D81", &[&id, &opt, "V1R0M0"])
                }
            }
            "CHKPRDOPT" => done("CPC0C20", &[&get("PRDID")]),
            "SNDPTFORD" => done("CPZ8C38", &["0000017", "1"]),
            "QSCHKUPD" => {
                self.attrs.last = super::now();
                Ok(msg(Kind::Info, "CPI3E01", &["2"]))
            }
            "QSCHGUPDA" => {
                let unq = |v: String| v.trim_matches('\'').to_string();
                if p.contains_key("URL") {
                    self.attrs.url = unq(get("URL"));
                }
                if p.contains_key("PROXY") {
                    let v = unq(get("PROXY"));
                    self.attrs.proxy = if v == "*NONE" { String::new() } else { v };
                }
                if p.contains_key("SCHEDULE") {
                    self.attrs.schedule = get("SCHEDULE");
                }
                done("CPC3E03", &[])
            }
            _ => fail("CPF0C1F", &[]),
        }
    }
}

impl Software for FixtureSoftware {
    fn products(&self, _user: &str) -> Vec<Product> {
        self.state.lock().expect("state").products.clone()
    }

    fn catalogs(&self, _user: &str) -> Vec<ImageCatalog> {
        vec![
            ImageCatalog {
                name: "QS400V1R0".into(),
                device: "OPTVRT01".into(),
                text: "QS/400 V1R0M0 products".into(),
            },
            ImageCatalog {
                name: "QS400PTF26290".into(),
                device: "OPTVRT02".into(),
                text: "Cumulative PTF package 26290".into(),
            },
        ]
    }

    fn catalog_products(&self, user: &str, catalog: &str) -> Result<Vec<Product>, Message> {
        match catalog {
            "QS400V1R0" => Ok(self.products(user)),
            "QS400PTF26290" => Ok(Vec::new()),
            _ => Err(msg(Kind::Error, "CPF0C1F", &[])),
        }
    }

    fn license_keys(&self, user: &str) -> Vec<LicenseKey> {
        self.products(user)
            .into_iter()
            .filter(|p| !p.license.is_empty())
            .map(|p| LicenseKey {
                product: p.id,
                term: "V1".into(),
                feature: "5050".into(),
                usage_limit: "*NOMAX".into(),
                expires: "*NONE".into(),
                processor_group: "*ANY".into(),
            })
            .collect()
    }

    fn ptfs(&self, _user: &str) -> Vec<Ptf> {
        self.state.lock().expect("state").ptfs.clone()
    }

    fn ptf_groups(&self, _user: &str) -> Vec<PtfGroup> {
        let g = |name: &str, level, status: &str, text: &str| PtfGroup {
            name: name.into(),
            level,
            status: status.into(),
            text: text.into(),
        };
        vec![
            g("QS400-CUM-26290", 26_290, "Installed", "Cumulative PTF package 26290"),
            g("QS99100", 12, "Installed", "HIPER PTFs"),
            g("QS99001", 7, "Installed", "Database group"),
            g("QS99002", 4, "Not installed", "HTTP server group"),
        ]
    }

    fn cover_letter(&self, user: &str, id: &str) -> Option<String> {
        let (text, product) = if let Some(p) = self.ptfs(user).into_iter().find(|p| p.id == id) {
            (p.text, p.product)
        } else {
            let f = self.fixes(user).into_iter().find(|f| f.id == id)?;
            (f.text, f.product)
        };
        Some(format!(
            "PTF/FIX #: {id}\nPRODUCT:   {product}  V1R0M0\n\nDESCRIPTION OF PROBLEM FIXED:\n  {text}\n\nSPECIAL INSTRUCTIONS:\n  None.\n"
        ))
    }

    fn fixes(&self, _user: &str) -> Vec<Fix> {
        let f = |id: &str, kind: &str, product: &str, d: u32, size: u64, text: &str, on: &str| Fix {
            id: id.into(),
            kind: kind.into(),
            product: product.into(),
            released: secs(2026, 10, d, 12, 0, 0),
            size,
            text: text.into(),
            on_system: on.into(),
        };
        vec![
            f("QS10491", "PTF", "QS00SS1", 27, 12 * (1 << 20) / 10, "Security fix for sign-on server host name verification", ""),
            f("QS10494", "PTF", "QS00DG1", 28, 4 * (1 << 20) / 10, "HTTP server: Listen on IPv6 link-local address", ""),
            f("QS400-CUM-26290", "Cumulative package", "QS00SS1", 17, 412 << 20, "Cumulative PTF package 26290", "Applied"),
        ]
    }

    fn update_attrs(&self, _user: &str) -> UpdateAttrs {
        self.state.lock().expect("state").attrs.clone()
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        self.ran.lock().expect("ran").push(command.to_string());
        self.state.lock().expect("state").run(user, command)
    }
}
