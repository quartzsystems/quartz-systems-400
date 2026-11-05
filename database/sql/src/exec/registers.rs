// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Special registers and built-in global variables beyond the date,
//! time and user ones: their values for the job, which ones `SET` may
//! change, and their result types. The job fills what it knows (its
//! name, process and thread); what QS/400 has no value for is null.

use super::rtype::SqlType;
use crate::error::{Result, SqlError};
use crate::value::Value;

/// The job's register values.
#[derive(Clone, Debug)]
pub struct Registers {
    /// `CURRENT DEGREE`.
    pub degree: String,
    /// `CURRENT IMPLICIT XMLPARSE OPTION`.
    pub xmlparse: String,
    /// `CURRENT CLIENT_ACCTNG`.
    pub client_acctng: String,
    /// `CURRENT CLIENT_APPLNAME`.
    pub client_applname: String,
    /// `CURRENT CLIENT_PROGRAMID`.
    pub client_programid: String,
    /// `CURRENT CLIENT_USERID`.
    pub client_userid: String,
    /// `CURRENT CLIENT_WRKSTNNAME`.
    pub client_wrkstnname: String,
    /// `SET PATH`: the path as `CURRENT PATH` shows it; `None` is the
    /// naming's default.
    pub path: Option<String>,
    /// `QSYS2.JOB_NAME`: `number/user/name`.
    pub job_name: Option<String>,
    /// `QSYS2.SERVER_MODE_JOB_NAME`.
    pub server_mode_job_name: Option<String>,
    /// `QSYS2.PROCESS_ID`.
    pub process_id: Option<i64>,
    /// `QSYS2.THREAD_ID`.
    pub thread_id: Option<i64>,
    /// `SYSIBM.CLIENT_HOST`.
    pub client_host: Option<String>,
    /// `SYSIBM.CLIENT_IPADDR`.
    pub client_ipaddr: Option<String>,
    /// `SYSIBM.CLIENT_PORT`.
    pub client_port: Option<i64>,
    /// `CURRENT TIMEZONE`: the job's offset from UTC in seconds.
    pub timezone_secs: i64,
    /// `CURRENT SERVER`: the relational database name.
    pub server: String,
}

impl Default for Registers {
    fn default() -> Self {
        Self {
            degree: "NONE".into(),
            xmlparse: "STRIP WHITESPACE".into(),
            client_acctng: String::new(),
            client_applname: String::new(),
            client_programid: String::new(),
            client_userid: String::new(),
            client_wrkstnname: String::new(),
            path: None,
            job_name: None,
            server_mode_job_name: None,
            process_id: None,
            thread_id: None,
            client_host: None,
            client_ipaddr: None,
            client_port: None,
            timezone_secs: 0,
            server: "QS400".into(),
        }
    }
}

/// The registers `CURRENT x` names beyond the parser's own, by the word
/// after `CURRENT` (or the underscore spelling).
pub const CURRENT: &[(&str, &str)] = &[
    ("DEGREE", "CURRENT DEGREE"),
    ("CLIENT_ACCTNG", "CURRENT CLIENT_ACCTNG"),
    ("CLIENT_APPLNAME", "CURRENT CLIENT_APPLNAME"),
    ("CLIENT_PROGRAMID", "CURRENT CLIENT_PROGRAMID"),
    ("CLIENT_USERID", "CURRENT CLIENT_USERID"),
    ("CLIENT_WRKSTNNAME", "CURRENT CLIENT_WRKSTNNAME"),
];

/// Built-in global variables by schema and name.
pub const GLOBALS: &[(&str, &str, &str)] = &[
    ("QSYS2", "JOB_NAME", "QSYS2.JOB_NAME"),
    ("QSYS2", "SERVER_MODE_JOB_NAME", "QSYS2.SERVER_MODE_JOB_NAME"),
    ("QSYS2", "PROCESS_ID", "QSYS2.PROCESS_ID"),
    ("QSYS2", "THREAD_ID", "QSYS2.THREAD_ID"),
    ("SYSIBM", "CLIENT_HOST", "SYSIBM.CLIENT_HOST"),
    ("SYSIBM", "CLIENT_IPADDR", "SYSIBM.CLIENT_IPADDR"),
    ("SYSIBM", "CLIENT_PORT", "SYSIBM.CLIENT_PORT"),
    ("SYSIBM", "PACKAGE_NAME", "SYSIBM.PACKAGE_NAME"),
    ("SYSIBM", "PACKAGE_SCHEMA", "SYSIBM.PACKAGE_SCHEMA"),
    ("SYSIBM", "PACKAGE_VERSION", "SYSIBM.PACKAGE_VERSION"),
    ("SYSIBM", "ROUTINE_SCHEMA", "SYSIBM.ROUTINE_SCHEMA"),
    ("SYSIBM", "ROUTINE_SPECIFIC_NAME", "SYSIBM.ROUTINE_SPECIFIC_NAME"),
    ("SYSIBM", "ROUTINE_TYPE", "SYSIBM.ROUTINE_TYPE"),
];

/// The canonical name of a built-in global variable.
#[must_use]
pub fn global(schema: Option<&str>, name: &str) -> Option<&'static str> {
    GLOBALS.iter().find(|(s, n, _)| *n == name && schema.is_none_or(|x| x == *s)).map(|(_, _, c)| *c)
}

/// Whether `SET` may change the register.
#[must_use]
pub fn settable(name: &str) -> bool {
    matches!(name, "CURRENT DEGREE" | "CURRENT IMPLICIT XMLPARSE OPTION")
}

fn opt_str(v: Option<&String>) -> Value {
    v.map_or(Value::Null, |s| Value::Str(s.clone()))
}

impl Registers {
    /// The value of a register or variable.
    #[must_use]
    pub fn value(&self, name: &str) -> Value {
        match name {
            "CURRENT DEGREE" => Value::Str(self.degree.clone()),
            "CURRENT IMPLICIT XMLPARSE OPTION" => Value::Str(self.xmlparse.clone()),
            "CURRENT CLIENT_ACCTNG" => Value::Str(self.client_acctng.clone()),
            "CURRENT CLIENT_APPLNAME" => Value::Str(self.client_applname.clone()),
            "CURRENT CLIENT_PROGRAMID" => Value::Str(self.client_programid.clone()),
            "CURRENT CLIENT_USERID" => Value::Str(self.client_userid.clone()),
            "CURRENT CLIENT_WRKSTNNAME" => Value::Str(self.client_wrkstnname.clone()),
            "QSYS2.JOB_NAME" => opt_str(self.job_name.as_ref()),
            "QSYS2.SERVER_MODE_JOB_NAME" => opt_str(self.server_mode_job_name.as_ref()),
            "QSYS2.PROCESS_ID" => self.process_id.map_or(Value::Null, Value::int),
            "QSYS2.THREAD_ID" => self.thread_id.map_or(Value::Null, Value::int),
            "SYSIBM.CLIENT_HOST" => opt_str(self.client_host.as_ref()),
            "SYSIBM.CLIENT_IPADDR" => opt_str(self.client_ipaddr.as_ref()),
            "SYSIBM.CLIENT_PORT" => self.client_port.map_or(Value::Null, Value::int),
            // No packages and no routines run here yet.
            _ => Value::Null,
        }
    }

    /// `SET` of a settable register: the value checked as IBM checks it.
    pub fn set(&mut self, name: &str, v: &Value) -> Result<()> {
        let text = v.as_text().trim().to_ascii_uppercase();
        let bad = || SqlError::Conversion { what: format!("'{text}' is not a value of {name}") };
        match name {
            "CURRENT DEGREE" => {
                let ok = matches!(text.as_str(), "ANY" | "NONE" | "MAX" | "IO" | "OPTIMIZE")
                    || text.parse::<u32>().is_ok_and(|n| (1..=32767).contains(&n));
                if !ok {
                    return Err(bad());
                }
                self.degree = text;
            }
            "CURRENT IMPLICIT XMLPARSE OPTION" => {
                if !matches!(text.as_str(), "STRIP WHITESPACE" | "PRESERVE WHITESPACE") {
                    return Err(bad());
                }
                self.xmlparse = text;
            }
            _ => return Err(SqlError::Unsupported { what: format!("SET {name}") }),
        }
        Ok(())
    }
}

/// The type of a register or variable.
#[must_use]
pub fn result_type(name: &str) -> SqlType {
    match name {
        "CURRENT DEGREE" => SqlType::char(5, false),
        "CURRENT DECFLOAT ROUNDING MODE" | "CURRENT IMPLICIT XMLPARSE OPTION" => SqlType::varchar(128, false),
        "CURRENT CLIENT_ACCTNG" | "CURRENT CLIENT_APPLNAME" | "CURRENT CLIENT_USERID" | "CURRENT CLIENT_PROGRAMID"
        | "CURRENT CLIENT_WRKSTNNAME" => SqlType::varchar(255, false),
        "QSYS2.JOB_NAME" | "QSYS2.SERVER_MODE_JOB_NAME" => SqlType::varchar(28, true),
        "QSYS2.PROCESS_ID" | "SYSIBM.CLIENT_PORT" => SqlType::int(true),
        "QSYS2.THREAD_ID" => SqlType::bigint(true),
        "SYSIBM.CLIENT_IPADDR" => SqlType::varchar(128, true),
        _ => SqlType::varchar(255, true),
    }
}

/// The system path, as `CURRENT PATH` spells it.
const SYSTEM_PATH: &str = "\"QSYS\",\"QSYS2\",\"SYSPROC\",\"SYSIBMADM\"";

/// The text of `CURRENT PATH` after `SET PATH`.
#[must_use]
pub fn path_text(ctx: &super::SqlContext, entries: &[crate::ast::PathEntry]) -> String {
    use crate::ast::PathEntry as P;
    let current = ctx.registers.path.clone().unwrap_or_else(|| match ctx.naming {
        crate::ast::Naming::Sys => "*LIBL".into(),
        crate::ast::Naming::Sql => format!("{SYSTEM_PATH},\"{}\"", ctx.user),
    });
    let parts: Vec<String> = entries
        .iter()
        .map(|e| match e {
            P::Name(n) => format!("\"{}\"", n.value),
            P::Libl => "*LIBL".into(),
            P::SystemPath => SYSTEM_PATH.into(),
            P::User => format!("\"{}\"", ctx.user),
            P::CurrentPath => current.clone(),
        })
        .collect();
    parts.join(",")
}
