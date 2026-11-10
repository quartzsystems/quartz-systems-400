// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QAP's `NetServer` service (version 10, draft) on a connection job's
//! secondary thread, as the signed-on profile: the `QZLS*` APIs, the
//! session table and the disabled users through the storage service
//! (`work::nsadmin`), so the APIs check that profile's authority.

#![allow(clippy::doc_markdown)] // NetServer

use netsvrcfg::client::Failure;
use qap_proto::ServerMessage;
use qapd::netserver::NetServerHost;

use super::host::{JobThread, message_bytes};
use crate::work::nsadmin;

impl NetServerHost for JobThread {
    fn api(&mut self, api: &str, parms: Vec<Vec<u8>>) -> Result<Vec<u8>, Failure> {
        nsadmin::call(&mut self.link(), api, parms)
    }

    fn sessions(&mut self) -> Vec<netsvrcfg::Session> {
        nsadmin::sessions(&mut self.link())
    }

    fn disabled(&mut self) -> Vec<(String, u64)> {
        nsadmin::disabled(&mut self.link())
    }

    fn server_job(&mut self) -> Option<String> {
        nsadmin::server(&mut self.link())
    }

    fn message(&mut self, id: &str, data: &[u8]) -> ServerMessage {
        message_bytes(&mut self.link(), id, data)
    }
}
