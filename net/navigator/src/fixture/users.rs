// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sample user and group profiles and authorization lists: the
//! prototype's fifteen users, six groups and five lists, and the
//! security commands acting on them as the system would. The sign-on
//! profiles ([`super::PROFILES`]) give the special authorities of the
//! users both know.

use std::collections::BTreeMap;
use std::sync::Mutex;

use qsweb::msg::Message;

use super::cl::{done, fail, parse, split, unquote};
use super::{PROFILES, has_special};
use crate::env::users::{
    AuthEntry, AuthList, ObjectAuthority, Profile, SPECIAL, SecuredObject, Users,
};
use crate::util::secs;

/// The sample system's users and groups.
pub struct FixtureUsers {
    /// Everything the pages show and the commands change.
    pub state: Mutex<State>,
    /// Commands run, as typed.
    pub ran: Mutex<Vec<String>>,
}

/// Profiles, lists and what the commands made besides.
#[derive(Default)]
pub struct State {
    /// User and group profiles.
    pub profiles: Vec<Profile>,
    /// Authorization lists.
    pub lists: Vec<AuthList>,
    /// Directories `CRTDIR` made, and their owners.
    pub dirs: BTreeMap<String, String>,
    /// Message queues `CRTMSGQ` made.
    pub msgqs: Vec<String>,
}

/// `(name, text, enabled, class, last sign-on, group, supplemental, owned)`.
type Row = (
    &'static str,
    &'static str,
    bool,
    &'static str,
    (u32, u32, u32, u32, u32),
    &'static str,
    &'static str,
    u32,
);

/// The users, in the system's order. Dates are `(month, day, h, m, s)`
/// of 2026.
const USERS: &[Row] = &[
    (
        "QSECOFR",
        "Security officer",
        true,
        "*SECOFR",
        (10, 29, 13, 50, 1),
        "",
        "ADMINS WEBUSERS WAREHOUSE",
        412,
    ),
    (
        "QPGMR",
        "Programmer",
        true,
        "*PGMR",
        (10, 29, 6, 0, 0),
        "",
        "WEBUSERS WAREHOUSE",
        58,
    ),
    (
        "QUSER",
        "Default user for host servers",
        true,
        "*USER",
        (10, 29, 14, 2, 11),
        "",
        "SERVICES WEBUSERS",
        4,
    ),
    (
        "QTMHHTTP",
        "HTTP server user",
        true,
        "*USER",
        (10, 29, 6, 0, 1),
        "",
        "SERVICES WEBUSERS",
        12,
    ),
    (
        "QTCP",
        "TCP/IP",
        true,
        "*USER",
        (10, 27, 6, 0, 0),
        "",
        "",
        31,
    ),
    (
        "QSSHD",
        "SSH daemon",
        true,
        "*USER",
        (10, 27, 6, 0, 2),
        "",
        "SERVICES WEBUSERS",
        3,
    ),
    (
        "TESTUSER",
        "Cody Wellman",
        true,
        "*SECOFR",
        (10, 29, 13, 5, 0),
        "ADMINS",
        "OPS WEBUSERS WAREHOUSE ACCTG",
        37,
    ),
    (
        "OPERATOR",
        "System operator",
        true,
        "*SYSOPR",
        (10, 29, 12, 50, 0),
        "OPS",
        "WEBUSERS WAREHOUSE",
        6,
    ),
    (
        "WEBUSER",
        "Access for Web default user",
        true,
        "*USER",
        (10, 29, 11, 41, 50),
        "WEBUSERS",
        "",
        0,
    ),
    (
        "GUEST",
        "Guest profile",
        false,
        "*USER",
        (10, 12, 9, 20, 17),
        "WEBUSERS",
        "",
        0,
    ),
    (
        "INVSVC",
        "Inventory service account",
        true,
        "*USER",
        (10, 29, 6, 0, 0),
        "SERVICES",
        "WEBUSERS WAREHOUSE",
        22,
    ),
    (
        "BACKUP",
        "Backup operator",
        true,
        "*SYSOPR",
        (10, 28, 23, 30, 0),
        "OPS",
        "WEBUSERS WAREHOUSE ACCTG",
        2,
    ),
    (
        "JDOE",
        "J. Doe - warehouse",
        true,
        "*USER",
        (10, 29, 8, 2, 33),
        "WAREHOUSE",
        "WEBUSERS ACCTG",
        3,
    ),
    (
        "ASMITH",
        "A. Smith - accounting",
        true,
        "*USER",
        (10, 28, 16, 44, 10),
        "ACCTG",
        "WEBUSERS WAREHOUSE",
        5,
    ),
    (
        "TEMP01",
        "Temporary contractor",
        false,
        "*USER",
        (8, 30, 10, 11, 0),
        "",
        "WEBUSERS WAREHOUSE ACCTG",
        0,
    ),
];

/// The groups: name, text, GID.
const GROUPS: &[(&str, &str, u32)] = &[
    ("ADMINS", "System administrators", 1001),
    ("OPS", "Operators", 1002),
    ("WEBUSERS", "Access for Web users", 1010),
    ("SERVICES", "Service accounts", 1020),
    ("WAREHOUSE", "Warehouse staff", 1030),
    ("ACCTG", "Accounting", 1031),
];

/// A profile with the system's defaults.
fn profile(name: &str, text: &str, class: &str) -> Profile {
    Profile {
        name: name.into(),
        text: text.into(),
        enabled: true,
        class: class.into(),
        pwd_changed: secs(2026, 9, 1, 0, 0, 0),
        pwd_interval: "*SYSVAL".into(),
        initial_program: "*NONE".into(),
        initial_menu: "MAIN".into(),
        current_library: "QGPL".into(),
        limit_capabilities: if class == "*USER" { "*YES" } else { "*NO" }.into(),
        jobd: "QGPL/QDFTJOBD".into(),
        outq: "*WRKSTN".into(),
        msgq: format!("QUSRSYS/{name}"),
        home: format!("/home/{name}"),
        language: "ENU".into(),
        country: "US".into(),
        ccsid: "37".into(),
        ..Profile::default()
    }
}

/// The sample profiles: the users, then the groups.
pub fn profiles() -> Vec<Profile> {
    let mut out: Vec<Profile> = USERS
        .iter()
        .map(
            |&(name, text, enabled, class, (mo, d, h, mi, s), group, supp, owned)| {
                let mut p = profile(name, text, class);
                p.enabled = enabled;
                p.last_signon = Some(secs(2026, mo, d, h, mi, s));
                p.group = group.into();
                p.supplemental = supp.split_whitespace().map(ToString::to_string).collect();
                p.owned = owned;
                p.special = match PROFILES.iter().find(|q| q.name == name) {
                    Some(q) => q.special.iter().map(ToString::to_string).collect(),
                    None => match class {
                        "*SECOFR" => SPECIAL.iter().map(ToString::to_string).collect(),
                        "*SYSOPR" => vec!["*JOBCTL".into(), "*SAVSYS".into()],
                        _ => Vec::new(),
                    },
                };
                if name == "GUEST" {
                    p.invalid_signons = 3;
                }
                p
            },
        )
        .collect();
    out.extend(GROUPS.iter().map(|&(name, text, gid)| {
        let mut p = profile(name, text, "*USER");
        p.gid = Some(gid);
        p.is_group = true;
        p.enabled = false;
        p.msgq = "*NONE".into();
        p.home = String::new();
        p
    }));
    out
}

/// The sample authorization lists.
fn lists() -> Vec<AuthList> {
    let e = |user: &str, aut: &str, mgt: bool| AuthEntry {
        user: user.into(),
        authority: aut.into(),
        list_mgt: mgt,
    };
    let l = |name: &str, text: &str, owner: &str, objects: u32, entries: Vec<AuthEntry>| AuthList {
        name: name.into(),
        text: text.into(),
        owner: owner.into(),
        objects,
        entries,
    };
    vec![
        l(
            "INVDATA",
            "Inventory database files",
            "TESTUSER",
            14,
            vec![
                e("TESTUSER", "*ALL", true),
                e("*PUBLIC", "*EXCLUDE", false),
                e("WAREHOUSE", "*CHANGE", false),
                e("INVSVC", "*CHANGE", false),
                e("ACCTG", "*USE", false),
            ],
        ),
        l(
            "WEBCONF",
            "HTTP server configuration",
            "QSECOFR",
            6,
            vec![
                e("QSECOFR", "*ALL", true),
                e("*PUBLIC", "*USE", false),
                e("QTMHHTTP", "*CHANGE", false),
            ],
        ),
        l(
            "PAYROLL",
            "Payroll objects",
            "QSECOFR",
            22,
            vec![
                e("QSECOFR", "*ALL", true),
                e("*PUBLIC", "*EXCLUDE", false),
                e("ACCTG", "*CHANGE", false),
                e("ASMITH", "*ALL", true),
            ],
        ),
        l(
            "QIFS",
            "IFS shared directories",
            "QSECOFR",
            3,
            vec![e("QSECOFR", "*ALL", true), e("*PUBLIC", "*USE", false)],
        ),
        l(
            "SRCLIBS",
            "Source libraries",
            "QPGMR",
            9,
            vec![
                e("QPGMR", "*ALL", true),
                e("*PUBLIC", "*USE", false),
                e("TESTUSER", "*ALL", false),
                e("OPERATOR", "*USE", false),
                e("BACKUP", "*USE", false),
                e("JDOE", "*EXCLUDE", false),
            ],
        ),
    ]
}

impl Default for FixtureUsers {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                profiles: profiles(),
                lists: lists(),
                ..State::default()
            }),
            ran: Mutex::new(Vec::new()),
        }
    }
}

/// A parameter's list of values.
fn values(v: &str) -> Vec<String> {
    v.split_whitespace()
        .map(|s| unquote(s).to_ascii_uppercase())
        .collect()
}

/// `*NONE` as empty.
fn none(v: &str) -> String {
    let v = v.trim().to_ascii_uppercase();
    if v == "*NONE" { String::new() } else { v }
}

/// Whether `n` is a valid object name.
fn valid_name(n: &str) -> bool {
    (1..=10).contains(&n.len())
        && n.starts_with(|c: char| c.is_ascii_uppercase() || "$#@".contains(c))
        && n.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || "$#@_".contains(c))
}

/// Apply `CRTUSRPRF`/`CHGUSRPRF` parameters to `p`.
fn apply(p: &mut Profile, k: &BTreeMap<String, String>) {
    let get = |n: &str| k.get(n).map(String::as_str);
    if let Some(v) = get("TEXT") {
        p.text = unquote(v);
    }
    if let Some(v) = get("STATUS") {
        p.enabled = v == "*ENABLED";
    }
    if let Some(v) = get("USRCLS") {
        p.class = v.into();
    }
    if let Some(v) = get("GRPPRF") {
        p.group = none(v);
    }
    if let Some(v) = get("SUPGRPPRF") {
        p.supplemental = values(v).into_iter().filter(|s| s != "*NONE").collect();
    }
    if let Some(v) = get("PWDEXPITV") {
        p.pwd_interval = v.into();
    }
    if let Some(v) = get("PASSWORD") {
        let _ = v;
        p.pwd_changed = super::now();
    }
    if let Some(v) = get("INLPGM") {
        p.initial_program = v.into();
    }
    if let Some(v) = get("INLMNU") {
        p.initial_menu = v.into();
    }
    if let Some(v) = get("CURLIB") {
        p.current_library = v.into();
    }
    if let Some(v) = get("LMTCPB") {
        p.limit_capabilities = v.into();
    }
    if let Some(v) = get("JOBD") {
        p.jobd = v.into();
    }
    if let Some(v) = get("OUTQ") {
        p.outq = v.into();
    }
    if let Some(v) = get("MSGQ") {
        p.msgq = v.into();
    }
    if let Some(v) = get("HOMEDIR") {
        p.home = unquote(v);
    }
    if let Some(v) = get("SPCAUT") {
        p.special = values(v)
            .into_iter()
            .filter(|s| s.starts_with('*') && s != "*NONE")
            .collect();
    }
    if let Some(v) = get("LANGID") {
        p.language = v.into();
    }
    if let Some(v) = get("CNTRYID") {
        p.country = v.into();
    }
    if let Some(v) = get("CCSID") {
        p.ccsid = v.into();
    }
    if let Some(v) = get("GID") {
        p.gid = v.parse().ok();
    }
}

impl State {
    #[allow(clippy::too_many_lines)]
    fn run(
        &mut self,
        user: &str,
        name: &str,
        k: &BTreeMap<String, String>,
    ) -> Result<Message, Message> {
        let get = |n: &str| {
            k.get(n)
                .map(String::as_str)
                .unwrap_or_default()
                .trim()
                .to_ascii_uppercase()
        };
        let secadm = has_special(user, "*SECADM");
        match name {
            "CRTUSRPRF" | "CHGUSRPRF" | "DLTUSRPRF" if !secadm => fail("CPF2292", &[]),
            "CRTUSRPRF" => {
                let n = get("USRPRF");
                if !valid_name(&n) {
                    return fail("CPF2203", &[&n]);
                }
                if self.profiles.iter().any(|p| p.name == n) {
                    return fail("CPF2214", &[&n]);
                }
                if let Some(s) = values(k.get("SPCAUT").map_or("", String::as_str))
                    .iter()
                    .find(|s| s.starts_with('*') && *s != "*NONE" && !has_special(user, s))
                {
                    let _ = s;
                    return fail("CPF2291", &[]);
                }
                let mut p = profile(&n, "", "*USER");
                p.special = Vec::new();
                p.limit_capabilities = "*NO".into();
                apply(&mut p, k);
                if get("GID") == "*GEN" {
                    p.gid = Some(
                        self.profiles
                            .iter()
                            .filter_map(|q| q.gid)
                            .max()
                            .unwrap_or(1000)
                            + 1,
                    );
                }
                p.is_group = p.gid.is_some();
                self.profiles.push(p);
                done("CPC2213", &[&n])
            }
            "CHGUSRPRF" => {
                let n = get("USRPRF");
                let Some(p) = self.profiles.iter_mut().find(|p| p.name == n) else {
                    return fail("CPF2204", &[&n]);
                };
                if values(k.get("SPCAUT").map_or("", String::as_str))
                    .iter()
                    .any(|s| {
                        s.starts_with('*')
                            && s != "*NONE"
                            && !p.special.contains(s)
                            && !has_special(user, s)
                    })
                {
                    return fail("CPF2291", &[]);
                }
                apply(p, k);
                done("CPC2205", &[&n])
            }
            "DLTUSRPRF" => {
                let n = get("USRPRF");
                let Some(p) = self.profiles.iter().find(|p| p.name == n).cloned() else {
                    return fail("CPF2204", &[&n]);
                };
                if n.starts_with('Q') {
                    return fail("CPF2245", &[&n]);
                }
                let members: Vec<String> = self
                    .profiles
                    .iter()
                    .filter(|m| m.group == n || m.supplemental.contains(&n))
                    .map(|m| m.name.clone())
                    .collect();
                if !members.is_empty() {
                    return fail("CPF2257", &[&n, &members.join(" ")]);
                }
                let opt = values(k.get("OWNOBJOPT").map_or("*NODLT", String::as_str));
                if p.owned > 0 && opt.first().is_none_or(|o| o == "*NODLT") {
                    return fail("CPF2215", &[&n, &p.owned.to_string()]);
                }
                if opt.first().is_some_and(|o| o == "*CHGOWN") {
                    let to = opt.get(1).cloned().unwrap_or_default();
                    let Some(np) = self.profiles.iter_mut().find(|q| q.name == to) else {
                        return fail("CPF2204", &[&to]);
                    };
                    np.owned += p.owned;
                }
                self.profiles.retain(|q| q.name != n);
                for l in &mut self.lists {
                    l.entries.retain(|e| e.user != n);
                }
                done("CPC2191", &[&n, "QSYS", "*USRPRF"])
            }
            "CRTAUTL" => {
                let n = get("AUTL");
                if !valid_name(&n) {
                    return fail("CPF2203", &[&n]);
                }
                if self.lists.iter().any(|l| l.name == n) {
                    return fail("CPF2282", &[&n]);
                }
                self.lists.push(AuthList {
                    name: n.clone(),
                    text: unquote(k.get("TEXT").map_or("", String::as_str)),
                    owner: user.into(),
                    objects: 0,
                    entries: vec![
                        AuthEntry {
                            user: user.into(),
                            authority: "*ALL".into(),
                            list_mgt: true,
                        },
                        AuthEntry {
                            user: "*PUBLIC".into(),
                            authority: get("AUT")
                                .chars()
                                .next()
                                .map_or("*EXCLUDE".into(), |_| get("AUT")),
                            list_mgt: false,
                        },
                    ],
                });
                done("CPC2208", &[&n])
            }
            "DLTAUTL" | "ADDAUTLE" | "CHGAUTLE" | "RMVAUTLE" => {
                let n = get("AUTL");
                let Some(l) = self.lists.iter_mut().find(|l| l.name == n) else {
                    return fail("CPF2105", &[&n, "QSYS", "*AUTL"]);
                };
                let mgr = l.owner == user
                    || has_special(user, "*ALLOBJ")
                    || l.entries.iter().any(|e| e.user == user && e.list_mgt);
                if !mgr {
                    return fail("CPF2207", &[&n, "QSYS", "*AUTL"]);
                }
                if name == "DLTAUTL" {
                    self.lists.retain(|l| l.name != n);
                    return done("CPC2191", &[&n, "QSYS", "*AUTL"]);
                }
                let users = values(k.get("USER").map_or("", String::as_str));
                let aut = values(k.get("AUT").map_or("*USE", String::as_str));
                let level = aut
                    .iter()
                    .find(|a| *a != "*AUTLMGT")
                    .cloned()
                    .unwrap_or_else(|| "*USE".into());
                let mgt = aut.iter().any(|a| a == "*AUTLMGT");
                for u in &users {
                    let at = l.entries.iter().position(|e| e.user == *u);
                    match (name, at) {
                        ("ADDAUTLE", Some(_)) => return fail("CPF2112", &[u]),
                        ("ADDAUTLE", None) => l.entries.push(AuthEntry {
                            user: u.clone(),
                            authority: level.clone(),
                            list_mgt: mgt,
                        }),
                        (_, None) => return fail("CPF2285", &[u, &n]),
                        ("CHGAUTLE", Some(i)) => {
                            l.entries[i].authority.clone_from(&level);
                            l.entries[i].list_mgt = mgt;
                        }
                        (_, Some(i)) => {
                            l.entries.remove(i);
                        }
                    }
                }
                done("CPC2209", &[&n])
            }
            "CRTDIR" | "MKDIR" => {
                let d = unquote(k.get("DIR").map_or("", String::as_str));
                if self.dirs.contains_key(&d) {
                    return fail("CPF2112", &[&d]);
                }
                self.dirs.insert(d.clone(), user.into());
                done("CPCA083", &[&d])
            }
            "CHGOWN" => {
                let d = unquote(k.get("OBJ").map_or("", String::as_str));
                let to = get("NEWOWN");
                match self.dirs.get_mut(&d) {
                    Some(o) => {
                        *o = to;
                        done("CPC2205", &[&d])
                    }
                    None => fail("CPF2105", &[&d, "", "*DIR"]),
                }
            }
            "CRTMSGQ" => {
                let q = get("MSGQ");
                let (lib, qn) = split(&q, "QGPL");
                if self.msgqs.contains(&q) {
                    return fail("CPF2112", &[qn]);
                }
                let (lib, qn) = (lib.to_string(), qn.to_string());
                self.msgqs.push(q);
                done("CPC9801", &[&qn, &lib, "MSGQ"])
            }
            _ => fail("CPF9899", &[name]),
        }
    }
}

impl Users for FixtureUsers {
    fn profiles(&self, _user: &str) -> Vec<Profile> {
        self.state.lock().expect("users").profiles.clone()
    }

    fn authorities(&self, _user: &str, name: &str) -> Vec<ObjectAuthority> {
        let st = self.state.lock().expect("users");
        let mut out: Vec<ObjectAuthority> = st
            .lists
            .iter()
            .filter(|l| l.owner == name)
            .map(|l| ObjectAuthority {
                object: l.name.clone(),
                lib: "QSYS".into(),
                typ: "*AUTL".into(),
                authority: "*ALL".into(),
                owner: true,
            })
            .collect();
        for l in &st.lists {
            if let Some(e) = l.entries.iter().find(|e| e.user == name && l.owner != name) {
                out.push(ObjectAuthority {
                    object: l.name.clone(),
                    lib: "QSYS".into(),
                    typ: "*AUTL".into(),
                    authority: e.authority.clone(),
                    owner: false,
                });
            }
        }
        if let Some(p) = st.profiles.iter().find(|p| p.name == name)
            && !p.msgq.starts_with('*')
        {
            let (lib, q) = split(&p.msgq, "QUSRSYS");
            out.push(ObjectAuthority {
                object: q.into(),
                lib: lib.into(),
                typ: "*MSGQ".into(),
                authority: "*ALL".into(),
                owner: true,
            });
        }
        out
    }

    fn lists(&self, _user: &str) -> Vec<AuthList> {
        self.state.lock().expect("users").lists.clone()
    }

    fn secured(&self, _user: &str, name: &str) -> Vec<SecuredObject> {
        let st = self.state.lock().expect("users");
        let Some(l) = st.lists.iter().find(|l| l.name == name) else {
            return Vec::new();
        };
        let lib = match l.name.as_str() {
            "INVDATA" => "HLINV",
            "WEBCONF" => "QUSRSYS",
            _ => "QGPL",
        };
        (1..=l.objects)
            .map(|i| SecuredObject {
                object: format!("{}{i:02}", &l.name[..l.name.len().min(6)]),
                lib: lib.into(),
                typ: if l.name == "SRCLIBS" { "*LIB" } else { "*FILE" }.into(),
                owner: l.owner.clone(),
            })
            .collect()
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        self.ran.lock().expect("ran").push(command.to_string());
        let (name, k) = parse(command);
        self.state.lock().expect("users").run(user, &name, &k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_and_members() {
        let ps = profiles();
        assert_eq!(ps.iter().filter(|p| !p.is_group).count(), 15);
        let members = |g: &str| {
            ps.iter()
                .filter(|p| p.group == g || p.supplemental.iter().any(|s| s == g))
                .count()
        };
        let counts: Vec<usize> = GROUPS.iter().map(|g| members(g.0)).collect();
        assert_eq!(counts, [2, 3, 14, 4, 9, 5]);
    }
}
