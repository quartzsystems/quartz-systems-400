// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QS/400's own include members for the client, generated from one
//! table (`protos.txt` and [`crate::consts`]): `QSYSINC/H(LDAP)`,
//! `QSYSINC/H(LBER)`, `QSYSINC/H(LDAPSSL)` and the RPG copy member
//! `QSYSINC/QRPGLESRC(LDAP)`. The shipped copies are in
//! `lic/osobjects/shipped/qsysinc/`; a test keeps them current.

use std::fmt::Write as _;

use crate::consts::{Hdr, OIDS, TABLE, Val};

/// The prototype table.
pub const PROTOS: &str = include_str!("protos.txt");

/// One parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    /// Its C type.
    pub ty: String,
    /// Its name.
    pub name: String,
}

impl Param {
    /// Whether C passes it as a pointer.
    #[must_use]
    pub fn is_ptr(&self) -> bool {
        self.ty.contains('*') || self.ty == "LDAPRebindProc"
    }
}

/// One function of the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proto {
    /// Its header.
    pub hdr: Hdr,
    /// Whether QS/400 refuses it.
    pub refused: bool,
    /// The return type.
    pub ret: String,
    /// The name.
    pub name: String,
    /// The parameters.
    pub params: Vec<Param>,
    /// Whether it takes `...`.
    pub varargs: bool,
    /// The prototype as written.
    pub text: String,
}

fn split_name(s: &str) -> (String, String) {
    let s = s.trim();
    let at = s
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map_or(0, |i| i + 1);
    (s[..at].trim().to_string(), s[at..].to_string())
}

/// The table's functions, in order.
///
/// # Panics
///
/// A malformed line (the table is part of the crate).
#[must_use]
pub fn protos() -> Vec<Proto> {
    let mut out = Vec::new();
    for line in PROTOS.lines() {
        let l = line.trim_end();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let hdr = match &l[..1] {
            "L" => Hdr::Ldap,
            "B" => Hdr::Lber,
            "S" => Hdr::Ssl,
            other => panic!("bad header letter {other} in {l}"),
        };
        let refused = &l[1..2] == "-";
        let text = l[2..].trim().to_string();
        let open = text.find('(').expect("a (");
        let (ret, name) = split_name(&text[..open]);
        let inside = &text[open + 1..text.rfind(')').expect("a )")];
        let mut params = Vec::new();
        let mut varargs = false;
        for p in inside.split(',').map(str::trim) {
            match p {
                "" | "void" => {}
                "..." => varargs = true,
                _ => {
                    let (ty, name) = split_name(p);
                    params.push(Param { ty, name });
                }
            }
        }
        out.push(Proto {
            hdr,
            refused,
            ret,
            name,
            params,
            varargs,
            text,
        });
    }
    out
}

/// The entry's argument types a prototype gives (after the three
/// words every procedure takes): `i64` for an integer, `u128` for a
/// pointer, and an `i64` pad where a pointer would start in the last
/// argument register.
#[must_use]
pub fn abi_types(p: &Proto) -> Vec<&'static str> {
    let mut used = 0;
    let mut out = Vec::new();
    for a in &p.params {
        if a.is_ptr() {
            if used == 2 {
                out.push("i64");
                used = 3;
            }
            out.push("u128");
            used += 2;
        } else {
            out.push("i64");
            used += 1;
        }
    }
    out
}

const WIDTH: usize = 80;

/// A prototype folded at commas to fit the source member.
fn fold(text: &str) -> String {
    if text.len() <= WIDTH {
        return format!("{text}\n");
    }
    let mut out = String::new();
    let mut line = String::new();
    for (i, part) in text.split(", ").enumerate() {
        let piece = if i == 0 {
            part.to_string()
        } else {
            format!(", {part}")
        };
        if line.len() + piece.len() + 1 > WIDTH && !line.is_empty() {
            out.push_str(line.trim_end());
            out.push_str(",\n");
            line = format!("    {}", piece.trim_start_matches(", "));
        } else {
            line.push_str(&piece);
        }
    }
    out.push_str(&line);
    out.push('\n');
    out
}

fn defines(out: &mut String, hdr: Hdr) {
    for (h, name, v) in TABLE {
        if *h != hdr {
            continue;
        }
        let Val::Int(n) = v else { continue };
        let value = match *name {
            "LDAP_OPT_ON" | "LDAP_OPT_OFF" => format!("((void *) {n})"),
            _ if *n < 0 => format!("({n})"),
            _ if *name == "LDAP_AUTH_SASL"
                || *name == "LDAP_AUTH_SIMPLE"
                || name.starts_with("LDAP_RES_")
                || name.starts_with("LDAP_SSL_")
                || name.starts_with("LBER_")
                || (*n > 0x50 && name.starts_with("LDAP_") && !name.starts_with("LDAP_OPT")) =>
            {
                format!("0x{n:02X}")
            }
            _ => n.to_string(),
        };
        let _ = writeln!(out, "#define {name:<38} {value}");
    }
    for (h, name, oid) in OIDS {
        if *h == hdr {
            let _ = writeln!(out, "#define {name:<30} \"{oid}\"");
        }
    }
}

fn prototypes(out: &mut String, hdr: Hdr) {
    for p in protos().iter().filter(|p| p.hdr == hdr) {
        if p.refused {
            out.push_str("/* Answers LDAP_NOT_SUPPORTED on QS/400. */\n");
        }
        out.push_str(&fold(&p.text));
    }
}

const BANNER: &str = "/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work */
/* is licensed under the terms of the MIT license which can be found in */
/* the root directory of this project.                                  */
";

/// `QSYSINC/H(LBER)`.
#[must_use]
pub fn lber_h() -> String {
    let mut o = String::from(BANNER);
    o.push_str(
        "/*                                                                      */
/* LBER: the BER encoder and decoder of QS/400's LDAP client (QGLDCLNT). */
/* Generated from runtime/ldapclnt; see docs/DIRSRV.md section 13.      */

#ifndef QS400_LBER_H
#define QS400_LBER_H

/* Tags and lengths are four bytes. */
typedef unsigned long ber_tag_t;
typedef unsigned long ber_len_t;

/* An element being encoded or decoded; its contents are the system's. */
typedef struct berelement BerElement;

/* A counted value: bv_len at offset 0, bv_val at 16. */
struct berval {
    ber_len_t bv_len;
    char *bv_val;
};

#define LBER_ERROR   ((ber_tag_t) -1)
#define LBER_DEFAULT ((ber_tag_t) -1)
",
    );
    defines(&mut o, Hdr::Lber);
    o.push_str(
        "
/* ber_printf and ber_scanf format letters: see docs/DIRSRV.md section 13. */
",
    );
    prototypes(&mut o, Hdr::Lber);
    o.push_str("\n#endif\n");
    o
}

/// `QSYSINC/H(LDAP)`.
#[must_use]
pub fn ldap_h() -> String {
    let mut o = String::from(BANNER);
    o.push_str(
        "/*                                                                      */
/* LDAP: QS/400's LDAP client interface (service program QGLDCLNT).     */
/* Generated from runtime/ldapclnt; see docs/DIRSRV.md section 13.      */
/* Strings are in the job's CCSID unless LDAP_OPT_UTF8_IO is on. Where  */
/* the LDAP C API draft gives a number it is used; the rest are QS/400's. */

#ifndef QS400_LDAP_H
#define QS400_LDAP_H

#include <lber.h>

/* A session and a chain of messages; their contents are the system's. */
typedef struct ldap LDAP;
typedef struct ldapmsg LDAPMessage;
typedef struct ldapfriendlymap LDAPFriendlyMap;

/* A control: OID at 0, value at 16, criticality at 48. */
typedef struct ldapcontrol {
    char *ldctl_oid;
    struct berval ldctl_value;
    char ldctl_iscritical;
} LDAPControl;

/* A change: operation at 0, type at 16, values at 32. */
typedef struct ldapmod {
    int mod_op;
    char *mod_type;
    union {
        char **modv_strvals;
        struct berval **modv_bvals;
    } mod_vals;
} LDAPMod;
#define mod_values  mod_vals.modv_strvals
#define mod_bvalues mod_vals.modv_bvals

/* A sort key: attribute at 0, ordering rule at 16, reverse at 32. */
typedef struct ldapsortkey {
    char *attributeType;
    char *orderingRule;
    int reverseOrder;
} LDAPSortKey;

/* A parsed URL: 112 bytes. */
typedef struct ldap_url_desc {
    char *lud_host;
    int lud_port;
    char *lud_dn;
    char **lud_attrs;
    int lud_scope;
    char *lud_filter;
    char *lud_string;
} LDAPURLDesc;

/* A virtual list view request: attrvalue set asks by value. */
typedef struct ldapvlvinfo {
    int ldvlv_version;
    unsigned long ldvlv_before_count;
    unsigned long ldvlv_after_count;
    unsigned long ldvlv_offset;
    unsigned long ldvlv_count;
    struct berval *ldvlv_attrvalue;
    struct berval *ldvlv_context;
    void *ldvlv_extradata;
} LDAPVLVInfo;

/* ldap_version: the release as VRM, the protocol, the TLS level. */
typedef struct ldapversion {
    int sdk_version;
    int protocol_version;
    int ssl_version;
} LDAPVersion;

/* The rebind callback (not supported on QS/400). */
typedef int (*LDAPRebindProc)(LDAP *ld, char **dnp, char **passwdp,
    int *authmethodp, int freeit);

/* struct timeval is two four-byte fields, tv_sec and tv_usec. */
struct timeval;

",
    );
    defines(&mut o, Hdr::Ldap);
    o.push('\n');
    prototypes(&mut o, Hdr::Ldap);
    o.push_str("\n#include <ldapssl.h>\n\n#endif\n");
    o
}

/// `QSYSINC/H(LDAPSSL)`.
#[must_use]
pub fn ldapssl_h() -> String {
    let mut o = String::from(BANNER);
    o.push_str(
        "/*                                                                      */
/* LDAPSSL: secure connections of QS/400's LDAP client (QGLDCLNT).      */
/* A null key ring is the *SYSTEM certificate store; an application ID  */
/* is a Digital Certificate Manager definition.                         */
/* Generated from runtime/ldapclnt; see docs/DIRSRV.md section 13.      */

#ifndef QS400_LDAPSSL_H
#define QS400_LDAPSSL_H

#include <ldap.h>

",
    );
    defines(&mut o, Hdr::Ssl);
    o.push('\n');
    prototypes(&mut o, Hdr::Ssl);
    o.push_str("\n#endif\n");
    o
}

fn rpg_type(ty: &str) -> &'static str {
    match ty {
        "LDAPRebindProc" => "pointer(*proc) value",
        "const char *" | "char *" => "pointer value options(*string)",
        t if t.contains('*') => "pointer value",
        "unsigned long" | "ber_tag_t" | "ber_len_t" => "uns(10) value",
        "char" | "const char" => "int(3) value",
        _ => "int(10) value",
    }
}

fn rpg_ret(ty: &str) -> &'static str {
    match ty {
        "void" => "",
        t if t.contains('*') => " pointer",
        "unsigned long" | "ber_tag_t" | "ber_len_t" => " uns(10)",
        _ => " int(10)",
    }
}

/// `QSYSINC/QRPGLESRC(LDAP)`: the constants, the structures as
/// templates and every prototype but the two variadic ones.
#[must_use]
pub fn ldap_rpgle() -> String {
    let mut o = String::from(
        "**FREE
// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.
//
// LDAP: QS/400's LDAP client (service program QGLDCLNT) for ILE RPG, a
// QS/400 addition generated from the same table as QSYSINC/H(LDAP).
// Strings are in the job's CCSID unless LDAP_OPT_UTF8_IO is on.
// ber_printf and ber_scanf take a variable list and are not declared.

/if defined(QS400_LDAP_RPGLE)
/eof
/endif
/define QS400_LDAP_RPGLE

",
    );
    for (_, name, v) in TABLE {
        if let Val::Int(n) = v {
            let _ = writeln!(o, "dcl-c {name} {n};");
        }
    }
    let _ = writeln!(o, "dcl-c LBER_ERROR 4294967295;");
    for (_, name, oid) in OIDS {
        let _ = writeln!(o, "dcl-c {name} '{oid}';");
    }
    o.push_str(
        "
dcl-ds berval_t qualified template align;
  bv_len uns(10);
  bv_val pointer;
end-ds;

dcl-ds LDAPControl_t qualified template align;
  ldctl_oid pointer;
  ldctl_value likeds(berval_t);
  ldctl_iscritical char(1);
end-ds;

dcl-ds LDAPMod_t qualified template align;
  mod_op int(10);
  mod_type pointer;
  mod_vals pointer;
end-ds;

dcl-ds LDAPSortKey_t qualified template align;
  attributeType pointer;
  orderingRule pointer;
  reverseOrder int(10);
end-ds;

dcl-ds LDAPURLDesc_t qualified template align;
  lud_host pointer;
  lud_port int(10);
  lud_dn pointer;
  lud_attrs pointer;
  lud_scope int(10);
  lud_filter pointer;
  lud_string pointer;
end-ds;

dcl-ds LDAPVLVInfo_t qualified template align;
  ldvlv_version int(10);
  ldvlv_before_count uns(10);
  ldvlv_after_count uns(10);
  ldvlv_offset uns(10);
  ldvlv_count uns(10);
  ldvlv_attrvalue pointer;
  ldvlv_context pointer;
  ldvlv_extradata pointer;
end-ds;

dcl-ds LDAPVersion_t qualified template;
  sdk_version int(10);
  protocol_version int(10);
  ssl_version int(10);
end-ds;

dcl-ds timeval_t qualified template;
  tv_sec int(10);
  tv_usec int(10);
end-ds;
",
    );
    for p in protos() {
        if p.varargs {
            continue;
        }
        o.push('\n');
        if p.refused {
            o.push_str("// Answers LDAP_NOT_SUPPORTED on QS/400.\n");
        }
        let head = format!("dcl-pr {}{}", p.name, rpg_ret(&p.ret));
        let ext = format!("extproc('{}');", p.name);
        if head.len() + 1 + ext.len() <= WIDTH {
            let _ = writeln!(o, "{head} {ext}");
        } else {
            let _ = writeln!(o, "{head}\n       {ext}");
        }
        for a in &p.params {
            let _ = writeln!(o, "  dcl-parm {} {};", a.name, rpg_type(&a.ty));
        }
        o.push_str("end-pr;\n");
    }
    o
}

/// Every member: source file, member, text, source type, contents.
#[must_use]
pub fn members() -> Vec<(
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    String,
)> {
    vec![
        ("H", "LDAP", "LDAP client interface", "C", ldap_h()),
        ("H", "LBER", "LDAP client BER interface", "C", lber_h()),
        (
            "H",
            "LDAPSSL",
            "LDAP client secure connections",
            "C",
            ldapssl_h(),
        ),
        (
            "QRPGLESRC",
            "LDAP",
            "LDAP client interface for RPG",
            "RPGLE",
            ldap_rpgle(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_parses() {
        let p = protos();
        let init = p.iter().find(|p| p.name == "ldap_init").unwrap();
        assert_eq!(init.ret, "LDAP *");
        assert_eq!(init.params[0].ty, "const char *");
        assert_eq!(init.params[1].name, "port");
        assert_eq!(abi_types(init), ["u128", "i64"]);
        let s = p.iter().find(|p| p.name == "ldap_search").unwrap();
        assert_eq!(
            abi_types(s),
            ["u128", "i64", "u128", "i64", "u128", "u128", "i64"]
        );
        assert!(p.iter().find(|p| p.name == "ber_printf").unwrap().varargs);
        assert!(
            p.iter()
                .find(|p| p.name == "ldap_krb_cc_destroy")
                .unwrap()
                .params
                .is_empty()
        );
    }

    #[test]
    fn members_fit_and_are_quartz_written() {
        for (_, m, _, _, text) in members() {
            for (n, l) in text.lines().enumerate() {
                assert!(l.len() <= WIDTH, "{m} line {}: {} chars", n + 1, l.len());
                assert!(l.chars().all(ccsid::is_representable), "{m} line {}", n + 1);
            }
        }
    }
}
