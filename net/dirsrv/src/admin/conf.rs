// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The administration questions an instance's `ibmslapd.conf` answers:
//! the properties ([`PROPERTIES`] places each), the suffixes, the
//! indexes (`ibm-slapdIndex: attr:kinds` on the directory's backend
//! entry, kinds `p` presence, `e` equality, `s` substring) and the
//! administrator. Each change edits the text in place
//! ([`SlapdConf::set`]), so its order and comments stay.

use super::{AdminChange, AdminError, AdminInfo, ErrorKind, Index, Properties, Suffix, SuffixKind};
use crate::config::attrs::{self, Rule};
use crate::config::slapd::{CONFIGURATION, MASTER_SERVER};
use crate::config::{ConfigError, SlapdConf, dn_key};

/// The directory's backend entry: user suffixes, paging, sorting, read
/// only, the configured indexes.
pub const DIRECTORY: &str =
    "cn=Directory, cn=RDBM Backends, cn=IBM Directory, cn=Schemas, cn=Configuration";

/// The schema entry (`ibm-slapdSchemaCheck`).
pub const SCHEMAS: &str = "cn=IBM Directory, cn=Schemas, cn=Configuration";

/// The projected backend's entry.
pub const SYSTEM: &str =
    "cn=System, cn=System Backends, cn=IBM Directory, cn=Schemas, cn=Configuration";

const SSL: &str = "cn=SSL, cn=Configuration";
const CONNECTIONS: &str = "cn=Connection Management, cn=Front End, cn=Configuration";
const LOG: &str = "cn=ibmslapd, cn=Log Management, cn=Configuration";
const AUDIT: &str = "cn=Audit, cn=Log Management, cn=Configuration";
const PERSISTENT: &str = "cn=Persistent Search, cn=Configuration";
const PWD_POLICY: &str = "cn=pwdPolicy Admin, cn=configuration";

/// Where each property is: its field, entry and attribute.
pub const PROPERTIES: &[(&str, &str, &str)] = &[
    ("port", CONFIGURATION, "ibm-slapdPort"),
    ("secure_port", SSL, "ibm-slapdSecurePort"),
    ("security", SSL, "ibm-slapdSecurity"),
    ("size_limit", CONFIGURATION, "ibm-slapdSizeLimit"),
    ("time_limit", CONFIGURATION, "ibm-slapdTimeLimit"),
    ("idle_timeout", CONNECTIONS, "ibm-slapdIdleTimeOut"),
    ("write_timeout", CONNECTIONS, "ibm-slapdWriteTimeout"),
    ("allow_anon", CONNECTIONS, "ibm-slapdAllowAnon"),
    ("deref_aliases", CONFIGURATION, "ibm-slapdDerefAliases"),
    ("schema_check", SCHEMAS, "ibm-slapdSchemaCheck"),
    ("pw_encryption", CONFIGURATION, "ibm-slapdPwEncryption"),
    ("read_only", DIRECTORY, "ibm-slapdReadOnly"),
    ("log_options", LOG, "ibm-slapdLogOptions"),
    ("audit", AUDIT, "ibm-audit"),
    (
        "persistent_search",
        PERSISTENT,
        "ibm-slapdEnablePersistentSearch",
    ),
    (
        "max_persistent",
        PERSISTENT,
        "ibm-slapdMaxPersistentSearches",
    ),
    (
        "paged_non_admin",
        DIRECTORY,
        "ibm-slapdPagedResAllowNonAdmin",
    ),
    ("paged_limit", DIRECTORY, "ibm-slapdPagedResLmt"),
    (
        "sort_non_admin",
        DIRECTORY,
        "ibm-slapdSortSrchAllowNonAdmin",
    ),
    ("sort_key_limit", DIRECTORY, "ibm-slapdSortKeyLimit"),
    (
        "changelog_max",
        CONFIGURATION,
        "ibm-slapdChangeLogMaxEntries",
    ),
    ("pwd_policy", PWD_POLICY, "ibm-slapdConfigPwdPolicyOn"),
    ("projected_read_only", SYSTEM, "ibm-slapdOs400UsrprjRead"),
    ("referrals", CONFIGURATION, "ibm-slapdReferral"),
];

fn tf(b: bool) -> String {
    if b { "TRUE" } else { "FALSE" }.into()
}

/// Each property's values as the file writes them, in [`PROPERTIES`]'
/// order. A change log of 0 and no referrals are no values (QS/400's
/// own attributes are absent unless used).
fn values(p: &Properties) -> Vec<Vec<String>> {
    let n = |v: u32| vec![v.to_string()];
    vec![
        vec![p.port.to_string()],
        vec![p.secure_port.to_string()],
        vec![p.security.clone()],
        n(p.size_limit),
        n(p.time_limit),
        n(p.idle_timeout),
        n(p.write_timeout),
        vec![tf(p.allow_anon)],
        vec![p.deref_aliases.clone()],
        vec![p.schema_check.clone()],
        vec![p.pw_encryption.clone()],
        vec![tf(p.read_only)],
        vec![p.log_options.clone()],
        vec![tf(p.audit)],
        vec![tf(p.persistent_search)],
        n(p.max_persistent),
        vec![tf(p.paged_non_admin)],
        n(p.paged_limit),
        vec![tf(p.sort_non_admin)],
        n(p.sort_key_limit),
        if p.changelog_max == 0 {
            Vec::new()
        } else {
            n(p.changelog_max)
        },
        vec![tf(p.pwd_policy)],
        vec![tf(p.projected_read_only)],
        p.referrals.clone(),
    ]
}

/// The properties `c` gives; an attribute it lacks, or one whose value
/// does not read, is IBM's default.
#[must_use]
pub fn properties(c: &SlapdConf) -> Properties {
    let d = Properties::default();
    let get = |i: usize| {
        let (_, e, a) = PROPERTIES[i];
        c.get(e, a).map(|v| v.trim().to_string())
    };
    let num = |i: usize, dflt: u32| get(i).and_then(|v| v.parse().ok()).unwrap_or(dflt);
    let port = |i: usize, dflt: u16| get(i).and_then(|v| v.parse().ok()).unwrap_or(dflt);
    let flag = |i: usize, dflt: bool| get(i).map_or(dflt, |v| v.eq_ignore_ascii_case("true"));
    let text = |i: usize, dflt: &str| get(i).unwrap_or_else(|| dflt.to_string());
    Properties {
        port: port(0, d.port),
        secure_port: port(1, d.secure_port),
        security: text(2, &d.security),
        size_limit: num(3, d.size_limit),
        time_limit: num(4, d.time_limit),
        idle_timeout: num(5, d.idle_timeout),
        write_timeout: num(6, d.write_timeout),
        allow_anon: flag(7, d.allow_anon),
        deref_aliases: text(8, &d.deref_aliases),
        schema_check: text(9, &d.schema_check),
        pw_encryption: text(10, &d.pw_encryption),
        read_only: flag(11, d.read_only),
        log_options: text(12, &d.log_options),
        audit: flag(13, d.audit),
        persistent_search: flag(14, d.persistent_search),
        max_persistent: num(15, d.max_persistent),
        paged_non_admin: flag(16, d.paged_non_admin),
        paged_limit: num(17, d.paged_limit),
        sort_non_admin: flag(18, d.sort_non_admin),
        sort_key_limit: num(19, d.sort_key_limit),
        changelog_max: num(20, 0),
        pwd_policy: flag(21, d.pwd_policy),
        projected_read_only: flag(22, d.projected_read_only),
        referrals: {
            let (_, e, a) = PROPERTIES[23];
            c.values(e, a)
        },
    }
}

/// Write the properties of `to` that differ from what `c` holds; the
/// attributes written. Nothing is written when one value is refused.
///
/// # Errors
/// `GLD0232` for a value the attribute does not take (a referral that
/// is not an LDAP URL too); `GLD0126` when the file lacks the entry.
pub fn set_properties(c: &mut SlapdConf, to: &Properties) -> Result<Vec<String>, AdminError> {
    let now = values(&properties(c));
    let want = values(to);
    let mut changed = Vec::new();
    for (i, (was, is)) in now.iter().zip(&want).enumerate() {
        if was == is {
            continue;
        }
        let (_, _, attr) = PROPERTIES[i];
        for v in is {
            let ok = match attrs::rule(attr) {
                Some(Rule::Honoured(check)) => attrs::passes(check, v),
                _ => false,
            } && (attr != "ibm-slapdReferral" || ldap::url::LdapUrl::parse(v).is_ok());
            if !ok {
                return Err(ConfigError::BadValue {
                    attr: attr.into(),
                    value: v.clone(),
                }
                .into());
            }
        }
        changed.push(i);
    }
    let mut next = c.clone();
    for &i in &changed {
        let (_, entry, attr) = PROPERTIES[i];
        let vals: Vec<&str> = want[i].iter().map(String::as_str).collect();
        next.set(entry, attr, &vals)?;
    }
    *c = next;
    Ok(changed
        .iter()
        .map(|&i| PROPERTIES[i].2.to_string())
        .collect())
}

fn kind_of(entry: &str, dn: &str) -> SuffixKind {
    let cn = entry
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match cn.as_str() {
        "cn=system" => SuffixKind::Projected,
        "cn=schemadb" => SuffixKind::Schema,
        "cn=configdb" => SuffixKind::Config,
        _ if matches!(dn_key(dn).as_str(), "cn=localhost" | "cn=ibmpolicies") => SuffixKind::Local,
        _ => SuffixKind::User,
    }
}

/// Every suffix, in the file's order (`entries` unknown).
#[must_use]
pub fn suffixes(c: &SlapdConf) -> Vec<Suffix> {
    let mut out = Vec::new();
    for (entry, attrs) in c.entries() {
        for (t, v) in attrs {
            if t.eq_ignore_ascii_case("ibm-slapdSuffix") {
                let dn = v.trim().to_string();
                out.push(Suffix {
                    kind: kind_of(entry, &dn),
                    dn,
                    entries: None,
                });
            }
        }
    }
    out
}

/// Whether DN key `a` is `b` or below it.
fn within(a: &str, b: &str) -> bool {
    a == b || a.ends_with(&format!(",{b}"))
}

/// Add user suffix `dn` to the directory's backend.
///
/// # Errors
/// `GLD0224` for a DN that is not one (or the root); `CPF9898` when it
/// is a suffix already, or within or above another suffix.
pub fn add_suffix(c: &mut SlapdConf, dn: &str) -> Result<(), AdminError> {
    let dn = dn.trim();
    if ldap::Dn::parse(dn).map_or(true, |d| d.is_root()) {
        return Err(ConfigError::BadDn(dn.into()).into());
    }
    let key = dn_key(dn);
    if let Some(s) = suffixes(c)
        .iter()
        .find(|s| within(&key, &dn_key(&s.dn)) || within(&dn_key(&s.dn), &key))
    {
        let (kind, text) = if dn_key(&s.dn) == key {
            (
                ErrorKind::Exists,
                format!("Suffix {} is already configured.", s.dn),
            )
        } else {
            (
                ErrorKind::Invalid,
                format!("Suffix {dn} overlaps suffix {}.", s.dn),
            )
        };
        return Err(AdminError::plain(kind, &text));
    }
    let mut all = c.values(DIRECTORY, "ibm-slapdSuffix");
    all.push(dn.into());
    let v: Vec<&str> = all.iter().map(String::as_str).collect();
    c.set(DIRECTORY, "ibm-slapdSuffix", &v)?;
    Ok(())
}

/// Remove user suffix `dn`.
///
/// # Errors
/// `CPF9898` when no user suffix is `dn`.
pub fn remove_suffix(c: &mut SlapdConf, dn: &str) -> Result<(), AdminError> {
    let key = dn_key(dn);
    match suffixes(c).iter().find(|s| dn_key(&s.dn) == key) {
        Some(s) if s.kind == SuffixKind::User => {}
        Some(s) => {
            return Err(AdminError::plain(
                ErrorKind::Invalid,
                &format!("Suffix {} is the server's own and cannot be removed.", s.dn),
            ));
        }
        None => {
            return Err(AdminError::plain(
                ErrorKind::NotFound,
                &format!("Suffix {} is not configured.", dn.trim()),
            ));
        }
    }
    let keep: Vec<String> = c
        .values(DIRECTORY, "ibm-slapdSuffix")
        .into_iter()
        .filter(|v| dn_key(v) != key)
        .collect();
    let v: Vec<&str> = keep.iter().map(String::as_str).collect();
    c.set(DIRECTORY, "ibm-slapdSuffix", &v)?;
    Ok(())
}

/// The indexes every entry store has (docs/DIRSRV.md §5.2 `IndexDef`).
#[must_use]
pub fn always() -> Vec<Index> {
    let ix = |attr: &str, substring: bool| Index {
        attr: attr.into(),
        presence: true,
        equality: true,
        substring,
        always: true,
    };
    vec![
        ix("objectClass", false),
        ix("cn", true),
        ix("uid", true),
        ix("sn", true),
        ix("mail", true),
        ix("entryUUID", false),
    ]
}

/// An `ibm-slapdIndex` value read; `None` when it does not.
#[must_use]
pub fn parse_index(v: &str) -> Option<Index> {
    let (attr, kinds) = v.trim().split_once(':')?;
    let attr = attr.trim();
    let kinds = kinds.trim().to_ascii_lowercase();
    if attr.is_empty() || kinds.is_empty() || kinds.chars().any(|k| !"pes".contains(k)) {
        return None;
    }
    Some(Index {
        attr: attr.into(),
        presence: kinds.contains('p'),
        equality: kinds.contains('e'),
        substring: kinds.contains('s'),
        always: false,
    })
}

/// An index as its `ibm-slapdIndex` value.
#[must_use]
pub fn print_index(i: &Index) -> String {
    let mut k = String::new();
    for (on, c) in [(i.presence, 'p'), (i.equality, 'e'), (i.substring, 's')] {
        if on {
            k.push(c);
        }
    }
    format!("{}:{k}", i.attr)
}

/// The indexes: [`always`], then the configured ones.
#[must_use]
pub fn indexes(c: &SlapdConf) -> Vec<Index> {
    let mut out = always();
    out.extend(
        c.values(DIRECTORY, "ibm-slapdIndex")
            .iter()
            .filter_map(|v| parse_index(v)),
    );
    out
}

/// Add or change configured index `i`.
///
/// # Errors
/// `CPF9898` for one of [`always`] or an index of no kind.
pub fn put_index(c: &mut SlapdConf, i: &Index) -> Result<(), AdminError> {
    let attr = i.attr.trim();
    if always().iter().any(|a| a.attr.eq_ignore_ascii_case(attr)) {
        return Err(AdminError::plain(
            ErrorKind::Invalid,
            &format!("Attribute {attr} is always indexed."),
        ));
    }
    if attr.is_empty() || !(i.presence || i.equality || i.substring) {
        return Err(AdminError::plain(
            ErrorKind::Invalid,
            "An index needs an attribute and at least one kind of key.",
        ));
    }
    let new = Index {
        attr: attr.into(),
        always: false,
        ..i.clone()
    };
    let mut vals: Vec<String> = c
        .values(DIRECTORY, "ibm-slapdIndex")
        .into_iter()
        .filter(|v| parse_index(v).is_none_or(|x| !x.attr.eq_ignore_ascii_case(attr)))
        .collect();
    vals.push(print_index(&new));
    let v: Vec<&str> = vals.iter().map(String::as_str).collect();
    c.set(DIRECTORY, "ibm-slapdIndex", &v)?;
    Ok(())
}

/// Remove configured index `attr`.
///
/// # Errors
/// `CPF9898` for one of [`always`] or one not configured.
pub fn remove_index(c: &mut SlapdConf, attr: &str) -> Result<(), AdminError> {
    let attr = attr.trim();
    if always().iter().any(|a| a.attr.eq_ignore_ascii_case(attr)) {
        return Err(AdminError::plain(
            ErrorKind::Invalid,
            &format!("Attribute {attr} is always indexed."),
        ));
    }
    let all = c.values(DIRECTORY, "ibm-slapdIndex");
    let keep: Vec<String> = all
        .iter()
        .filter(|v| parse_index(v).is_none_or(|x| !x.attr.eq_ignore_ascii_case(attr)))
        .cloned()
        .collect();
    if keep.len() == all.len() {
        return Err(AdminError::plain(
            ErrorKind::NotFound,
            &format!("Attribute {attr} has no index."),
        ));
    }
    let v: Vec<&str> = keep.iter().map(String::as_str).collect();
    c.set(DIRECTORY, "ibm-slapdIndex", &v)?;
    Ok(())
}

/// The administrator and the update DN.
#[must_use]
pub fn admin(c: &SlapdConf) -> AdminInfo {
    AdminInfo {
        dn: c.admin_dn(),
        password_set: c.admin_pw().is_some(),
        update_dn: c.update_dn(),
    }
}

/// `CHGDIRSVRA DN` on `c`: the password kept as `{SSHA512}`.
///
/// # Errors
/// `GLD0202` for an empty password, `GLD0224`, `GLD0215`.
pub fn set_admin(
    c: &mut SlapdConf,
    instance: &str,
    change: &AdminChange,
    password: &str,
) -> Result<(), AdminError> {
    if password.is_empty() {
        return Err(ConfigError::NoPassword.into());
    }
    let stored = crate::pwd::hash(password.as_bytes());
    match change {
        AdminChange::Password => c.set_admin(None, &stored),
        AdminChange::Dn { dn } => c.set_admin(Some(dn), &stored),
        AdminChange::UpdatePassword => c.set_update_pw(instance, &stored),
    }?;
    Ok(())
}

/// The update DN's entry, for a caller that names one.
pub const UPDATE_ENTRY: &str = MASTER_SERVER;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::shipped;

    fn conf() -> SlapdConf {
        SlapdConf::parse("f", &shipped::slapd_conf("QUSRDIR", "SYSTEM")).unwrap()
    }

    #[test]
    fn properties_read_and_change_in_place() {
        let mut c = conf();
        let p = properties(&c);
        assert_eq!(p, Properties::default());
        assert!(set_properties(&mut c, &p).unwrap().is_empty());
        let to = Properties {
            size_limit: 1000,
            allow_anon: false,
            changelog_max: 5000,
            referrals: vec!["ldap://other.example:389".into()],
            ..p.clone()
        };
        assert_eq!(
            set_properties(&mut c, &to).unwrap(),
            [
                "ibm-slapdSizeLimit",
                "ibm-slapdAllowAnon",
                "ibm-slapdChangeLogMaxEntries",
                "ibm-slapdReferral"
            ]
        );
        assert_eq!(properties(&c), to);
        assert!(c.text().contains("ibm-slapdAllowAnon: FALSE"));
        assert_eq!(c.validate("QUSRDIR"), Ok(()));
        // A value not taken changes nothing.
        let bad = Properties {
            pw_encryption: "aes256".into(),
            size_limit: 7,
            ..to.clone()
        };
        let e = set_properties(&mut c, &bad).unwrap_err();
        assert_eq!(e.msgid, "GLD0232");
        assert_eq!(properties(&c), to);
        let bad = Properties {
            referrals: vec!["not a url".into()],
            ..to
        };
        assert_eq!(set_properties(&mut c, &bad).unwrap_err().msgid, "GLD0232");
    }

    #[test]
    fn suffixes_and_indexes() {
        let mut c = conf();
        let kinds: Vec<SuffixKind> = suffixes(&c).iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            [
                SuffixKind::Config,
                SuffixKind::Schema,
                SuffixKind::Local,
                SuffixKind::Local,
                SuffixKind::Projected
            ]
        );
        add_suffix(&mut c, "dc=example,dc=com").unwrap();
        assert_eq!(
            add_suffix(&mut c, "DC=EXAMPLE, DC=COM").unwrap_err().kind,
            ErrorKind::Exists
        );
        assert_eq!(
            add_suffix(&mut c, "ou=a,dc=example,dc=com")
                .unwrap_err()
                .kind,
            ErrorKind::Invalid
        );
        assert_eq!(add_suffix(&mut c, "nonsense").unwrap_err().msgid, "GLD0224");
        assert!(
            suffixes(&c)
                .iter()
                .any(|s| s.dn == "dc=example,dc=com" && s.kind == SuffixKind::User)
        );
        assert_eq!(
            remove_suffix(&mut c, "cn=localhost").unwrap_err().kind,
            ErrorKind::Invalid
        );
        remove_suffix(&mut c, "dc=example, dc=com").unwrap();
        assert_eq!(suffixes(&c).len(), 5);
        assert_eq!(c.validate("QUSRDIR"), Ok(()));

        assert_eq!(indexes(&c).len(), 6);
        let tel = Index {
            attr: "telephoneNumber".into(),
            presence: true,
            equality: true,
            ..Index::default()
        };
        put_index(&mut c, &tel).unwrap();
        assert!(c.text().contains("ibm-slapdIndex: telephoneNumber:pe"));
        put_index(
            &mut c,
            &Index {
                substring: true,
                ..tel
            },
        )
        .unwrap();
        assert_eq!(indexes(&c).len(), 7);
        assert!(indexes(&c)[6].substring);
        assert!(put_index(&mut c, &always()[1]).is_err());
        remove_index(&mut c, "TELEPHONENUMBER").unwrap();
        assert_eq!(
            remove_index(&mut c, "telephoneNumber").unwrap_err().kind,
            ErrorKind::NotFound
        );
        assert_eq!(parse_index("x:q"), None);
    }

    #[test]
    fn the_administrator_as_chgdirsvra_changes_it() {
        let mut c = conf();
        assert_eq!(
            admin(&c),
            AdminInfo {
                dn: "cn=Administrator".into(),
                password_set: false,
                update_dn: None
            }
        );
        assert_eq!(
            set_admin(&mut c, "QUSRDIR", &AdminChange::Password, "")
                .unwrap_err()
                .msgid,
            "GLD0202"
        );
        set_admin(&mut c, "QUSRDIR", &AdminChange::Password, "secret").unwrap();
        assert!(crate::pwd::verify(c.admin_pw().unwrap().as_bytes(), b"secret"));
        assert_eq!(
            set_admin(
                &mut c,
                "QUSRDIR",
                &AdminChange::Dn { dn: "bad".into() },
                "x"
            )
            .unwrap_err()
            .msgid,
            "GLD0224"
        );
        assert_eq!(
            set_admin(&mut c, "QUSRDIR", &AdminChange::UpdatePassword, "x")
                .unwrap_err()
                .msgid,
            "GLD0215"
        );
    }
}
