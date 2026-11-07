// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The APIs' provider, answered by the service. `QCMDEXC` is the
//! job's own: its hook on the runtime runs the command in place, so
//! the provider's `exec_command` is never reached.

use sysapi::{
    ApiFail, ApiResult, CommitInfo, DataArea, DataAreaType, DepInfo, DtaqEntry, DtaqInfo, FileInfo,
    FormatInfo, IdxAttr, LocalNow, MemberInfo, ObjInfo, Provider, QueueMsg, SysVal, UserInfo,
};

use wm::obj::msgq::{MsgKind, QMsg};

use crate::Shared;
use crate::proto::Client;

/// The provider.
pub struct RemoteProvider {
    link: Shared,
    lda: super::data::Lda,
}

impl RemoteProvider {
    /// Over a link.
    #[must_use]
    pub fn new(link: Shared, lda: super::data::Lda) -> Self {
        Self { link, lda }
    }
}

/// A message queue's kind by its `*MSGQ` name.
fn kind_of(name: &str) -> MsgKind {
    match name {
        "*INQ" => MsgKind::Inq,
        "*RPY" => MsgKind::Rpy,
        "*COMP" => MsgKind::Comp,
        "*DIAG" => MsgKind::Diag,
        "*NOTIFY" => MsgKind::Notify,
        "*ESCAPE" => MsgKind::Escape,
        "*COPY" => MsgKind::Copy,
        _ => MsgKind::Info,
    }
}

fn kind_name(k: MsgKind) -> &'static str {
    match k {
        MsgKind::Info => "*INFO",
        MsgKind::Inq => "*INQ",
        MsgKind::Rpy => "*RPY",
        MsgKind::Comp => "*COMP",
        MsgKind::Diag => "*DIAG",
        MsgKind::Notify => "*NOTIFY",
        MsgKind::Escape => "*ESCAPE",
        MsgKind::Copy => "*COPY",
    }
}

fn queue_msg(m: QMsg) -> QueueMsg {
    QueueMsg {
        key: m.key,
        kind: kind_name(m.kind).into(),
        severity: m.severity,
        msgid: m.msgid,
        msgf: m.msgf,
        data: m.data,
        text: m.text,
        help: m.help,
        reply: m.reply,
        job: m.job,
        from_pgm: m.from_pgm,
        sent_us: m.time,
    }
}

/// A service refusal (`CPFnnnn text`) as an API's.
fn msgq_fail(e: &str, lib: &str, name: &str) -> ApiFail {
    match e.split_whitespace().next() {
        Some(id) if id.len() == 7 && id.starts_with("CP") && id != "CPF2403" => {
            ApiFail::id(id).text(name, 10).text(lib, 10)
        }
        _ => ApiFail::id("CPF2403").text(name, 10).text(lib, 10),
    }
}

impl Provider for RemoteProvider {
    fn signed_on_users(&mut self) -> ApiResult<Vec<String>> {
        let jobs = self.link.borrow_mut().with(Client::active_jobs);
        let mut users: Vec<String> = jobs
            .into_iter()
            .filter(|j| j.kind == 'I')
            .map(|j| j.user)
            .collect();
        users.sort();
        users.dedup();
        Ok(users)
    }

    fn msgq_send(&mut self, lib: &str, name: &str, m: QueueMsg) -> ApiResult<u32> {
        let q = QMsg {
            kind: kind_of(&m.kind),
            severity: m.severity,
            msgid: m.msgid,
            msgf: m.msgf,
            data: m.data,
            text: m.text,
            help: m.help,
            ..QMsg::default()
        };
        self.link
            .borrow_mut()
            .with(|l| l.msgq_send(format!("{lib}/{name}"), q))
            .map_err(|e| msgq_fail(&e, lib, name))
    }

    fn msgq_receive(
        &mut self,
        lib: &str,
        name: &str,
        kind: Option<&str>,
        key: Option<u32>,
        remove: bool,
    ) -> ApiResult<Option<QueueMsg>> {
        self.link
            .borrow_mut()
            .with(|l| l.msgq_receive(format!("{lib}/{name}"), kind.map(kind_of), key, remove))
            .map(|m| m.map(queue_msg))
            .map_err(|e| msgq_fail(&e, lib, name))
    }

    fn msgq_list(&mut self, lib: &str, name: &str) -> ApiResult<Vec<QueueMsg>> {
        self.link
            .borrow_mut()
            .with(|l| l.msgq_list(format!("{lib}/{name}")))
            .map(|v| v.into_iter().map(queue_msg).collect())
            .map_err(|e| msgq_fail(&e, lib, name))
    }

    fn licpgm(&mut self, api: &str, parms: Vec<Vec<u8>>) -> Result<Vec<u8>, ApiFail> {
        self.link
            .borrow_mut()
            .with(|l| l.licpgm_api(api.into(), parms))
            .map_err(|(msgid, data)| ApiFail { msgid, data })
    }

    fn hardware(&mut self) -> Result<hwrsrc::Table, ApiFail> {
        self.link
            .borrow_mut()
            .with(|l| l.hw_table(false))
            .map_err(|e| ApiFail::id("CPF9898").text(e.trim_start_matches("CPF9898 "), 512))
    }

    fn sec(&mut self, q: sec::api::Query) -> Result<sec::api::Answer, ApiFail> {
        self.link.borrow_mut().with(|l| l.sec_api(q))
    }

    fn regfac(&mut self, q: regfac::Query) -> Result<regfac::Answer, ApiFail> {
        self.link
            .borrow_mut()
            .with(|l| l.regfac(q))
            .map_err(|f| ApiFail {
                msgid: f.msgid,
                data: f.data,
            })
    }

    fn list_objects(&mut self, lib: &str, name: &str, ty: &str) -> Vec<ObjInfo> {
        self.link
            .borrow_mut()
            .with(|l| l.list_objects(lib.into(), name.into(), ty.into()))
    }

    fn object(&mut self, lib: &str, name: &str, ty: &str) -> Option<ObjInfo> {
        self.link
            .borrow_mut()
            .with(|l| l.object(lib.into(), name.into(), ty.into()))
    }

    fn change_object(
        &mut self,
        lib: &str,
        name: &str,
        ty: &str,
        text: Option<String>,
        user_attr: Option<String>,
    ) -> ApiResult<String> {
        self.link
            .borrow_mut()
            .with(|l| l.change_object(lib.into(), name.into(), ty.into(), text, user_attr))
    }

    fn rename_object(
        &mut self,
        lib: &str,
        name: &str,
        ty: &str,
        new_lib: &str,
        new_name: &str,
        replace: bool,
    ) -> ApiResult<String> {
        self.link.borrow_mut().with(|l| {
            l.rename_object(
                lib.into(),
                name.into(),
                ty.into(),
                new_lib.into(),
                new_name.into(),
                replace,
            )
        })
    }

    fn usridx_create(&mut self, lib: &str, name: &str, attr: IdxAttr) -> ApiResult<()> {
        self.link
            .borrow_mut()
            .with(|l| l.usridx_create(lib.into(), name.into(), attr))
    }

    fn usridx_delete(&mut self, lib: &str, name: &str) -> ApiResult<()> {
        self.link
            .borrow_mut()
            .with(|l| l.usridx_delete(lib.into(), name.into()))
    }

    fn usridx_add(
        &mut self,
        lib: &str,
        name: &str,
        entries: Vec<Vec<u8>>,
        replace: bool,
    ) -> ApiResult<usize> {
        self.link
            .borrow_mut()
            .with(|l| l.usridx_add(lib.into(), name.into(), entries, replace))
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
        self.link.borrow_mut().with(|l| {
            l.usridx_find(
                lib.into(),
                name.into(),
                rule,
                key.to_vec(),
                key2.to_vec(),
                max,
                remove,
            )
        })
    }

    fn usridx_attrs(&mut self, lib: &str, name: &str) -> ApiResult<(IdxAttr, usize, String)> {
        self.link
            .borrow_mut()
            .with(|l| l.usridx_attrs(lib.into(), name.into()))
    }

    fn dtaq_send(
        &mut self,
        lib: &str,
        name: &str,
        key: &[u8],
        data: &[u8],
        sender: &str,
    ) -> ApiResult<()> {
        self.link.borrow_mut().with(|l| {
            l.api_dtaq_send(
                lib.into(),
                name.into(),
                key.to_vec(),
                data.to_vec(),
                sender.into(),
            )
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
        self.link.borrow_mut().with(|l| {
            l.api_dtaq_receive(lib.into(), name.into(), order.into(), key.to_vec(), remove)
        })
    }

    fn dtaq_clear(&mut self, lib: &str, name: &str, order: &str, key: &[u8]) -> ApiResult<()> {
        self.link
            .borrow_mut()
            .with(|l| l.api_dtaq_clear(lib.into(), name.into(), order.into(), key.to_vec()))
    }

    fn dtaq_describe(&mut self, lib: &str, name: &str) -> ApiResult<DtaqInfo> {
        self.link
            .borrow_mut()
            .with(|l| l.api_dtaq_describe(lib.into(), name.into()))
    }

    fn dtaq_entries(&mut self, lib: &str, name: &str) -> ApiResult<Vec<DtaqEntry>> {
        self.link
            .borrow_mut()
            .with(|l| l.api_dtaq_entries(lib.into(), name.into()))
    }

    fn dtaara_read(&mut self, lib: &str, name: &str) -> ApiResult<DataArea> {
        if name == "*GDA" {
            let value = self
                .link
                .borrow_mut()
                .with(Client::gda_read)
                .map_err(|_| ApiFail::id("CPF1311"))?;
            return Ok(DataArea {
                ty: DataAreaType::Char,
                lib: String::new(),
                len: u32::try_from(value.len()).unwrap_or(512),
                decimals: 0,
                value,
            });
        }
        if name == "*LDA" {
            return Ok(DataArea {
                ty: DataAreaType::Char,
                lib: String::new(),
                len: 1024,
                decimals: 0,
                value: self.lda.borrow().clone(),
            });
        }
        self.link
            .borrow_mut()
            .with(|l| l.api_dtaara_read(lib.into(), name.into()))
    }

    fn dtaara_write(&mut self, lib: &str, name: &str, start: usize, bytes: &[u8]) -> ApiResult<()> {
        if name == "*GDA" {
            let at = u32::try_from(start).unwrap_or(1);
            return self
                .link
                .borrow_mut()
                .with(|l| l.gda_write(at, bytes.to_vec()))
                .map_err(|_| ApiFail::id("CPF1311"));
        }
        if name == "*LDA" {
            let mut lda = self.lda.borrow_mut();
            let at = start.saturating_sub(1);
            if at + bytes.len() > lda.len() {
                return Err(ApiFail::id("CPF1088"));
            }
            lda[at..at + bytes.len()].copy_from_slice(bytes);
            return Ok(());
        }
        self.link
            .borrow_mut()
            .with(|l| l.api_dtaara_write(lib.into(), name.into(), start, bytes.to_vec()))
    }

    fn sysval(&mut self, name: &str) -> Option<SysVal> {
        self.link.borrow_mut().with(|l| l.sysval(name.into()))
    }

    fn netattr(&mut self, name: &str) -> Option<SysVal> {
        self.link.borrow_mut().with(|l| l.netattr(name.into()))
    }

    fn system_status(&mut self, format: &str, reset: bool) -> sysapi::ApiResult<Vec<u8>> {
        self.link
            .borrow_mut()
            .with(|l| l.sys_status(format.into(), reset))
            .map_err(|_| ApiFail::id("CPF3C21"))
    }

    fn now(&mut self) -> LocalNow {
        self.link.borrow_mut().with(Client::now)
    }

    fn exec_command(&mut self, _text: &str) -> ApiResult<()> {
        Err(ApiFail::id("CPF0006"))
    }

    fn check_command(&mut self, text: &str) -> ApiResult<()> {
        self.link
            .borrow_mut()
            .with(|l| l.check_command(text.into()))
    }

    fn file(&mut self, lib: &str, name: &str) -> Option<FileInfo> {
        self.link
            .borrow_mut()
            .with(|l| l.file(lib.into(), name.into()))
    }

    fn members(&mut self, lib: &str, name: &str) -> Vec<MemberInfo> {
        self.link
            .borrow_mut()
            .with(|l| l.members(lib.into(), name.into()))
    }

    fn formats(&mut self, lib: &str, name: &str) -> Vec<FormatInfo> {
        self.link
            .borrow_mut()
            .with(|l| l.formats(lib.into(), name.into()))
    }

    fn dependents(&mut self, lib: &str, name: &str, member: &str) -> Vec<DepInfo> {
        self.link
            .borrow_mut()
            .with(|l| l.dependents(lib.into(), name.into(), member.into()))
    }

    fn commit_info(&mut self) -> Option<CommitInfo> {
        self.link.borrow_mut().with(Client::commit_info)
    }

    fn user_info(&mut self, name: &str) -> Option<UserInfo> {
        self.link.borrow_mut().with(|l| l.user_info(name.into()))
    }
}
