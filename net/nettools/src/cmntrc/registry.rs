// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system's communications traces, one per configuration object
//! and type, and the rules of `STRCMNTRC`, `ENDCMNTRC`, `DLTCMNTRC`
//! and `CHKCMNTRC`: a trace exists from its start until it is deleted,
//! collects until it is ended (or fills with `TRCFULL(*STOPTRC)`), and
//! must be ended before it is deleted. The capture itself runs
//! elsewhere (the storage service's thread) against an [`Entry`]'s
//! trace and stop flag.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::{CfgType, Header, Status, Trace};
use crate::msg::{self, Kind, Msg};

/// A trace and the flag that stops its capture.
#[derive(Clone, Debug)]
pub struct Entry {
    /// The trace.
    pub trace: Arc<Mutex<Trace>>,
    /// Set to end the capture.
    pub stop: Arc<AtomicBool>,
}

impl Entry {
    /// The trace, locked.
    pub fn lock(&self) -> MutexGuard<'_, Trace> {
        self.trace.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn key(&self) -> (String, CfgType) {
        let t = self.lock();
        (t.header.cfgobj.clone(), t.header.cfgtype)
    }
}

/// The traces.
#[derive(Debug, Default)]
pub struct Registry {
    entries: Vec<Entry>,
}

fn what(obj: &str, ty: CfgType) -> String {
    format!("{obj} type {}", ty.name())
}

impl Registry {
    /// The trace of `obj` and `ty`.
    pub fn get(&self, obj: &str, ty: CfgType) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.key() == (obj.to_ascii_uppercase(), ty))
    }

    /// Every trace, in start order.
    pub fn all(&self) -> &[Entry] {
        &self.entries
    }

    /// `STRCMNTRC`: a new active trace, or `CPF39AA` when the object
    /// already has one (ended or not: `DLTCMNTRC` first).
    pub fn start(&mut self, mut h: Header) -> Result<(Entry, Msg), Msg> {
        h.cfgobj = h.cfgobj.to_ascii_uppercase();
        if self.get(&h.cfgobj, h.cfgtype).is_some() {
            return Err(Msg::new(
                msg::TRC_EXISTS,
                Kind::Escape,
                format!("A trace of {} exists; delete it before starting another.", what(&h.cfgobj, h.cfgtype)),
            ));
        }
        h.status = Status::Active;
        h.ended = None;
        let done = Msg::new(
            msg::TRC_STARTED,
            Kind::Comp,
            format!("Communications trace of {} started.", what(&h.cfgobj, h.cfgtype)),
        );
        let e = Entry {
            trace: Arc::new(Mutex::new(Trace::new(h))),
            stop: Arc::new(AtomicBool::new(false)),
        };
        self.entries.push(e.clone());
        Ok((e, done))
    }

    fn missing(obj: &str, ty: CfgType) -> Msg {
        Msg::new(
            msg::TRC_NOT_FOUND,
            Kind::Escape,
            format!("There is no communications trace of {}.", what(obj, ty)),
        )
    }

    /// `ENDCMNTRC` at `now` (microseconds): the capture stops and the
    /// data stays.
    pub fn end(&mut self, obj: &str, ty: CfgType, now: u64) -> Result<Msg, Msg> {
        let obj = obj.to_ascii_uppercase();
        let e = self.get(&obj, ty).ok_or_else(|| Self::missing(&obj, ty))?;
        e.stop.store(true, Ordering::SeqCst);
        let mut t = e.lock();
        match t.header.status {
            Status::Inactive => Err(Msg::new(
                msg::TRC_ALREADY_ENDED,
                Kind::Escape,
                format!("The communications trace of {} has already ended.", what(&obj, ty)),
            )),
            Status::Active | Status::Stopped | Status::Failed => {
                t.header.status = Status::Inactive;
                if t.header.ended.is_none() {
                    t.header.ended = Some(now);
                }
                Ok(Msg::new(
                    msg::TRC_ENDED,
                    Kind::Comp,
                    format!("Communications trace of {} ended.", what(&obj, ty)),
                ))
            }
        }
    }

    /// `DLTCMNTRC`: the trace and its data go; an active trace must be
    /// ended first.
    pub fn delete(&mut self, obj: &str, ty: CfgType) -> Result<Msg, Msg> {
        let obj = obj.to_ascii_uppercase();
        let i = self
            .entries
            .iter()
            .position(|e| e.key() == (obj.clone(), ty))
            .ok_or_else(|| Self::missing(&obj, ty))?;
        if self.entries[i].lock().header.status == Status::Active {
            return Err(Msg::new(
                msg::TRC_CANNOT_DELETE,
                Kind::Escape,
                format!("The communications trace of {} is active; end it before deleting it.", what(&obj, ty)),
            ));
        }
        let e = self.entries.remove(i);
        e.stop.store(true, Ordering::SeqCst);
        Ok(Msg::new(
            msg::TRC_DELETED,
            Kind::Comp,
            format!("Communications trace of {} deleted.", what(&obj, ty)),
        ))
    }

    /// `CHKCMNTRC CFGOBJ CFGTYPE`: a status message per trace (`None`
    /// is `*ALL`), then the completion.
    pub fn check(&self, obj: Option<&str>, ty: Option<CfgType>) -> Result<Vec<Msg>, Msg> {
        if self.entries.is_empty() {
            return Err(Msg::new(msg::TRC_NONE, Kind::Escape, "There are no communications traces."));
        }
        let obj = obj.map(str::to_ascii_uppercase);
        let mut out = Vec::new();
        for e in &self.entries {
            let t = e.lock();
            let h = &t.header;
            if obj.as_ref().is_some_and(|o| *o != h.cfgobj) || ty.is_some_and(|x| x != h.cfgtype) {
                continue;
            }
            out.push(Msg::new(
                msg::TRC_STATUS,
                Kind::Info,
                format!("Communications trace {} type {} is {}.", h.cfgobj, h.cfgtype.name(), h.status.name()),
            ));
        }
        if out.is_empty() {
            return Err(match (obj, ty) {
                (Some(o), Some(t)) => Self::missing(&o, t),
                (_, Some(t)) => Msg::new(
                    msg::TRC_NONE_OF_TYPE,
                    Kind::Escape,
                    format!("There are no communications traces of type {}.", t.name()),
                ),
                _ => Msg::new(msg::TRC_NONE, Kind::Escape, "There are no communications traces."),
            });
        }
        out.push(Msg::new(msg::TRC_CHECKED, Kind::Comp, "Communications traces checked."));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmntrc::tests::header;

    #[test]
    fn start_end_delete() {
        let mut r = Registry::default();
        let (e, m) = r.start(Header { cfgobj: "ethline".into(), ..header() }).unwrap();
        assert_eq!(m.id, "CPF39A0");
        assert_eq!(e.lock().header.cfgobj, "ETHLINE");
        assert_eq!(r.start(header()).unwrap_err().id, "CPF39AA");
        assert_eq!(r.delete("ETHLINE", CfgType::Line).unwrap_err().id, "CPF39B2");
        assert_eq!(r.end("ETHLINE", CfgType::Line, 9).unwrap().id, "CPF39A1");
        assert!(e.stop.load(Ordering::SeqCst));
        assert_eq!(e.lock().header.ended, Some(9));
        assert_eq!(r.end("ETHLINE", CfgType::Line, 10).unwrap_err().id, "CPF39AE");
        assert_eq!(r.end("ETHLINE", CfgType::Nwi, 10).unwrap_err().id, "CPF39B1");
        assert_eq!(r.delete("ETHLINE", CfgType::Line).unwrap().id, "CPF39A2");
        assert!(r.get("ETHLINE", CfgType::Line).is_none());
    }

    #[test]
    fn check_lists_statuses() {
        let mut r = Registry::default();
        assert_eq!(r.check(None, None).unwrap_err().id, "CPF39B0");
        r.start(header()).unwrap();
        r.start(Header { cfgobj: "LOOP".into(), ..header() }).unwrap();
        r.end("LOOP", CfgType::Line, 1).unwrap();
        let m = r.check(None, None).unwrap();
        let ids: Vec<&str> = m.iter().map(|x| x.id).collect();
        assert_eq!(ids, ["CPF39A5", "CPF39A5", "CPF39A4"]);
        assert!(m[1].text.contains("LOOP type *LIN is Inactive"));
        assert_eq!(r.check(Some("loop"), None).unwrap().len(), 2);
        assert_eq!(r.check(None, Some(CfgType::Nws)).unwrap_err().id, "CPF39BE");
        assert_eq!(r.check(Some("X"), Some(CfgType::Line)).unwrap_err().id, "CPF39B1");
    }

    #[test]
    fn a_stopped_trace_can_be_ended() {
        let mut r = Registry::default();
        let (e, _) = r.start(Header { wrap: false, maxstg: 10, ..header() }).unwrap();
        assert!(!e.lock().push(5, true, &[0; 64]));
        assert_eq!(e.lock().header.status, Status::Stopped);
        assert_eq!(r.end("ETHLINE", CfgType::Line, 8).unwrap().id, "CPF39A1");
        assert_eq!(e.lock().header.ended, Some(5), "the time it filled stays");
    }
}
