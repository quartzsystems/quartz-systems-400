// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job's thread and the connections' threads. The link, the IFS
//! over it and the runtime are the job thread's ([`Main`]); a
//! connection's thread hands it a closure through the [`Bridge`] and
//! waits for the answer, so requests to the service never interleave
//! (running a command as a web user swaps the job's profile for the
//! length of one closure).

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use qsweb::msg::{Kind, Message};
use sec::api::{Answer, Query};
use stg::Shared;

use crate::console::Console;
use crate::work::ifsfiles::IfsFiles;

/// How long a connection waits for the job's thread.
const WAIT: Duration = Duration::from_secs(120);

/// Work for the job's thread.
pub type Task = Box<dyn FnOnce(&mut Main) + Send>;

/// What the job's thread holds.
pub struct Main {
    /// The storage service.
    pub link: Shared,
    /// The IFS over the link, as `httpd`'s files.
    pub files: IfsFiles,
    /// The job's runtime, for commands.
    pub console: Console,
    /// Profile handles (`QSYGETPH`) of the users signed on, by user.
    pub handles: HashMap<String, Vec<u8>>,
}

impl Main {
    /// Over the job's link and runtime.
    pub fn new(link: &Shared, console: Console) -> Self {
        Self {
            link: Rc::clone(link),
            files: IfsFiles::new(link),
            console,
            handles: HashMap::new(),
        }
    }

    /// A security API's answer, its failure as the message ID.
    pub fn sec(&self, q: Query) -> Result<Answer, String> {
        self.ask(|l| stg::Client::sec_api(l, q))
            .map_err(|e| e.msgid)
    }

    /// One request to the service. The link's borrow ends before the
    /// answer is returned, so whatever the caller chains after it may
    /// ask again; `link.borrow_mut().with(..)` in one expression keeps
    /// the borrow to the statement's end, and a second borrow in it
    /// panics.
    pub fn ask<T>(&self, f: impl FnOnce(&mut stg::Link) -> Result<T, ipc::WireError>) -> T {
        self.link.borrow_mut().with(f)
    }

    /// Run `f` with the job swapped to `user`'s profile (the handle
    /// taken at sign-on), then as the job's own user again. `Err` when
    /// the user has no handle here or it could not be set.
    pub fn as_user<T>(&mut self, user: &str, f: impl FnOnce(&mut Self) -> T) -> Result<T, Message> {
        let Some(handle) = self.handles.get(user).cloned() else {
            return Err(Message::new(
                Kind::Error,
                "CPF22E9",
                &format!("User {user} is not signed on to this server. Sign on again."),
            ));
        };
        if let Err(id) = self.sec(Query::SetHandle { handle }) {
            self.handles.remove(user);
            return Err(Message::new(
                Kind::Error,
                &id,
                &format!("The job could not run as user {user}. Sign on again."),
            ));
        }
        let r = f(self);
        self.link.borrow_mut().with(stg::Client::host_reset);
        Ok(r)
    }
}

/// The connections' way to the job's thread.
#[derive(Clone)]
pub struct Bridge {
    tx: Sender<Task>,
}

impl Bridge {
    /// A bridge and the end the job's thread reads.
    pub fn new() -> (Self, Receiver<Task>) {
        let (tx, rx) = mpsc::channel();
        (Self { tx }, rx)
    }

    /// Run `f` on the job's thread; `None` when the job is ending or
    /// does not answer in time.
    pub fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Main) -> T + Send + 'static,
    ) -> Option<T> {
        let (tx, rx) = mpsc::sync_channel(1);
        let task: Task = Box::new(move |m| {
            let _ = tx.send(f(m));
        });
        self.tx.send(task).ok()?;
        rx.recv_timeout(WAIT).ok()
    }
}
