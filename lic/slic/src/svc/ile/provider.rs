// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system APIs' provider over the lent store: object lists and
//! descriptions, user indexes, data queues and areas, system values,
//! file descriptions, and commands the console runs after the call.

use qsname::Name;
use store::obj::dtaq::{self, KeyOp, Order, Select};
use store::obj::header::text_of;
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types, usridx};
use store::txn::Txn;
use sysapi::{
    ApiFail, ApiResult, DataArea, DataAreaType as ApiAreaType, DepInfo, DtaqEntry, DtaqInfo,
    FieldInfo, FileInfo, FormatInfo, IdxAttr, LocalNow, MemberInfo, ObjInfo, Provider, SysVal,
};

use super::sources::find_object;
use super::{Shared, with_main, with_temp};

/// The provider.
pub struct StoreProvider {
    shared: Shared,
    /// Commands `QCMDEXC` asked for, run by the console after the call.
    pub pending: Vec<String>,
}

impl StoreProvider {
    /// Over the lent store.
    pub fn new(shared: Shared) -> Self {
        Self {
            shared,
            pending: Vec::new(),
        }
    }

    fn run<T>(
        &self,
        name: &str,
        lib: &str,
        type_code: u16,
        f: impl Fn(&mut Txn<'_>, ObjectId) -> Result<T, String>,
    ) -> ApiResult<T> {
        let (id, _, temp) = find_object(&self.shared, name, lib, type_code)
            .ok_or_else(|| ApiFail::not_found(name, lib))?;
        let r = if temp {
            with_temp(&self.shared, |txn, _| f(txn, id))
        } else {
            with_main(&self.shared, |txn, _| f(txn, id))
        };
        r.map_err(|e| ApiFail::id("CPF9899").text(&e, e.len().min(100)))
    }
}

/// The type name of an object type code.
fn type_name(code: u16) -> String {
    store::obj::types::type_by_code(code)
        .map_or_else(|| format!("*{code:04X}"), |t| t.name.to_string())
}

fn type_code_of(name: &str) -> Option<u16> {
    store::obj::types::type_by_name(name).map(|t| t.code)
}

fn stamp(unix: u64) -> String {
    sysapi::picture::render(sysapi::Civil::of_unix(unix), "YYYYMMDDHHMISS")
        .map(|s| {
            let c = if unix >= 946_684_800 { "1" } else { "0" };
            format!("{c}{}", &s[2..])
        })
        .unwrap_or_default()
}

impl Provider for StoreProvider {
    fn list_objects(&mut self, lib: &str, name: &str, ty: &str) -> Vec<ObjInfo> {
        let wanted = if ty == "*ALL" { None } else { type_code_of(ty) };
        let r = with_main(&self.shared, |txn, qsys| {
            let libs: Vec<(String, ObjectId)> = match lib {
                "*LIBL" | "*ALL" => super::name_context(txn, qsys)
                    .libl
                    .iter()
                    .filter_map(|id| {
                        ops::describe(txn, *id)
                            .ok()
                            .map(|h| (h.name.to_string(), *id))
                    })
                    .collect(),
                l => {
                    let n = Name::parse(l).map_err(|e| e.to_string())?;
                    vec![(
                        l.to_string(),
                        ops::find_library(txn, qsys, n)
                            .map_err(|e| e.to_string())?
                            .id,
                    )]
                }
            };
            let mut out = Vec::new();
            for (lname, lid) in libs {
                for (key, id) in ops::list(txn, lid, wanted).map_err(|e| e.to_string())? {
                    let n = key.name.to_string();
                    let hit = match name.strip_suffix('*') {
                        _ if name == "*ALL" => true,
                        Some(p) => n.starts_with(p),
                        None => n == name,
                    };
                    if !hit {
                        continue;
                    }
                    let h = ops::describe(txn, id).map_err(|e| e.to_string())?;
                    out.push(ObjInfo {
                        name: n,
                        lib: lname.clone(),
                        ty: type_name(key.type_code),
                        attr: text_of(&h.attribute),
                        text: text_of(&h.text),
                        owner: text_of(&h.owner),
                        created: stamp(h.created),
                        changed: stamp(h.changed),
                        source: None,
                        user_attr: String::new(),
                    });
                }
            }
            Ok(out)
        });
        r.unwrap_or_default()
    }

    fn object(&mut self, lib: &str, name: &str, ty: &str) -> Option<ObjInfo> {
        self.list_objects(lib, name, ty).into_iter().next()
    }

    fn change_object(
        &mut self,
        lib: &str,
        name: &str,
        ty: &str,
        text: Option<String>,
        _attr: Option<String>,
    ) -> ApiResult<String> {
        let code =
            type_code_of(ty).ok_or_else(|| ApiFail::id("CPF9801").text(name, 10).text(lib, 10))?;
        let (id, found, temp) = find_object(&self.shared, name, lib, code)
            .ok_or_else(|| ApiFail::not_found(name, lib))?;
        if let Some(t) = text {
            let now = crate::ipl::system::now_unix();
            let f =
                |txn: &mut Txn<'_>| ops::change_text(txn, id, &t, now).map_err(|e| e.to_string());
            let r = if temp {
                with_temp(&self.shared, |txn, _| f(txn))
            } else {
                with_main(&self.shared, |txn, _| f(txn))
            };
            r.map_err(|e| ApiFail::id("CPF9899").text(&e, 50))?;
        }
        Ok(found)
    }

    fn rename_object(
        &mut self,
        lib: &str,
        name: &str,
        ty: &str,
        new_lib: &str,
        new_name: &str,
        _replace: bool,
    ) -> ApiResult<String> {
        let code =
            type_code_of(ty).ok_or_else(|| ApiFail::id("CPF9801").text(name, 10).text(lib, 10))?;
        let (id, found, temp) = find_object(&self.shared, name, lib, code)
            .ok_or_else(|| ApiFail::not_found(name, lib))?;
        let now = crate::ipl::system::now_unix();
        let nn = Name::parse(new_name).map_err(|_| ApiFail::id("CPF9801").text(new_name, 10))?;
        let r = if temp {
            with_temp(&self.shared, |txn, _| {
                ops::rename(txn, id, nn, now).map_err(|e| e.to_string())
            })
        } else {
            with_main(&self.shared, |txn, qsys| {
                ops::rename(txn, id, nn, now).map_err(|e| e.to_string())?;
                if new_lib != "*SAME" && !new_lib.is_empty() && new_lib != found {
                    let l = ops::find_library(
                        txn,
                        qsys,
                        Name::parse(new_lib).map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string())?;
                    ops::move_to(txn, id, l.id, now).map_err(|e| e.to_string())?;
                }
                Ok(())
            })
        };
        r.map_err(|e| ApiFail::id("CPF2132").text(&e, 50))?;
        Ok(found)
    }

    fn usridx_create(&mut self, lib: &str, name: &str, attr: IdxAttr) -> ApiResult<()> {
        let n = Name::parse(name).map_err(|_| ApiFail::no_library(lib))?;
        let lib_u = lib.to_ascii_uppercase();
        let key_len = u16::try_from(if attr.keyed { attr.key_len } else { 1 }).unwrap_or(1);
        let entry_len = u16::try_from(attr.entry_len).unwrap_or(1).max(key_len);
        let params = |library: ObjectId| CreateParams {
            library,
            name: n,
            type_code: types::USRIDX,
            attribute: String::new(),
            text: attr.text.clone(),
            owner: "QSYS".into(),
            system: "QS400".into(),
            now: crate::ipl::system::now_unix(),
            by_system: false,
        };
        let r = if lib_u == "QTEMP" {
            with_temp(&self.shared, |txn, qtemp| {
                usridx::create(txn, &params(qtemp), attr.fixed, key_len, entry_len)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            })
        } else {
            let target = if lib_u == "*CURLIB" || lib_u == "*LIBL" {
                "QGPL".to_string()
            } else {
                lib_u.clone()
            };
            let user_domain = super::sources::user_domain_allowed(&self.shared, &target);
            with_main(&self.shared, |txn, qsys| {
                let l =
                    ops::find_library(txn, qsys, Name::parse(&target).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                let id = usridx::create(txn, &params(l.id), attr.fixed, key_len, entry_len)
                    .map_err(|e| e.to_string())?;
                if !user_domain {
                    let (page, mut h) = ops::read_header(txn, id).map_err(|e| e.to_string())?;
                    h.domain = store::obj::header::Domain::System;
                    ops::write_header(txn, page, &h).map_err(|e| e.to_string())?;
                }
                Ok(())
            })
        };
        r.map_err(|e| {
            if e.contains("exists") {
                ApiFail::id("CPF9870").text(name, 10).text(lib, 10)
            } else {
                ApiFail::id("CPF9810").text(lib, 10)
            }
        })
    }

    fn usridx_delete(&mut self, lib: &str, name: &str) -> ApiResult<()> {
        self.run(name, lib, types::USRIDX, |txn, id| {
            ops::delete(txn, JobHandle(1), id).map_err(|e| e.to_string())
        })
    }

    fn usridx_add(
        &mut self,
        lib: &str,
        name: &str,
        entries: Vec<Vec<u8>>,
        replace: bool,
    ) -> ApiResult<usize> {
        self.run(name, lib, types::USRIDX, |txn, id| {
            let attr = usridx::describe(txn, id).map_err(|e| e.to_string())?;
            let mut n = 0;
            for e in &entries {
                if replace {
                    let key = &e[..usize::from(attr.key_len).min(e.len())];
                    let _ = usridx::remove(txn, id, KeyOp::Eq, key, usize::MAX);
                }
                usridx::insert(txn, id, e).map_err(|e| e.to_string())?;
                n += 1;
            }
            Ok(n)
        })
    }

    fn usridx_find(
        &mut self,
        lib: &str,
        name: &str,
        rule: u32,
        key: &[u8],
        key2: &[u8],
        max: usize,
        remove: bool,
    ) -> ApiResult<Vec<Vec<u8>>> {
        self.run(name, lib, types::USRIDX, |txn, id| {
            let found: Vec<Vec<u8>> = match rule {
                6 => usridx::first(txn, id, max),
                7 => usridx::last(txn, id, max),
                8 => usridx::find(txn, id, KeyOp::Ge, key, usize::MAX).map(|v| {
                    v.into_iter()
                        .filter(|e| e.as_slice() <= key2 || e.get(..key2.len()) == Some(key2))
                        .take(max)
                        .collect()
                }),
                r => {
                    let op = match r {
                        1 => KeyOp::Eq,
                        2 => KeyOp::Gt,
                        3 => KeyOp::Lt,
                        4 => KeyOp::Ge,
                        _ => KeyOp::Le,
                    };
                    usridx::find(txn, id, op, key, max)
                }
            }
            .map_err(|e| e.to_string())?;
            if remove {
                let attr = usridx::describe(txn, id).map_err(|e| e.to_string())?;
                for e in &found {
                    let k = &e[..usize::from(attr.key_len).min(e.len())];
                    let _ = usridx::remove(txn, id, KeyOp::Eq, k, 1);
                }
            }
            Ok(found)
        })
    }

    fn usridx_attrs(&mut self, lib: &str, name: &str) -> ApiResult<(IdxAttr, usize, String)> {
        let (_, found, _) = find_object(&self.shared, name, lib, types::USRIDX)
            .ok_or_else(|| ApiFail::not_found(name, lib))?;
        self.run(name, lib, types::USRIDX, |txn, id| {
            let a = usridx::describe(txn, id).map_err(|e| e.to_string())?;
            let h = ops::describe(txn, id).map_err(|e| e.to_string())?;
            Ok((
                IdxAttr {
                    fixed: a.fixed,
                    entry_len: u32::from(a.entry_len),
                    keyed: true,
                    key_len: u32::from(a.key_len),
                    immediate: true,
                    sequential: false,
                    text: text_of(&h.text),
                },
                usize::try_from(a.entry_count).unwrap_or(0),
                found.clone(),
            ))
        })
    }

    fn dtaq_send(
        &mut self,
        lib: &str,
        name: &str,
        key: &[u8],
        data: &[u8],
        _sender: &str,
    ) -> ApiResult<()> {
        self.run(name, lib, types::DTAQ, |txn, id| {
            dtaq::send(txn, id, key, data, None)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
    }

    fn dtaq_receive(
        &mut self,
        lib: &str,
        name: &str,
        order: &str,
        key: &[u8],
        remove: bool,
    ) -> ApiResult<Option<DtaqEntry>> {
        let _ = remove;
        self.run(name, lib, types::DTAQ, |txn, id| {
            let op = match order.trim() {
                "" | "EQ" => KeyOp::Eq,
                "NE" => KeyOp::Ne,
                "GT" => KeyOp::Gt,
                "GE" => KeyOp::Ge,
                "LT" => KeyOp::Lt,
                _ => KeyOp::Le,
            };
            let attr = dtaq::describe(txn, id).map_err(|e| e.to_string())?;
            let sel = if matches!(attr.order, Order::Keyed { .. }) {
                Select::Key(op, key)
            } else {
                Select::Next
            };
            Ok(dtaq::receive(txn, id, &sel)
                .map_err(|e| e.to_string())?
                .map(|e| DtaqEntry {
                    key: e.key,
                    data: e.data,
                    sender: Vec::new(),
                }))
        })
    }

    fn dtaq_clear(&mut self, lib: &str, name: &str, _order: &str, _key: &[u8]) -> ApiResult<()> {
        self.run(name, lib, types::DTAQ, |txn, id| {
            dtaq::clear(txn, id).map_err(|e| e.to_string())
        })
    }

    fn dtaq_describe(&mut self, lib: &str, name: &str) -> ApiResult<DtaqInfo> {
        let (_, found, _) = find_object(&self.shared, name, lib, types::DTAQ)
            .ok_or_else(|| ApiFail::not_found(name, lib))?;
        self.run(name, lib, types::DTAQ, |txn, id| {
            let a = dtaq::describe(txn, id).map_err(|e| e.to_string())?;
            let h = ops::describe(txn, id).map_err(|e| e.to_string())?;
            Ok(DtaqInfo {
                name: name.to_string(),
                lib: found.clone(),
                max_len: a.max_entry_len,
                key_len: match a.order {
                    Order::Keyed { key_len } => u32::from(key_len),
                    _ => 0,
                },
                sequence: match a.order {
                    Order::Fifo => 'F',
                    Order::Lifo => 'L',
                    Order::Keyed { .. } => 'K',
                },
                sender_id: a.sender_id,
                text: text_of(&h.text),
                entries: u32::try_from(a.entry_count).unwrap_or(u32::MAX),
                allocated: u32::try_from(a.entry_count).unwrap_or(u32::MAX),
            })
        })
    }

    fn dtaq_entries(&mut self, lib: &str, name: &str) -> ApiResult<Vec<DtaqEntry>> {
        let _ = (lib, name);
        Ok(Vec::new())
    }

    fn dtaara_read(&mut self, lib: &str, name: &str) -> ApiResult<DataArea> {
        let (_, found, _) = find_object(&self.shared, name, lib, types::DTAARA)
            .ok_or_else(|| ApiFail::not_found(name, lib))?;
        self.run(name, lib, types::DTAARA, |txn, id| {
            use store::obj::dtaara::{self, DataAreaType, DataAreaValue};
            let (kind, _) = dtaara::describe(txn, id).map_err(|e| e.to_string())?;
            let v = dtaara::read(txn, id).map_err(|e| e.to_string())?;
            Ok(match (kind, v) {
                (DataAreaType::Char { len }, DataAreaValue::Char(s)) => {
                    let mut b = ccsid::to_ebcdic(&s).unwrap_or_default();
                    b.resize(usize::from(len), 0x40);
                    DataArea {
                        ty: ApiAreaType::Char,
                        lib: found.clone(),
                        len: u32::from(len),
                        decimals: 0,
                        value: b,
                    }
                }
                (DataAreaType::Dec { digits, decimals }, DataAreaValue::Dec(n)) => {
                    let p = qdecimal::Packed::new(digits, decimals).map_err(|e| e.to_string())?;
                    let d = qdecimal::Decimal::new(n, decimals).map_err(|e| e.to_string())?;
                    DataArea {
                        ty: ApiAreaType::Dec,
                        lib: found.clone(),
                        len: u32::from(digits),
                        decimals: u32::from(decimals),
                        value: p
                            .encode(&d, qdecimal::Rounding::Truncate)
                            .map_err(|e| e.to_string())?,
                    }
                }
                (_, DataAreaValue::Lgl(b)) => DataArea {
                    ty: ApiAreaType::Lgl,
                    lib: found.clone(),
                    len: 1,
                    decimals: 0,
                    value: vec![if b { 0xF1 } else { 0xF0 }],
                },
                _ => DataArea {
                    ty: ApiAreaType::Char,
                    lib: found.clone(),
                    len: 0,
                    decimals: 0,
                    value: Vec::new(),
                },
            })
        })
    }

    fn dtaara_write(&mut self, lib: &str, name: &str, start: usize, bytes: &[u8]) -> ApiResult<()> {
        self.run(name, lib, types::DTAARA, |txn, id| {
            use store::obj::dtaara::{self, DataAreaType, DataAreaValue};
            let (kind, _) = dtaara::describe(txn, id).map_err(|e| e.to_string())?;
            match kind {
                DataAreaType::Char { .. } => dtaara::write_substring(
                    txn,
                    id,
                    u16::try_from(start).unwrap_or(1),
                    u16::try_from(bytes.len()).unwrap_or(u16::MAX),
                    &ccsid::from_ebcdic(bytes),
                )
                .map_err(|e| e.to_string()),
                DataAreaType::Dec { digits, decimals } => {
                    let p = qdecimal::Packed::new(digits, decimals).map_err(|e| e.to_string())?;
                    let d = p.decode(bytes).map_err(|e| e.to_string())?;
                    let n = d
                        .rescale(decimals, qdecimal::Rounding::Truncate)
                        .ok()
                        .and_then(|d| d.mantissa_i128())
                        .unwrap_or(0);
                    dtaara::write(txn, id, &DataAreaValue::Dec(n)).map_err(|e| e.to_string())
                }
                DataAreaType::Lgl => {
                    dtaara::write(txn, id, &DataAreaValue::Lgl(bytes.first() == Some(&0xF1)))
                        .map_err(|e| e.to_string())
                }
            }
        })
    }

    fn sysval(&mut self, name: &str) -> Option<SysVal> {
        let lent = self.shared.borrow();
        let sv = lent.as_ref()?.sysvals.as_ref()?;
        Some(match sv.get(name).ok()? {
            sysval::Value::Dec(n) => SysVal::bin(i32::try_from(n).unwrap_or(0)),
            sysval::Value::Char(s) => SysVal::chars(&s, s.len().max(1)),
            sysval::Value::List(v) => {
                let j = v.join(" ");
                SysVal::chars(&j, j.len().max(1))
            }
        })
    }

    fn netattr(&mut self, name: &str) -> Option<SysVal> {
        use crate::svc::neta;
        if !neta::is_attribute(name) {
            return None;
        }
        let sysname = self
            .sysval("QSYSNAME")
            .map_or_else(|| "QS400".to_string(), |v| ccsid::decode_trimmed(&v.data));
        let value = self
            .run(neta::AREA, "QSYS", types::DTAARA, |txn, id| neta::read_area(txn, id, &sysname))
            .ok()
            .and_then(|m| m.get(name).cloned())
            .or_else(|| neta::defaults(&sysname).get(name).cloned())?;
        Some(if neta::NUMERIC.contains(&name) {
            SysVal::bin(value.trim().parse().unwrap_or(0))
        } else {
            let len = match name {
                "MSGQ" | "OUTQ" | "DDMACC" | "PCSACC" | "ALRFTR" => 20,
                "DFTCNNLST" | "NWSDOMAIN" => 10,
                _ => 8,
            };
            SysVal::chars(&value, len.max(value.len()))
        })
    }

    fn now(&mut self) -> LocalNow {
        let unix = crate::ipl::system::now_unix();
        let offset_min = self
            .sysval("QUTCOFFSET")
            .map(|v| ccsid::decode_trimmed(&v.data))
            .and_then(|s| {
                let (sign, rest) = s.split_at(1);
                let (h, m) = rest.split_once(':')?;
                let v: i32 = h.parse::<i32>().ok()? * 60 + m.parse::<i32>().ok()?;
                Some(if sign == "-" { -v } else { v })
            })
            .unwrap_or(0);
        LocalNow {
            unix,
            offset_min,
            zone: self
                .sysval("QTIMZON")
                .map_or_else(|| "QN0000UTC".into(), |v| ccsid::decode_trimmed(&v.data)),
            dst: false,
        }
    }

    fn exec_command(&mut self, text: &str) -> ApiResult<()> {
        self.pending.push(text.to_string());
        Ok(())
    }

    fn check_command(&mut self, text: &str) -> ApiResult<()> {
        if text.trim().is_empty() {
            return Err(ApiFail::id("CPF0006"));
        }
        Ok(())
    }

    fn file(&mut self, lib: &str, name: &str) -> Option<FileInfo> {
        let (_, found, _) = find_object(&self.shared, name, lib, types::FILE)?;
        self.run(name, lib, types::FILE, |txn, id| {
            let d = db::file::describe(txn, id).map_err(|e| e.to_string())?;
            Ok(FileInfo {
                name: d.name().to_string(),
                lib: found.clone(),
                attr: if matches!(d.attr.kind, db::file::FileKind::Logical) {
                    "LF".into()
                } else {
                    "PF".into()
                },
                text: text_of(&d.header.text),
                rec_len: d.formats.first().map_or(0, |f| f.format.record_len),
                keyed: d.formats.first().is_some_and(|f| !f.format.keys.is_empty()),
                source: !matches!(
                    d.attr.kind,
                    db::file::FileKind::Logical | db::file::FileKind::Physical
                ),
                based_on: Vec::new(),
            })
        })
        .ok()
    }

    fn members(&mut self, lib: &str, name: &str) -> Vec<MemberInfo> {
        self.run(name, lib, types::FILE, |txn, id| {
            let d = db::file::describe(txn, id).map_err(|e| e.to_string())?;
            Ok(d.members
                .iter()
                .map(|m| MemberInfo {
                    name: m.name.to_string(),
                    srctype: m.srctype.map(|n| n.to_string()).unwrap_or_default(),
                    created: stamp(d.header.created),
                    changed: stamp(d.header.changed),
                    text: String::new(),
                    records: 0,
                    deleted: 0,
                })
                .collect())
        })
        .unwrap_or_default()
    }

    fn formats(&mut self, lib: &str, name: &str) -> Vec<FormatInfo> {
        self.run(name, lib, types::FILE, |txn, id| {
            let d = db::file::describe(txn, id).map_err(|e| e.to_string())?;
            Ok(d.formats
                .iter()
                .map(|f| FormatInfo {
                    name: f.format.name.to_string(),
                    rec_len: f.format.record_len,
                    text: f.format.text.clone().unwrap_or_default(),
                    fields: f
                        .format
                        .fields
                        .iter()
                        .map(|fd| FieldInfo {
                            name: fd.name.to_string(),
                            ty: fd.data_type.letter(),
                            pos: fd.offset + 1,
                            len: fd.byte_len,
                            digits: if fd.data_type.is_numeric() {
                                fd.length
                            } else {
                                0
                            },
                            decimals: u32::from(fd.decimals),
                            text: fd.text.clone().unwrap_or_default(),
                            headings: [
                                fd.colhdg.first().cloned().unwrap_or_default(),
                                fd.colhdg.get(1).cloned().unwrap_or_default(),
                                fd.colhdg.get(2).cloned().unwrap_or_default(),
                            ],
                        })
                        .collect(),
                })
                .collect())
        })
        .unwrap_or_default()
    }

    fn dependents(&mut self, lib: &str, name: &str, _member: &str) -> Vec<DepInfo> {
        self.run(name, lib, types::FILE, |txn, id| {
            let d = db::file::describe(txn, id).map_err(|e| e.to_string())?;
            Ok(d.dependents
                .iter()
                .map(|x| DepInfo {
                    name: x.lf_file.to_string(),
                    lib: String::new(),
                    member: x.lf_member.to_string(),
                    kind: 'L',
                })
                .collect())
        })
        .unwrap_or_default()
    }

    fn user_info(&mut self, name: &str) -> Option<sysapi::UserInfo> {
        (name == "QSYS" || name == "QSECOFR").then(|| sysapi::UserInfo {
            name: name.into(),
            status: "*ENABLED".into(),
            class: "*SECOFR".into(),
            special: vec!["*ALLOBJ".into(), "*SECADM".into()],
            text: "Service profile".into(),
        })
    }
}
