// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The APIs' provider, answered by the service. `QCMDEXC` stays in the
//! job: the commands queue here and the job runs them after the call
//! (`MIGRATE(phase7)`: the command analyzer runs them in place).

use sysapi::{
    ApiFail, ApiResult, CommitInfo, DataArea, DataAreaType, DepInfo, DtaqEntry, DtaqInfo, FileInfo,
    FormatInfo, IdxAttr, LocalNow, MemberInfo, ObjInfo, Provider, SysVal, UserInfo,
};

use crate::Shared;
use crate::proto::Client;

/// The provider.
pub struct RemoteProvider {
    link: Shared,
    lda: super::data::Lda,
    /// Commands `QCMDEXC` queued, oldest first.
    pub pending: Vec<String>,
}

impl RemoteProvider {
    /// Over a link.
    #[must_use]
    pub fn new(link: Shared, lda: super::data::Lda) -> Self {
        Self {
            link,
            lda,
            pending: Vec::new(),
        }
    }
}

impl Provider for RemoteProvider {
    fn sec(&mut self, q: sec::api::Query) -> Result<sec::api::Answer, ApiFail> {
        self.link.borrow_mut().with(|l| l.sec_api(q))
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

    fn exec_command(&mut self, text: &str) -> ApiResult<()> {
        if text.trim().is_empty() {
            return Err(ApiFail::id("CPF0006"));
        }
        self.pending.push(text.trim().to_string());
        Ok(())
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
