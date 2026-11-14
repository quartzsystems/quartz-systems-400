// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a profile or a printer device is as a directory entry
//! (docs/DIRSRV.md §10.1): `inetOrgPerson` for users, `printerService`
//! with `printerIPP` for printers, both tagged `publisher`.

use ldap::dn::{Ava, Dn, Rdn};
use ldap::msg::{Attribute, ModOp, Modification};

use super::config::Kind;

/// A user profile as the publisher needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserInfo {
    /// The profile name.
    pub name: String,
    /// Its text (`TEXT`), blank when none.
    pub text: String,
    /// Its SMTP address, when the system has one for it.
    pub mail: Option<String>,
}

/// A printer device as the publisher needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrinterInfo {
    /// The device name.
    pub device: String,
    /// Its text, blank when none.
    pub text: String,
    /// Its output queue, `LIB/NAME`.
    pub outq: String,
    /// A LAN printer's manufacturer type and model.
    pub make_model: Option<String>,
    /// Where the system takes print for it (`lpd://HOST/OUTQ`).
    pub uri: Option<String>,
}

/// A thing published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Object {
    /// A user profile.
    User(UserInfo),
    /// A printer device.
    Printer(PrinterInfo),
}

impl Object {
    /// Which agent publishes it.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        match self {
            Self::User(_) => Kind::Users,
            Self::Printer(_) => Kind::Printers,
        }
    }

    /// Its name: the profile or the device.
    #[must_use]
    pub fn key(&self) -> &str {
        match self {
            Self::User(u) => &u.name,
            Self::Printer(p) => &p.device,
        }
    }
}

/// The entry an object should be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wanted {
    /// The object's name, the value of [`Kind::key_attr`].
    pub key: String,
    /// The RDN it is published under.
    pub rdn: Rdn,
    /// The RDN used when another entry holds [`Wanted::rdn`].
    pub fallback: Rdn,
    /// Its object classes.
    pub classes: Vec<&'static str>,
    /// The attributes the publisher keeps, each with its values (none:
    /// the entry should not have it).
    pub attrs: Vec<Attribute>,
}

impl Wanted {
    /// The entry's DN under `parent`, with the fallback RDN if asked.
    #[must_use]
    pub fn dn(&self, parent: &Dn, fallback: bool) -> String {
        let rdn = if fallback { &self.fallback } else { &self.rdn };
        parent.child(rdn.clone()).to_string()
    }

    /// The attributes an add sends: the classes and every attribute
    /// with values.
    #[must_use]
    pub fn add_attrs(&self) -> Vec<Attribute> {
        let mut out = vec![Attribute::new("objectClass", &self.classes)];
        out.extend(self.attrs.iter().filter(|a| !a.vals.is_empty()).cloned());
        out
    }

    /// The changes a modify sends: each kept attribute replaced (an
    /// empty replace removes it).
    #[must_use]
    pub fn changes(&self) -> Vec<Modification> {
        self.attrs
            .iter()
            .map(|a| Modification {
                op: ModOp::Replace,
                attr: a.clone(),
            })
            .collect()
    }

    /// The value of `ty`, if it has one.
    #[must_use]
    pub fn value(&self, ty: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.ty.eq_ignore_ascii_case(ty))
            .and_then(|a| a.vals.first())
            .and_then(|v| std::str::from_utf8(v).ok())
    }
}

fn one(ty: &str, v: &str) -> Attribute {
    Attribute::new(ty, &[v])
}

fn maybe(ty: &str, v: Option<&str>) -> Attribute {
    match v.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => one(ty, s),
        None => Attribute::new(ty, &[]),
    }
}

/// The classes of a published user.
pub const USER_CLASSES: &[&str] = &[
    "top",
    "person",
    "organizationalPerson",
    "inetOrgPerson",
    "publisher",
];

/// The classes of a published printer.
pub const PRINTER_CLASSES: &[&str] = &["top", "printerService", "printerIPP", "publisher"];

/// The entry `obj` should be, tagged with `publisher`.
#[must_use]
pub fn wanted(obj: &Object, publisher: &str) -> Wanted {
    match obj {
        Object::User(u) => user(u, publisher),
        Object::Printer(p) => printer(p, publisher),
    }
}

fn user(u: &UserInfo, publisher: &str) -> Wanted {
    let name = u.name.trim();
    let text = u.text.trim();
    let cn = if text.is_empty() { name } else { text };
    let sn = text.split_whitespace().last().unwrap_or(name);
    Wanted {
        key: name.to_string(),
        rdn: Rdn::single("cn", cn),
        fallback: Rdn(vec![Ava::new("cn", cn), Ava::new("uid", name)]),
        classes: USER_CLASSES.to_vec(),
        attrs: vec![
            one("cn", cn),
            one("sn", sn),
            one("uid", name),
            maybe("mail", u.mail.as_deref()),
            one("publisherName", publisher),
        ],
    }
}

fn printer(p: &PrinterInfo, publisher: &str) -> Wanted {
    let dev = p.device.trim();
    let rdn = Rdn::single("printer-name", dev);
    Wanted {
        key: dev.to_string(),
        rdn: rdn.clone(),
        fallback: rdn,
        classes: PRINTER_CLASSES.to_vec(),
        attrs: vec![
            one("printer-name", dev),
            maybe("printer-location", Some(&p.text)),
            maybe("printer-make-and-model", p.make_model.as_deref()),
            maybe("printer-uri", p.uri.as_deref()),
            one("publisherName", publisher),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vals(w: &Wanted, ty: &str) -> Vec<String> {
        w.attrs
            .iter()
            .find(|a| a.ty == ty)
            .map(|a| {
                a.vals
                    .iter()
                    .map(|v| String::from_utf8(v.clone()).unwrap())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn a_user_with_text() {
        let w = wanted(
            &Object::User(UserInfo {
                name: "JSMITH".into(),
                text: "  Jane Q. Smith ".into(),
                mail: Some("jane@example.com".into()),
            }),
            "QS400 QSHQ400",
        );
        let parent = Dn::parse("ou=Users,dc=example,dc=com").unwrap();
        assert_eq!(
            w.dn(&parent, false),
            "cn=Jane Q. Smith,ou=Users,dc=example,dc=com"
        );
        assert_eq!(
            w.dn(&parent, true),
            "cn=Jane Q. Smith+uid=JSMITH,ou=Users,dc=example,dc=com"
        );
        assert_eq!(vals(&w, "sn"), ["Smith"]);
        assert_eq!(vals(&w, "uid"), ["JSMITH"]);
        assert_eq!(vals(&w, "mail"), ["jane@example.com"]);
        assert_eq!(vals(&w, "publisherName"), ["QS400 QSHQ400"]);
        let add = w.add_attrs();
        assert_eq!(add[0].ty, "objectClass");
        assert_eq!(add[0].vals.len(), USER_CLASSES.len());
        assert_eq!(add.len(), 6);
    }

    #[test]
    fn a_user_without_text_or_mail() {
        let w = wanted(
            &Object::User(UserInfo {
                name: "BOB".into(),
                text: String::new(),
                mail: None,
            }),
            "P",
        );
        assert_eq!(vals(&w, "cn"), ["BOB"]);
        assert_eq!(vals(&w, "sn"), ["BOB"]);
        // Mail is not added, and a modify removes one.
        assert!(w.add_attrs().iter().all(|a| a.ty != "mail"));
        let m = w.changes();
        let mail = m.iter().find(|m| m.attr.ty == "mail").unwrap();
        assert_eq!(mail.op, ModOp::Replace);
        assert!(mail.attr.vals.is_empty());
    }

    #[test]
    fn text_with_specials_is_escaped_in_the_dn() {
        let w = wanted(
            &Object::User(UserInfo {
                name: "ACCT".into(),
                text: "Smith, Jones + Co".into(),
                mail: None,
            }),
            "P",
        );
        let parent = Dn::parse("ou=Users,o=x").unwrap();
        let dn = w.dn(&parent, false);
        assert_eq!(dn, "cn=Smith\\, Jones \\+ Co,ou=Users,o=x");
        let back = Dn::parse(&dn).unwrap();
        assert_eq!(
            back.rdn().unwrap().0[0].value_str(),
            Some("Smith, Jones + Co")
        );
    }

    #[test]
    fn a_printer() {
        let w = wanted(
            &Object::Printer(PrinterInfo {
                device: "PRT01".into(),
                text: "Second floor".into(),
                outq: "QUSRSYS/PRT01".into(),
                make_model: Some("*HP4".into()),
                uri: Some("lpd://qs.example.com/PRT01".into()),
            }),
            "P",
        );
        let parent = Dn::parse("ou=Printers,o=x").unwrap();
        assert_eq!(w.dn(&parent, false), "printer-name=PRT01,ou=Printers,o=x");
        assert_eq!(w.key, "PRT01");
        assert_eq!(vals(&w, "printer-location"), ["Second floor"]);
        assert_eq!(vals(&w, "printer-uri"), ["lpd://qs.example.com/PRT01"]);
        assert_eq!(vals(&w, "printer-make-and-model"), ["*HP4"]);
        assert_eq!(w.add_attrs()[0].vals.len(), PRINTER_CLASSES.len());
    }

    #[test]
    fn a_printer_without_text_or_address() {
        let w = wanted(
            &Object::Printer(PrinterInfo {
                device: "PRT02".into(),
                text: "  ".into(),
                outq: "QUSRSYS/PRT02".into(),
                make_model: None,
                uri: None,
            }),
            "P",
        );
        let add = w.add_attrs();
        assert!(
            add.iter()
                .all(|a| a.ty != "printer-location" && a.ty != "printer-uri")
        );
        assert_eq!(add.len(), 3);
    }
}
