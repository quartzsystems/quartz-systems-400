// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 199's QAP half: the `Dirsrv` service (version 12) through the
//! real connection jobs against the running `QUSRDIR`. As `QSECOFR`: the
//! instances (`QUSRDIR` running, its job), the properties (389), the
//! suffixes (`cn=localhost`), the indexes, an index on `description`
//! put and removed, the administrator, the schema's `person` classes,
//! the suffix's access control, publishing and its status, the
//! counters. As `QAPTEST` (no `*IOSYSCFG`): the instances refused.

use qap_proto::ops::dirsrv::{
    DirAdmin, DirCounters, DirIndex, DirIndexRef, DirIndexes, DirProperties, DnRef, EntryAccess,
    InstanceRef, InstanceState, Instances, ListDirSchema, Publishing, PublishingStatus,
    PutDirIndex, SchemaDefinitions, SchemaKind, Suffixes,
};
use qap_proto::{Op, Status};

use super::client::{Conn, brief, msgid};

/// The instance and the suffix steps 186 and 187 use.
const INSTANCE: &str = "QUSRDIR";
const SUFFIX: &str = "cn=localhost";
/// The index the step puts and removes.
const INDEXED: &str = "description";

fn q() -> InstanceRef {
    InstanceRef {
        instance: INSTANCE.into(),
    }
}

fn indexes(sec: &mut Conn) -> Result<DirIndexes, String> {
    sec.ok(Op::ListDirIndexes, &q())
}

/// Run the step on `a` (`QAPTEST`) and `sec` (`QSECOFR`), both signed on.
pub fn run(a: &mut Conn, sec: &mut Conn) -> Result<String, String> {
    let no = a.call(Op::ListDirInstances, &())?;
    if no.status != Status::NotAuthorized {
        return Err(format!("ListDirInstances as QAPTEST: {}", brief(&no)));
    }
    let refused = msgid(&no).to_string();
    let list: Instances = sec.ok(Op::ListDirInstances, &())?;
    let me = list
        .instances
        .iter()
        .find(|i| i.name == INSTANCE)
        .ok_or_else(|| format!("no {INSTANCE} among {:?}", list.instances))?;
    if me.state != InstanceState::Running || me.job.is_none() {
        return Err(format!("{INSTANCE}: {me:?}"));
    }
    let p: DirProperties = sec.ok(Op::GetDirProperties, &q())?;
    if p.port != 389 {
        return Err(format!("properties: {p:?}"));
    }
    let s: Suffixes = sec.ok(Op::ListDirSuffixes, &q())?;
    if !s.suffixes.iter().any(|x| x.dn.eq_ignore_ascii_case(SUFFIX)) {
        return Err(format!("no {SUFFIX} among {:?}", s.suffixes));
    }
    let before = indexes(sec)?;
    if !before
        .indexes
        .iter()
        .any(|i| i.attribute.eq_ignore_ascii_case("cn"))
    {
        return Err(format!("no cn index: {:?}", before.indexes));
    }
    index_put_and_removed(sec, before.indexes.len())?;
    let admin: DirAdmin = sec.ok(Op::GetDirAdmin, &q())?;
    if !admin.dn.eq_ignore_ascii_case("cn=Administrator") || !admin.password_set {
        return Err(format!("administrator: {admin:?}"));
    }
    let schema: SchemaDefinitions = sec.ok(
        Op::ListDirSchema,
        &ListDirSchema {
            instance: INSTANCE.into(),
            kind: SchemaKind::ObjectClass,
            contains: "person".into(),
            max: 20,
            ..ListDirSchema::default()
        },
    )?;
    if schema.definitions.is_empty() {
        return Err("no object class named like person".into());
    }
    let access: EntryAccess = sec.ok(
        Op::GetDirAccess,
        &DnRef {
            instance: INSTANCE.into(),
            dn: SUFFIX.into(),
        },
    )?;
    let publishing: Publishing = sec.ok(Op::GetPublishing, &())?;
    let status: PublishingStatus = sec.ok(Op::GetPublishingStatus, &())?;
    if publishing.agents.len() != 2 || status.agents.len() != 2 {
        return Err(format!("publishing: {publishing:?}; {status:?}"));
    }
    let counts: DirCounters = sec.ok(Op::GetDirCounters, &q())?;
    if counts.binds == 0 || counts.started == 0 {
        return Err(format!("counters: {counts:?}"));
    }
    Ok(format!(
        "QAP: QAPTEST refused ({refused}); as QSECOFR {INSTANCE} running, port {}, {} suffixes, {} indexes, {INDEXED} indexed and removed, {}, {} person classes, {SUFFIX}'s ACL from {}, 2 publishing agents, {} binds counted",
        p.port,
        s.suffixes.len(),
        before.indexes.len(),
        admin.dn,
        schema.definitions.len(),
        access.acl_source,
        counts.binds
    ))
}

/// An index on [`INDEXED`] put, seen and removed; the count as it was.
fn index_put_and_removed(sec: &mut Conn, count: usize) -> Result<(), String> {
    let put = PutDirIndex {
        instance: INSTANCE.into(),
        index: DirIndex {
            attribute: INDEXED.into(),
            equality: true,
            ..DirIndex::default()
        },
    };
    let r = sec.call(Op::PutDirIndex, &put)?;
    if r.status != Status::Ok {
        return Err(format!("PutDirIndex {INDEXED}: {}", brief(&r)));
    }
    let with = indexes(sec)?;
    if !with
        .indexes
        .iter()
        .any(|i| i.attribute.eq_ignore_ascii_case(INDEXED) && i.equality)
    {
        return Err(format!("{INDEXED} not indexed: {:?}", with.indexes));
    }
    let rm = DirIndexRef {
        instance: INSTANCE.into(),
        attribute: INDEXED.into(),
    };
    let r = sec.call(Op::RemoveDirIndex, &rm)?;
    if r.status != Status::Ok {
        return Err(format!("RemoveDirIndex {INDEXED}: {}", brief(&r)));
    }
    if indexes(sec)?.indexes.len() != count {
        return Err("the index count after the removal".into());
    }
    Ok(())
}
