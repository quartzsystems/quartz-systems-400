// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job: what generated code and the runtime library share while a
//! program runs: the pending exception, the call stack and activation
//! groups, the spaces pointers address, the console. A job process
//! (`qsjob`) makes one and sets its sources to the storage service's;
//! tests and the IR kit make one over memory.

use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use codegen::Cond;
use codegen::abi::Env;
use ile::{ActGrpAttr, PgmKind};

use crate::act::{ActivationGroup, Activator, AgKind, ObjectSource, call_entry};
use crate::cond::Exception;
use crate::msg::{ClearMode, Message, MsgQueue, QueueRef};
use crate::space::Spaces;
use qir::MsgType;

/// An inquiry the job must answer.
#[derive(Clone, Debug)]
pub struct Inquiry {
    /// Identifier.
    pub msgid: String,
    /// Message file.
    pub msgf: String,
    /// Message data.
    pub data: Vec<u8>,
    /// First-level text.
    pub text: String,
    /// Second-level text.
    pub help: String,
    /// The default reply.
    pub dft: Option<String>,
    /// Valid replies; empty for any.
    pub valid: Vec<String>,
}

/// Answers an inquiry: the reply, or `None` for the default.
pub type InquiryFn = Box<dyn FnMut(&Inquiry) -> Option<String>>;

/// A program implemented in Rust, called like any other: the job and
/// the parameter addresses.
pub type NativeFn = Rc<dyn Fn(&mut Job, &[u64])>;

/// How a command string is run (`QCMDEXC`), checked (`QCMDCHK`) or
/// prompted (`QCAPCMD`): the job, the text and the mode (0 run, 1
/// check, 2 prompt then run, 3 prompt and return the string); the
/// resulting command string, or the escape message and its data. The
/// job process installs it over the command analyzer.
pub type CmdExecFn = Rc<dyn Fn(&mut Job, &str, u8) -> Result<String, (String, Vec<u8>)>>;

/// A hook the job process installs and the runtime calls with the job.
pub type JobHook = Rc<dyn Fn(&mut Job)>;

/// An open file's owner: the activation (by its static area), the
/// module and the file declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileKey {
    /// Base of the program's static area.
    pub statics: u64,
    /// Module index in the program.
    pub module: u32,
    /// File index in the module.
    pub file: u32,
}

/// An open file's state.
#[derive(Clone, Debug)]
pub struct FileState {
    /// The source's handle.
    pub handle: u32,
    /// Address of the information data structure, 0 for none.
    pub infds: u64,
    /// Its length.
    pub infds_len: usize,
    /// The file's standing.
    pub info: crate::io::FileInfo,
    /// The line printer of an open printer file.
    pub printer: Option<Box<crate::io::spool::PrinterState>>,
    /// An open display file: `handle` is the display source's.
    pub display: bool,
    /// The last operation's feedback bits (`dbfb`): 1 end of file, 2
    /// found, 4 equal.
    pub fb: i32,
}

/// A job's identity: `QSVCJOB QSYS 000001` for the service job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobIdent {
    /// Job name.
    pub name: String,
    /// User profile.
    pub user: String,
    /// Six-digit number.
    pub number: String,
    /// Library list, `QSYS` first.
    pub libl: Vec<String>,
    /// Current library.
    pub curlib: Option<String>,
    /// System name.
    pub system: String,
}

impl Default for JobIdent {
    fn default() -> Self {
        Self {
            name: "QSVCJOB".into(),
            user: "QSYS".into(),
            number: "000001".into(),
            libl: vec!["QSYS".into(), "QGPL".into(), "QTEMP".into()],
            curlib: None,
            system: "QS400".into(),
        }
    }
}

/// A resolved message queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueueTarget {
    Frame(usize),
    Ext,
    SysOpr,
}

/// The job. The `abi` header comes first so the pointer generated code
/// holds is also a pointer to the whole record.
#[repr(C)]
pub struct Job {
    /// The part generated code reads: the pending flag.
    pub abi: codegen::abi::Job,
    /// Who the job is: set by the job process from its start.
    pub ident: JobIdent,
    /// The exception percolating now, if any.
    pub exception: Option<Exception>,
    /// Exceptions handled by a `monitor`, oldest first.
    pub handled: Vec<Exception>,
    /// Every exception raised, oldest first; the dump reads it.
    pub raised: Vec<Exception>,
    /// Commitment control is active (`STRCMTCTL`): RPG's `COMMIT` and
    /// `ROLBK` work only then.
    pub cmtctl: bool,
    /// The next file opened goes under commitment control (RPG's
    /// `COMMIT` keyword, set just before the open).
    pub commit_next: bool,
    /// Call-stack depth: procedures entered and not yet left.
    pub depth: u32,
    /// The deepest the call stack has been.
    pub max_depth: u32,
    /// Automatic storage too large for the native stack: the depth of
    /// the entry that owns it and the bytes.
    pub autos: Vec<(u32, Box<[u8]>)>,
    /// Spaces pointers address.
    pub spaces: Spaces,
    /// Lines `dsply` wrote; the console job shows them
    /// (`MIGRATE(phase8)`: the job's display).
    pub console: Vec<String>,
    /// Replies `dsply` reads, oldest first.
    pub replies: VecDeque<String>,
    /// Run when generated code finds the attention flag set: the job
    /// process takes the break messages waiting for the job.
    pub attn: Option<JobHook>,
    /// Programs, activation groups and the call stack.
    pub act: Activator,
    /// Native programs by name, found before any object (`QSYS`).
    pub natives: HashMap<String, NativeFn>,
    /// The runtime call table.
    pub table: Vec<usize>,
    /// Mapped user spaces.
    pub usrspcs: crate::usrspc::UserSpaces,
    /// Where files are opened; none until the console or a test sets it.
    pub io: Option<Box<dyn crate::io::IoSource>>,
    /// Where printer files' spooled files go.
    pub spool: Option<Box<dyn crate::io::spool::SpoolSink>>,
    /// Where display files show and read their formats.
    pub display: Option<Box<dyn crate::io::display::DisplaySource>>,
    /// Data areas and data queues.
    pub data: Option<Box<dyn crate::io::DataSource>>,
    /// Where embedded SQL statements run; none until a job sets it.
    pub sql: Option<Box<dyn sql::embed::Embedded>>,
    /// Open files by activation, module and file index.
    pub files: HashMap<FileKey, FileState>,
    /// Files passed to procedures: what each handle names.
    pub file_handles: Vec<(FileKey, qir::FileRef)>,
    /// Declared files bound to a caller's (file parameters): the depth
    /// of the procedure that bound it, the declared file, the handle.
    pub file_binds: Vec<(u32, FileKey, usize)>,
    /// The debugger, while `STRDBG` is in effect.
    pub debug: Option<crate::debug::Debugger>,
    /// Where the native stack stood at the first entry call; the stack
    /// check in `enter` measures from it.
    pub stack_base: usize,
    /// Native stack the job may use below that point before `enter`
    /// raises `MCH0601`. A job process sets it from its main thread's
    /// stack; 1 MiB otherwise.
    pub stack_bytes: usize,
    /// The job's external message queue.
    pub ext_msgq: MsgQueue,
    /// `QSYSOPR` for a job over memory; a job process's inquiries go to
    /// the real one through `inquiry`.
    pub sysopr: MsgQueue,
    /// Message descriptions.
    pub msgs: Box<dyn msgf::MsgLookup>,
    /// Replies queued for inquiry messages, oldest first; when empty,
    /// `inquiry` asks, or the description's default answers.
    pub inquiry_replies: VecDeque<String>,
    /// The last dump written (`D` or `F` reply); with a spool sink it is
    /// also spooled as `QPSRVDMP`.
    pub last_dump: Option<String>,
    /// Every reply given to an inquiry, oldest first.
    pub inquiry_log: Vec<String>,
    /// A resume granted to an entry below the top: the exception
    /// clears when the stack comes back down to this depth.
    pub resume_depth: Option<u32>,
    /// The job log: every message sent, every request run.
    pub joblog: wm::joblog::JobLog,
    /// The log filled up under `JOBMSGQFL(*NOWRAP)`: the job must end.
    pub joblog_full: bool,
    /// Entries `*PRTWRAP` pushed out, to print with the job log.
    pub joblog_wrapped: Vec<wm::joblog::Entry>,
    /// Entries logged since the storage service's copy was last brought
    /// up to date; the copy is what is spooled at the end.
    pub log_outbox: Vec<wm::joblog::Entry>,
    /// Who answers an inquiry no queued reply answered: the job's
    /// `INQMSGRPY`. `None` takes the default reply.
    pub inquiry: Option<InquiryFn>,
    /// Who runs command strings for `QCMDEXC` and its kin; without one
    /// the APIs' provider takes them.
    pub cmdexc: Option<CmdExecFn>,
    /// The deepest call level that issued a file override still in
    /// effect; 0 when only command level did.
    pub ovr_top: u32,
    /// Told when the call stack drops below `ovr_top`: that level's
    /// overrides go.
    pub ovr_end: Option<JobHook>,
    /// Descriptors: sockets and IFS files in one numbering.
    pub fds: crate::fd::FdTable,
    /// `errno`, what `__errno()` points at.
    pub errno: crate::fd::ErrnoCell,
    /// `h_errno`, what `__h_errno()` points at.
    pub h_errno: crate::fd::ErrnoCell,
    next_key: u32,
    env: Box<Env>,
}

impl Job {
    /// A new job with its default activation group and nothing pending.
    /// Boxed, because the environment points back into it.
    #[must_use]
    pub fn boxed(source: Box<dyn ObjectSource>) -> Box<Self> {
        let mut j = Box::new(Self {
            abi: codegen::abi::Job::default(),
            ident: JobIdent::default(),
            exception: None,
            handled: Vec::new(),
            raised: Vec::new(),
            cmtctl: false,
            commit_next: false,
            depth: 0,
            max_depth: 0,
            autos: Vec::new(),
            spaces: Spaces::default(),
            console: Vec::new(),
            replies: VecDeque::new(),
            attn: None,
            act: Activator::new(source),
            natives: HashMap::new(),
            table: crate::rtlib::table(),
            usrspcs: crate::usrspc::UserSpaces::default(),
            io: None,
            spool: None,
            display: None,
            data: None,
            sql: None,
            files: HashMap::new(),
            file_handles: Vec::new(),
            file_binds: Vec::new(),
            debug: None,
            stack_base: 0,
            stack_bytes: 1 << 20,
            ext_msgq: MsgQueue::default(),
            sysopr: MsgQueue::default(),
            msgs: Box::new(msgf::BuiltinMsgs::new()),
            inquiry_replies: VecDeque::new(),
            last_dump: None,
            inquiry_log: Vec::new(),
            resume_depth: None,
            joblog: wm::joblog::JobLog::new(16, wm::joblog::Full::Wrap),
            joblog_full: false,
            joblog_wrapped: Vec::new(),
            log_outbox: Vec::new(),
            inquiry: None,
            cmdexc: None,
            ovr_top: 0,
            ovr_end: None,
            fds: crate::fd::FdTable::new(),
            errno: crate::fd::ErrnoCell::default(),
            h_errno: crate::fd::ErrnoCell::default(),
            next_key: 1,
            env: Box::new(Env {
                statics: std::ptr::null_mut(),
                job: std::ptr::null_mut(),
                entry: std::ptr::null_mut(),
            }),
        });
        j.env.job = std::ptr::from_mut(&mut j.abi);
        j
    }

    /// The environment generated code receives.
    pub fn env(&mut self) -> *mut Env {
        std::ptr::from_mut(&mut *self.env)
    }

    /// Whether an exception is pending.
    #[must_use]
    pub fn pending(&self) -> bool {
        self.abi.pending != 0
    }

    /// Clear the pending exception, as a handler outside generated code
    /// does.
    pub fn clear(&mut self) -> Option<Exception> {
        self.abi.pending = 0;
        self.exception.take()
    }

    /// Signal machine condition `cond` to the running entry, as generated
    /// code would.
    pub fn raise(&mut self, cond: Cond) {
        let _ = self.send_message(
            &cond.msgid(),
            "QCPFMSG",
            "QSYS",
            Vec::new(),
            qir::MsgType::Escape,
            crate::msg::QueueRef::Same,
        );
    }

    /// Call a program by name with parameter addresses, as a dynamic
    /// call does: a native program first, else the object, activated
    /// in the group its attribute names. A pending exception the
    /// program did not handle is left on the job.
    pub fn call_program(&mut self, name: &str, lib: &str, args: &[u64]) -> Result<(), Cond> {
        if (lib == "QSYS" || lib == "*LIBL")
            && let Some(f) = self.natives.get(name).cloned()
        {
            // Level 50: a user-state caller passes only user-domain storage.
            if self.act.qsecurity >= 50
                && self.act.user_state()
                && args.iter().any(|a| self.spaces.in_system(*a))
            {
                return Err(Cond::Mch6801);
            }
            f(self, args);
            return Ok(());
        }
        let table = self.table.clone();
        let caller_user = self.act.user_state();
        let p = self.act.load_program(name, lib, PgmKind::Pgm, &table)?;
        // A user-state program may not call a system-state one directly;
        // the published entries are the natives above.
        if self.act.enforcing() && caller_user && !p.object.attr.user_state {
            return Err(Cond::Mch6801);
        }
        let entry = p.entry_addr().ok_or(Cond::Mch3401)?;
        let saved = self.act.current;
        let created = if p.object.attr.actgrp == ActGrpAttr::New {
            Some(self.act.create_group("*NEW", AgKind::New))
        } else {
            None
        };
        let target = created.unwrap_or_else(|| self.act.group_for(&p.object, saved));
        self.act.current = target;
        let env = self.env();
        crate::act::escape(self);
        call_entry(entry, env, args);
        crate::act::escape(self);
        self.act.current = saved;
        if let Some(id) = created {
            self.reclaim_group(id);
        }
        Ok(())
    }

    /// Destroy an activation group by number; its static areas and
    /// heap storage die.
    pub fn reclaim_group(&mut self, id: u32) {
        self.run_group_exits(id);
        if let Ok(bases) = self.act.reclaim_id(id) {
            self.end_sql_group(id);
            self.close_files_of(&bases);
            for b in bases {
                self.spaces.kill_static(b);
                self.spaces.kill_at(b);
            }
        }
    }

    /// An activation group ended: its embedded SQL cursors close and
    /// its prepared statements go (`CLOSQLCSR(*ENDACTGRP)`).
    fn end_sql_group(&mut self, id: u32) {
        if let Some(s) = self.sql.as_mut() {
            let req = sql::embed::Request {
                scope: format!("{id}:"),
                desc: sql::embed::Desc {
                    op: sql::embed::Op::End,
                    ..sql::embed::Desc::default()
                },
                inputs: Vec::new(),
            };
            s.run(&req);
        }
    }

    /// Close the files of activations whose static areas are going away.
    fn close_files_of(&mut self, bases: &[u64]) {
        let keys: Vec<FileKey> = self
            .files
            .keys()
            .filter(|k| bases.contains(&k.statics))
            .copied()
            .collect();
        for k in keys {
            let Some(state) = self.files.remove(&k) else {
                continue;
            };
            if let Some(mut p) = state.printer {
                if let Some(sink) = self.spool.as_mut() {
                    let _ = p.close(sink.as_mut());
                }
            } else if let Some(io) = self.io.as_mut() {
                io.close(state.handle);
            }
        }
    }

    /// Print `text` into a new spooled file of printer file `file`, a
    /// line a record; its spool identifier.
    pub fn spool_text(&mut self, file: &str, text: &str) -> Result<u32, crate::io::IoFail> {
        let Some(sink) = self.spool.as_mut() else {
            return Err(crate::io::IoFail::new(1299, "CPF4101", &[file]));
        };
        let open = sink.open(file, "*LIBL", "*DFTACTGRP")?;
        let id = open.id;
        let mut p = crate::io::spool::PrinterState::new(open);
        for line in text.lines() {
            let rec: Vec<u8> = line
                .chars()
                .map(|c| ccsid::byte_of(c).unwrap_or(0x40))
                .collect();
            p.write(sink.as_mut(), "", &rec)?;
        }
        p.close(sink.as_mut())?;
        Ok(id)
    }

    /// Close every open printer file: the job is ending, and what they
    /// printed must reach their spooled files.
    pub fn close_printers(&mut self) {
        let keys: Vec<FileKey> = self
            .files
            .iter()
            .filter(|(_, s)| s.printer.is_some())
            .map(|(k, _)| *k)
            .collect();
        for k in keys {
            if let Some(mut p) = self.files.remove(&k).and_then(|s| s.printer)
                && let Some(sink) = self.spool.as_mut()
            {
                let _ = p.close(sink.as_mut());
            }
        }
    }

    /// Call a group's exit procedures (`CEE4RAGE`), newest first, with
    /// the group mark, the reason (1: `RCLACTGRP` or a normal end),
    /// a result code and a user return code, all by reference.
    fn run_group_exits(&mut self, id: u32) {
        let exits: Vec<u64> = self
            .act
            .groups
            .get(id as usize)
            .filter(|g| g.live)
            .map(|g| g.exits.clone())
            .unwrap_or_default();
        for code in exits.iter().rev() {
            let mut mark = id.to_be_bytes();
            let mut reason = 1i32.to_be_bytes();
            let mut result = [0u8; 4];
            let mut user_rc = [0u8; 4];
            let env = self.env();
            crate::act::escape(self);
            crate::act::call_handler(
                usize::try_from(*code).unwrap_or(0),
                env,
                [
                    mark.as_mut_ptr() as u64,
                    reason.as_mut_ptr() as u64,
                    result.as_mut_ptr() as u64,
                    user_rc.as_mut_ptr() as u64,
                ],
                4,
            );
        }
    }

    /// `%ALLOC` from the current group's default heap.
    pub fn alloc(&mut self, n: u64) -> Result<crate::space::FatPtr, Cond> {
        self.alloc_from(0, n)
    }

    /// `CEEGTST`: storage from a heap of the current group.
    pub fn alloc_from(
        &mut self,
        heap: crate::heap::HeapId,
        n: u64,
    ) -> Result<crate::space::FatPtr, Cond> {
        let ag = self.act.current as usize;
        let h = self.act.groups[ag]
            .heaps
            .iter_mut()
            .find(|h| h.id == heap)
            .ok_or(Cond::Mch3601)?;
        Ok(h.alloc(n, &mut self.spaces))
    }

    /// `%REALLOC` and `CEECZST`: the block resized, contents kept.
    pub fn realloc(
        &mut self,
        p: crate::space::FatPtr,
        n: u64,
    ) -> Result<crate::space::FatPtr, Cond> {
        let (space, _) = self.spaces.space_info(p)?;
        for g in &mut self.act.groups {
            for h in &mut g.heaps {
                if let Some(q) = h.realloc(space, n, &mut self.spaces) {
                    return Ok(q);
                }
            }
        }
        Err(Cond::Mch3601)
    }

    /// `DEALLOC` and `CEEFRST`: a block freed.
    pub fn dealloc(&mut self, p: crate::space::FatPtr) -> Result<(), Cond> {
        let (space, _) = self.spaces.space_info(p)?;
        for g in &mut self.act.groups {
            for h in &mut g.heaps {
                if h.free(space, &mut self.spaces) {
                    return Ok(());
                }
            }
        }
        Err(Cond::Mch3601)
    }

    /// `CEECRHP`: a user heap in the current group; its number.
    pub fn create_heap(&mut self) -> crate::heap::HeapId {
        let ag = self.act.current as usize;
        let g = &mut self.act.groups[ag];
        let id = g.heaps.iter().map(|h| h.id).max().map_or(1, |m| m + 1);
        g.heaps.push(crate::heap::Heap::new(id));
        id
    }

    /// `CEEDSHP`: a user heap of the current group discarded with
    /// everything in it; the default heap (0) cannot be.
    pub fn discard_heap(&mut self, heap: crate::heap::HeapId) -> Result<(), Cond> {
        if heap == 0 {
            return Err(Cond::Mch3601);
        }
        let ag = self.act.current as usize;
        let g = &mut self.act.groups[ag];
        let i = g
            .heaps
            .iter()
            .position(|h| h.id == heap)
            .ok_or(Cond::Mch3601)?;
        let h = g.heaps.remove(i);
        for b in &h.blocks {
            self.spaces.kill(b.space);
        }
        Ok(())
    }

    /// `QUSPTRUS`: a space pointer to a user space, mapped on first
    /// use; writes reach the object at [`Job::sync_user_spaces`].
    pub fn map_user_space(&mut self, name: &str, lib: &str) -> Result<crate::space::FatPtr, Cond> {
        self.usrspcs.map(name, lib, &mut self.spaces)
    }

    /// Write every changed user space back to its object.
    pub fn sync_user_spaces(&mut self) {
        self.usrspcs.sync();
    }

    /// `RCLACTGRP(name)`.
    pub fn rclactgrp(&mut self, name: &str) -> Result<(), &'static str> {
        let id = self
            .act
            .groups()
            .find(|g| g.kind == AgKind::Named && g.name == name)
            .map(|g| g.id)
            .ok_or("no such activation group")?;
        let bases = self.act.reclaim_id(id)?;
        self.end_sql_group(id);
        for b in bases {
            self.spaces.kill_static(b);
        }
        Ok(())
    }

    /// `RCLRSC`: close the files left open by programs of the default
    /// activation group that are no longer on the call stack; how many
    /// closed.
    pub fn rclrsc(&mut self) -> usize {
        let active: Vec<String> = self
            .act
            .frames
            .iter()
            .filter_map(|f| f.program.as_ref().map(|p| p.key.clone()))
            .collect();
        let bases: Vec<u64> = self
            .act
            .groups()
            .filter(|g| g.kind == AgKind::Default)
            .flat_map(|g| g.activations.iter())
            .filter(|a| !active.contains(&a.program.key))
            .map(|a| a.statics.as_ptr() as u64)
            .collect();
        let before = self.files.len();
        self.close_files_of(&bases);
        before - self.files.len()
    }

    /// `RCLACTGRP(*ELIGIBLE)`.
    pub fn rclactgrp_eligible(&mut self) {
        let live: Vec<u32> = self.act.groups().map(|g| g.id).collect();
        let bases = self.act.reclaim_eligible();
        let gone: Vec<u32> = live.into_iter().filter(|id| !self.act.groups().any(|g| g.id == *id)).collect();
        for id in gone {
            self.end_sql_group(id);
        }
        for b in bases {
            self.spaces.kill_static(b);
        }
    }

    /// The queue a reference names, from the current entry: a frame
    /// index, or the external or operator queue.
    fn queue_target(&self, q: QueueRef) -> Result<QueueTarget, Cond> {
        let n = self.act.frames.len();
        let frame = |i: Option<usize>| i.map(QueueTarget::Frame).ok_or(Cond::Mch3601);
        match q {
            QueueRef::Same => frame(n.checked_sub(1)),
            // The first entry's caller is the job itself: its external
            // queue.
            QueueRef::Prv if n == 1 => Ok(QueueTarget::Ext),
            QueueRef::Prv => frame(n.checked_sub(2)),
            QueueRef::Up(k) => frame(n.checked_sub(1 + k as usize)),
            QueueRef::CtlBdy => {
                let mut i = n.checked_sub(1).ok_or(Cond::Mch3601)?;
                loop {
                    if self.act.frames[i].control_boundary {
                        return Ok(QueueTarget::Frame(i));
                    }
                    if i == 0 {
                        return Ok(QueueTarget::Ext);
                    }
                    i -= 1;
                }
            }
            QueueRef::Ext => Ok(QueueTarget::Ext),
            QueueRef::SysOpr => Ok(QueueTarget::SysOpr),
        }
    }

    fn queue_mut(&mut self, t: QueueTarget) -> &mut MsgQueue {
        match t {
            QueueTarget::Frame(i) => &mut self.act.frames[i].msgq,
            QueueTarget::Ext => &mut self.ext_msgq,
            QueueTarget::SysOpr => &mut self.sysopr,
        }
    }

    /// The messages on a queue, for displays.
    pub fn queue(&self, q: QueueRef) -> Option<&MsgQueue> {
        Some(match self.queue_target(q).ok()? {
            QueueTarget::Frame(i) => &self.act.frames[i].msgq,
            QueueTarget::Ext => &self.ext_msgq,
            QueueTarget::SysOpr => &self.sysopr,
        })
    }

    /// A message's text, help and severity from its description.
    pub fn describe_message(
        &mut self,
        msgid: &str,
        msgf: &str,
        lib: &str,
        data: &[u8],
    ) -> (String, String, u16) {
        if msgid.trim().is_empty() {
            // An immediate message: the data is the text.
            return (ccsid::decode_trimmed(data), String::new(), 0);
        }
        match self.msgs.describe(msgid, msgf, lib) {
            Some(d) => (d.format(data), d.format_help(data), d.severity),
            None => (
                format!("{msgid} {}", ccsid::decode_trimmed(data)),
                String::new(),
                0,
            ),
        }
    }

    /// Send a program message. An escape message becomes the pending
    /// exception of the entry it was sent to; a message to an entry
    /// that does not exist is `MCH3601` here (`CPF2469` on the
    /// machine).
    pub fn send_message(
        &mut self,
        msgid: &str,
        msgf: &str,
        lib: &str,
        data: Vec<u8>,
        msg_type: MsgType,
        to: QueueRef,
    ) -> Result<u32, Cond> {
        let target = self.queue_target(to)?;
        Ok(self.send_message_to(target, msgid, msgf, lib, data, msg_type))
    }

    /// The severity of an exception's message description, 0 when it
    /// has none.
    #[must_use]
    pub fn severity_of(&mut self, e: &Exception) -> u16 {
        self.msgs
            .describe(&e.msgid, &e.msgf, "QSYS")
            .map_or(0, |d| d.severity)
    }

    pub(crate) fn send_message_to(
        &mut self,
        target: QueueTarget,
        msgid: &str,
        msgf: &str,
        lib: &str,
        data: Vec<u8>,
        msg_type: MsgType,
    ) -> u32 {
        let (text, help, severity) = self.describe_message(msgid, msgf, lib, &data);
        let key = self.next_key;
        self.next_key += 1;
        let from_depth = self.depth;
        let m = Message {
            key,
            msgid: msgid.into(),
            msgf: msgf.into(),
            msgf_lib: lib.into(),
            msg_type,
            data: data.clone(),
            text,
            help,
            severity,
            from_depth,
            received: false,
            reply: None,
        };
        self.log_message(&m, target);
        self.queue_mut(target).push(m);
        if msg_type == MsgType::Escape {
            match target {
                QueueTarget::Frame(i) => {
                    let depth = u32::try_from(i + 1).expect("few frames");
                    let mut e = Exception::message(msgid, msgf, data, depth);
                    e.key = key;
                    // A message RPG has no status for: 202 when a called
                    // program or procedure sent it, 9999 otherwise.
                    if e.status == 0 && !e.is_file_error() {
                        e.status = if from_depth > depth { 202 } else { 9999 };
                    }
                    self.raised.push(e.clone());
                    crate::except::dispatch(self, &e);
                }
                // The first entry's escape to its caller ends every
                // entry; the job finds it pending when the call returns.
                QueueTarget::Ext if !self.act.frames.is_empty() => {
                    let mut e = Exception::message(msgid, msgf, data, 0);
                    e.key = key;
                    self.raised.push(e.clone());
                    self.exception = Some(e);
                    self.abi.pending = 1;
                }
                _ => {}
            }
        }
        key
    }

    /// Receive the oldest message of a type, or the keyed one, from a
    /// queue; marked received or removed.
    pub fn receive_message(
        &mut self,
        from: QueueRef,
        ty: Option<MsgType>,
        key: Option<u32>,
        rmv: bool,
    ) -> Option<Message> {
        let t = self.queue_target(from).ok()?;
        self.queue_mut(t).receive(ty, key, rmv)
    }

    /// Remove one message by key, or clear by mode; how many went.
    pub fn remove_messages(
        &mut self,
        from: QueueRef,
        key: Option<u32>,
        clear: ClearMode,
    ) -> Result<usize, Cond> {
        let t = self.queue_target(from)?;
        let q = self.queue_mut(t);
        Ok(match key {
            Some(k) => usize::from(q.remove(k)),
            None => q.clear(clear),
        })
    }

    /// Move a message by key from one queue to another, key kept
    /// (`QMHMOVPM`); `false` when it is not there.
    pub fn move_message(&mut self, key: u32, from: QueueRef, to: QueueRef) -> bool {
        let (Ok(src), Ok(dst)) = (self.queue_target(from), self.queue_target(to)) else {
            return false;
        };
        let queue = self.queue_mut(src);
        let Some(i) = queue.msgs.iter().position(|m| m.key == key) else {
            return false;
        };
        let m = queue.msgs.remove(i);
        self.queue_mut(dst).push(m);
        true
    }

    /// Mark the pending exception handled when `key` is its message
    /// (`QMHCHGEM *HANDLE`); `false` otherwise.
    pub fn handle_pending(&mut self, key: u32) -> bool {
        if self.exception.as_ref().is_some_and(|e| e.key == key) {
            crate::except::resume(self);
            true
        } else {
            false
        }
    }

    /// End the request abnormally (`CEE4ABN`): the entries down to the
    /// nearest control boundary are cancelled and its caller receives
    /// `msgid` as an escape, as reply `C` to an inquiry does.
    pub fn cancel_request(&mut self, msgid: &str, msgf: &str, data: Vec<u8>) {
        crate::except::cancel_to_boundary(self, msgid, msgf, data);
    }

    /// End the request (`CEETREC`): every entry ends with `CEE9903`
    /// pending, which no handler sees; whoever called the first entry
    /// clears it.
    pub fn end_request(&mut self, user_rc: i32) {
        let data = user_rc.to_be_bytes().to_vec();
        let e = Exception::message("CEE9903", "QCEEMSG", data, 0);
        self.raised.push(e.clone());
        self.exception = Some(e);
        self.abi.pending = 1;
    }

    /// Live activation groups, for `DSPACTGRP`.
    pub fn groups(&self) -> impl Iterator<Item = &ActivationGroup> {
        self.act.groups()
    }
}

impl Job {
    /// The program, library and statement of call-stack entry `i`
    /// (0 is the oldest); `*N` for a native or unknown entry.
    /// The program and library of frame `i`, and its statement, as the
    /// job log names them.
    pub fn frame_where(&self, i: Option<usize>) -> ((String, String), String) {
        let unknown = || (("*N".to_string(), String::new()), "*N".to_string());
        let Some(i) = i else {
            return unknown();
        };
        let Some(f) = self.act.frames.get(i) else {
            return unknown();
        };
        let Some(p) = f.program.as_ref() else {
            return unknown();
        };
        let (lib, name) = p.key.split_once('/').unwrap_or(("", p.key.as_str()));
        let stmt = if i + 1 == self.act.frames.len() {
            self.abi.cur_stmt
        } else {
            f.stmt
        };
        ((name.to_string(), lib.to_string()), format!("{stmt:04}"))
    }

    /// Microseconds since the epoch.
    fn log_now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_micros()).unwrap_or(u64::MAX))
    }

    /// Put a sent message in the job log.
    fn log_message(&mut self, m: &Message, target: QueueTarget) {
        let top = self.act.frames.len().checked_sub(1);
        let (from, from_inst) = self.frame_where(top);
        let (to, to_inst) = match target {
            QueueTarget::Frame(i) => self.frame_where(Some(i)),
            QueueTarget::Ext => (("*EXT".into(), String::new()), "*N".into()),
            QueueTarget::SysOpr => (("QSYSOPR".into(), "QSYS".into()), "*N".into()),
        };
        let kind = match m.msg_type {
            MsgType::Info | MsgType::Scope | MsgType::Any => "Information",
            MsgType::Comp => "Completion",
            MsgType::Diag => "Diagnostic",
            MsgType::Escape => "Escape",
            MsgType::Status => "Status",
            MsgType::Notify => "Notify",
            MsgType::Rqs => "Request",
            MsgType::Inq => "Inquiry",
            MsgType::Rpy => "Reply",
        };
        self.log_entry(wm::joblog::Entry {
            msgid: m.msgid.clone(),
            kind: kind.into(),
            severity: m.severity,
            time: Self::log_now(),
            from,
            from_inst,
            to,
            to_inst,
            text: m.text.clone(),
            help: m.help.clone(),
            request: 0,
            external: matches!(target, QueueTarget::Ext),
        });
    }

    /// Append to the job log, noting a full log.
    pub fn log_entry(&mut self, e: wm::joblog::Entry) {
        let r = self.joblog.append(e);
        self.logged(r);
    }

    /// What an append did: the entry into the outbox, wrapped entries
    /// kept, a full log noted.
    fn logged(&mut self, r: wm::joblog::Appended) {
        match r {
            wm::joblog::Appended::Logged => {}
            wm::joblog::Appended::Wrapped(gone) => self.joblog_wrapped.extend(gone),
            wm::joblog::Appended::Full => {
                self.joblog_full = true;
                return;
            }
        }
        if let Some(e) = self.joblog.entries().last() {
            self.log_outbox.push(e.clone());
        }
    }

    /// Log a request the job runs: a command or a call.
    pub fn log_request(&mut self, text: &str, from: (&str, &str)) {
        let r = self
            .joblog
            .request(text, Self::log_now(), (from.0.into(), from.1.into()));
        self.logged(r);
    }

    /// Log a message the job sends to its own external queue from
    /// outside any program: job start and end, commands' messages.
    pub fn log_external(&mut self, msgid: &str, msgf: &str, data: &[u8], kind: MsgType) {
        self.log_external_with(msgid, msgf, data, kind, None, None);
    }

    /// [`log_external`](Self::log_external) with the first- or
    /// second-level text replaced when the sender has better than the
    /// description gives.
    pub fn log_external_with(
        &mut self,
        msgid: &str,
        msgf: &str,
        data: &[u8],
        kind: MsgType,
        text: Option<String>,
        help: Option<String>,
    ) {
        let (dtext, dhelp, severity) = self.describe_message(msgid, msgf, "*LIBL", data);
        let text = text.unwrap_or(dtext);
        let help = help.unwrap_or(dhelp);
        let key = self.next_key;
        self.next_key += 1;
        let m = Message {
            key,
            msgid: msgid.into(),
            msgf: msgf.into(),
            msgf_lib: "*LIBL".into(),
            msg_type: kind,
            data: data.to_vec(),
            text,
            help,
            severity,
            from_depth: 0,
            received: false,
            reply: None,
        };
        self.log_message(&m, QueueTarget::Ext);
        self.ext_msgq.push(m);
    }
}
