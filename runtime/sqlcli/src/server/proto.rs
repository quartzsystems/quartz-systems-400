// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What crosses between a job and the `QSQSRVR` job serving one of its
//! connections, as frames on the socket pair the descriptor relay
//! carried: the job says who it is with a ticket the storage service
//! gave it, then sends statements; the server answers each.

use sql::Value;
use sql::embed::{Desc, Reply, Request, WireReply, WireValue, from_wire, to_wire};

/// A message to the server job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToServer {
    /// The connection: the client's ticket, and the user and password
    /// `SQLConnect` named (empty for the client's own user).
    Hello {
        /// From the storage service (`sqlsrv_ticket`).
        ticket: Vec<u8>,
        /// The user.
        user: String,
        /// The password.
        password: String,
    },
    /// One statement.
    Run {
        /// Its scope.
        scope: String,
        /// Its description, `Desc::encode`.
        desc: String,
        /// Its inputs.
        inputs: Vec<WireValue>,
    },
    /// The connection ends.
    Bye,
}

ipc::wire_enum!(ToServer {
    0 => Hello { ticket, user, password },
    1 => Run { scope, desc, inputs },
    2 => Bye,
});

/// A message from the server job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FromServer {
    /// Connected: the server job's name.
    Ready {
        /// `number/user/name`.
        job: String,
    },
    /// Not connected.
    Refused {
        /// SQLSTATE.
        state: String,
        /// The native error.
        code: i32,
        /// Why.
        text: String,
    },
    /// A statement's reply.
    Done {
        /// The reply.
        reply: WireReply,
        /// The `OUT` and `INOUT` values by marker.
        outs: Vec<(u32, WireValue)>,
    },
}

ipc::wire_enum!(FromServer {
    0 => Ready { job },
    1 => Refused { state, code, text },
    2 => Done { reply, outs },
});

/// A request as it crosses.
#[must_use]
pub fn run_message(req: &Request) -> ToServer {
    ToServer::Run {
        scope: req.scope.clone(),
        desc: req.desc.encode(),
        inputs: req.inputs.iter().map(to_wire).collect(),
    }
}

/// The request a `Run` carries.
#[must_use]
pub fn request_of(scope: &str, desc: &str, inputs: &[WireValue]) -> Option<Request> {
    Some(Request {
        scope: scope.to_string(),
        desc: Desc::decode(desc)?,
        inputs: inputs.iter().map(from_wire).collect(),
    })
}

/// A reply as it crosses.
#[must_use]
pub fn done_message(reply: &Reply, outs: &[(usize, Value)]) -> FromServer {
    FromServer::Done {
        reply: reply.to_wire(),
        outs: outs
            .iter()
            .map(|(m, v)| (u32::try_from(*m).unwrap_or(u32::MAX), to_wire(v)))
            .collect(),
    }
}

/// The reply a `Done` carries.
#[must_use]
pub fn reply_of(reply: &WireReply, outs: &[(u32, WireValue)]) -> (Reply, Vec<(usize, Value)>) {
    (
        Reply::from_wire(reply),
        outs.iter()
            .map(|(m, v)| (*m as usize, from_wire(v)))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sql::embed::Op;

    #[test]
    fn messages_round_trip() {
        let req = Request {
            scope: "CLI:2".into(),
            desc: Desc {
                op: Op::Execute,
                name: "S3".into(),
                ..Desc::default()
            },
            inputs: vec![Value::int(4), Value::Str("x".into()), Value::Null],
        };
        let m = run_message(&req);
        let back: ToServer = ipc::decode(&ipc::encode(&m)).unwrap();
        let ToServer::Run {
            scope,
            desc,
            inputs,
        } = back
        else {
            panic!("not a run")
        };
        assert_eq!(request_of(&scope, &desc, &inputs).unwrap(), req);
        let hello = ToServer::Hello {
            ticket: vec![1, 2],
            user: String::new(),
            password: String::new(),
        };
        assert_eq!(
            ipc::decode::<ToServer>(&ipc::encode(&hello)).unwrap(),
            hello
        );
        let d = done_message(&Reply::default(), &[(1, Value::int(9))]);
        let FromServer::Done { reply, outs } = ipc::decode::<FromServer>(&ipc::encode(&d)).unwrap()
        else {
            panic!("not done")
        };
        assert_eq!(reply_of(&reply, &outs).1, vec![(1, Value::int(9))]);
    }
}
