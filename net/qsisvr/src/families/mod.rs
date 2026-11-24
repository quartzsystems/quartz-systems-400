// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The families a standalone system answers (`caps::STANDALONE`): base
//! with its dumps, dump space and fix level, service with its notes,
//! the panel and the console at ID 0. Subscriptions and pairing are the
//! connection's; everything else is refused as not agreed.

pub mod panel;

use qsi::any::{AnyReply, AnyRequest};
use qsi::base::{self, DumpKind};
use qsi::host::model::SYSTEM_ID;
use qsi::host::{console, panel as qpanel, partitions};
use qsi::{Empty, msgid};

use crate::host::{Host, Refusal, refusal};

/// How a request was answered.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // a reply is passed whole, once
pub enum Answer {
    /// A reply.
    Reply(AnyReply),
    /// Refused, with every message that applies.
    Refused(Refusal),
    /// A stream to open, then the reply.
    Open(Open),
}

/// A stream a request opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Open {
    /// The system's console.
    Console {
        /// In place of another viewer.
        take_over: bool,
    },
    /// A dump's bytes.
    Dump {
        /// Its name.
        name: String,
    },
}

/// The words of a system dump's kind.
pub const DUMP_KIND: &str = "system dump";

fn reply(r: Result<AnyReply, Refusal>) -> Answer {
    match r {
        Ok(r) => Answer::Reply(r),
        Err(n) => Answer::Refused(n),
    }
}

fn no_partition(id: u32) -> Answer {
    Answer::Refused(refusal(
        partitions::msgid::NO_PARTITION,
        &format!("A standalone system has no partition {id}: it answers for itself at ID 0."),
    ))
}

/// Answer `request` of a pinned console. `user` is for the log; it
/// grants nothing.
pub fn answer(host: &mut dyn Host, request: AnyRequest, user: &str) -> Answer {
    match request {
        AnyRequest::Base(b) => base(host, b, user),
        AnyRequest::Service(s) => reply(host.problems(&s, user).map(AnyReply::Service)),
        AnyRequest::Panel(p) => match p {
            qpanel::Request::Function(f) if f.id != SYSTEM_ID => no_partition(f.id),
            qpanel::Request::Functions(f) if f.id != SYSTEM_ID => no_partition(f.id),
            qpanel::Request::Function(f) => reply(
                panel::run(host, f.function, user)
                    .map(|text| AnyReply::Panel(qpanel::Reply::Function(qpanel::Shown { text }))),
            ),
            qpanel::Request::Functions(_) => {
                Answer::Reply(AnyReply::Panel(qpanel::Reply::Functions(panel::list())))
            }
        },
        AnyRequest::Console(c) => match c {
            console::Request::Attach(a) if a.id != SYSTEM_ID => no_partition(a.id),
            console::Request::Screen(s) if s.id != SYSTEM_ID => no_partition(s.id),
            console::Request::Attach(a) => Answer::Open(Open::Console {
                take_over: a.take_over,
            }),
            console::Request::Screen(_) => reply(
                host.screen()
                    .map(|c| AnyReply::Console(console::Reply::Screen(c))),
            ),
        },
        _ => Answer::Refused(refusal(
            msgid::NOT_AGREED,
            "A standalone system does not answer this family.",
        )),
    }
}

fn base(host: &mut dyn Host, request: base::Request, user: &str) -> Answer {
    use base::Reply as R;
    let r = match request {
        base::Request::Identify(_) => host.identity().map(R::Identify),
        base::Request::State(_) => host.state().map(R::State),
        base::Request::Power(p) => host.power(&p, user).map(|()| R::Power(Empty)),
        base::Request::Refcodes(q) => host.refcodes(q.before, q.limit).map(R::Refcodes),
        base::Request::Time(_) => host.time().map(R::Time),
        base::Request::FixLevel(_) => host.fix_level().map(R::FixLevel),
        base::Request::Resources(_) => host.resources().map(R::Resources),
        base::Request::Dumps(_) => host.dumps().map(R::Dumps),
        base::Request::TakeDump(t) => {
            if t.kind != DumpKind::ServiceData {
                return Answer::Refused(refusal(
                    base::msgid::NO_KIND,
                    "This system takes one kind of dump: its service data (a system dump).",
                ));
            }
            host.take_dump("Asked for by a management console", user)
                .map(|name| R::TakeDump(base::DumpBegun { name }))
        }
        base::Request::DeleteDump(d) => host
            .delete_dump(&d.name, user)
            .map(|()| R::DeleteDump(Empty)),
        base::Request::DownloadDump(d) => return Answer::Open(Open::Dump { name: d.name }),
        base::Request::DumpSpace(_) => host.dump_space().map(R::DumpSpace),
        // The connection answers these; reaching here is a mistake.
        base::Request::Subscribe(_) | base::Request::Unsubscribe(_) => {
            return Answer::Refused(refusal(
                msgid::UNKNOWN,
                "The request was not expected here.",
            ));
        }
    };
    reply(r.map(AnyReply::Base))
}
