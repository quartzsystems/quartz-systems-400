// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reply messages: `SVRCOD` first, then what the message names.

use crate::cp::{param, rm, svrcod};
use crate::msg::access::rdbnam;
use crate::wire::Object;

/// A reply message with its severity and further parameters.
pub fn message(cp: u16, severity: u16, rest: Vec<Object>) -> Object {
    let mut v = vec![Object::u16(param::SVRCOD, severity)];
    v.extend(rest);
    Object::coll(cp, v)
}

/// `ENDQRYRM`: the query ended.
pub fn endqryrm(rdb: &str) -> Object {
    message(rm::ENDQRYRM, svrcod::WARNING, vec![rdbnam(rdb)])
}

/// `ENDUOWRM`: the unit of work ended (`UOWDSP` 1 committed, 2 rolled
/// back).
pub fn enduowrm(rdb: &str, uowdsp: u8) -> Object {
    message(
        rm::ENDUOWRM,
        svrcod::WARNING,
        vec![Object::byte(param::UOWDSP, uowdsp), rdbnam(rdb)],
    )
}

/// `OPNQRYRM`: the query is open with fixed row protocol (`0x2418`)
/// or limited block protocol (`0x2417`).
pub fn opnqryrm(qryprctyp: u16, qryinsid: &[u8; 8]) -> Object {
    message(
        rm::OPNQRYRM,
        svrcod::INFO,
        vec![
            Object::u16(param::QRYPRCTYP, qryprctyp),
            Object::byte(param::SQLCSRHLD, 0xF0),
            Object::data(param::QRYINSID, qryinsid.to_vec()),
        ],
    )
}

/// `OPNQFLRM`: the open failed.
pub fn opnqflrm(rdb: &str) -> Object {
    message(rm::OPNQFLRM, svrcod::ERROR, vec![rdbnam(rdb)])
}

/// `QRYNOPRM`: no such query is open.
pub fn qrynoprm(rdb: &str, pkg: &Object) -> Object {
    message(rm::QRYNOPRM, svrcod::ERROR, vec![rdbnam(rdb), pkg.clone()])
}

/// `SQLERRRM`: an SQL error; the SQLCARD follows.
pub fn sqlerrrm(rdb: &str) -> Object {
    message(rm::SQLERRRM, svrcod::ERROR, vec![rdbnam(rdb)])
}

/// `RDBUPDRM`: the RDB was updated.
pub fn rdbupdrm(rdb: &str) -> Object {
    message(rm::RDBUPDRM, svrcod::INFO, vec![rdbnam(rdb)])
}

/// `RDBNACRM`: the RDB is not accessed.
pub fn rdbnacrm(rdb: &str) -> Object {
    message(rm::RDBNACRM, svrcod::ERROR, vec![rdbnam(rdb)])
}

/// `RDBNFNRM`: no such RDB.
pub fn rdbnfnrm(rdb: &str) -> Object {
    message(rm::RDBNFNRM, svrcod::ERROR, vec![rdbnam(rdb)])
}

/// `CMDNSPRM`: the command is not supported.
pub fn cmdnsprm(code: u16) -> Object {
    message(
        rm::CMDNSPRM,
        svrcod::ERROR,
        vec![Object::u16(param::CODPNT, code)],
    )
}

/// `PRMNSPRM`: a parameter is not supported.
pub fn prmnsprm(code: u16) -> Object {
    message(
        rm::PRMNSPRM,
        svrcod::ERROR,
        vec![Object::u16(param::CODPNT, code)],
    )
}

/// `SYNTAXRM`: the datastream does not read.
pub fn syntaxrm(synerrcd: u8, code: u16) -> Object {
    message(
        rm::SYNTAXRM,
        svrcod::ERROR,
        vec![
            Object::byte(param::SYNERRCD, synerrcd),
            Object::u16(param::CODPNT, code),
        ],
    )
}

/// `PRCCNVRM`: a command out of order.
pub fn prccnvrm(prccnvcd: u8) -> Object {
    message(
        rm::PRCCNVRM,
        svrcod::ERROR,
        vec![Object::byte(param::PRCCNVCD, prccnvcd)],
    )
}

/// `CMDCHKRM`: the command failed its checks.
pub fn cmdchkrm(severity: u16) -> Object {
    message(rm::CMDCHKRM, severity, Vec::new())
}
