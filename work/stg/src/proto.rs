// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The storage service's requests. A connection starts with `hello`;
//! every other request runs as the job that said hello, in a
//! transaction of its own the service begins and ends, so no store
//! transaction ever outlives a request.

use ipc::wire_struct;
use mi::io::{DataAreaValue, FileInfo as IoFileInfo, IoFail, IoResult, OpenSpec, OwnedRequest};
use msgf::MsgDesc;
use sysapi::{
    ApiFail, CommitInfo, DataArea, DepInfo, DtaqEntry, DtaqInfo, FileInfo, FormatInfo, IdxAttr,
    LocalNow, MemberInfo, ObjInfo, SysVal, UserInfo,
};
use wm::job::JobAttrs;
use wm::obj::jobq::{QJob, SbmJob};
use wm::obj::msgq::{MsgKind, QMsg};
use wm::obj::prtf::Prtf;
use wm::obj::splf::SplfAttrs;
use wm::sbs::{PoolInfo, SbsInfo, SbsState};
use wm::table::JobInfo;
use wm::wtr::{WtrDest, WtrStep};

/// What a command run in the service answered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecReply {
    /// Output.
    pub text: String,
    /// The prompt the command line wants next (`sql> ` in SQL mode).
    pub prompt: String,
    /// SRC codes the command raised, for `slic` to show and log.
    pub srcs: Vec<String>,
    /// The system values, when the command changed them.
    pub sysvals: Option<Vec<u8>>,
}

wire_struct!(ExecReply {
    text,
    prompt,
    srcs,
    sysvals
});

/// What `slic` hands the service as it starts: the units, in the order
/// they were found, and the system values as the IPL set them. The
/// service answers with `Result<String, String>`: what it opened, or
/// why it could not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Start {
    /// Device nodes of the ASP 1 units.
    pub devices: Vec<String>,
    /// The system values blob.
    pub sysvals: Option<Vec<u8>>,
    /// The values the IPL set (`QABNORMSW`, `QIPLSTS`), never saved.
    pub system: Vec<u8>,
}

wire_struct!(Start {
    devices,
    sysvals,
    system
});

/// Index attributes, entry count and library.
pub type IdxAttrs = (IdxAttr, usize, String);

/// A message a command's processing program sent: identifier, type
/// code (`MsgType` as `QUSRJOBI` names it: 1 info, 4 completion, 5
/// diagnostic, 7 escape ...) and data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CmdMsg {
    /// `AAAnnnn`.
    pub msgid: String,
    /// The message type code.
    pub kind: u8,
    /// Substitution data.
    pub data: Vec<u8>,
}

wire_struct!(CmdMsg { msgid, kind, data });

/// What running a system command in the service answered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CmdReply {
    /// Output for the display.
    pub text: String,
    /// Messages for the job log: completion and diagnostics, in order.
    pub messages: Vec<CmdMsg>,
    /// The escape message that ended the command, if one did.
    pub escape: Option<CmdMsg>,
    /// The parameter block back, for `RTNVAL(*YES)` parameters.
    pub segments: Vec<Option<Vec<u8>>>,
    /// The console state `exec` reports: the prompt to show next, the
    /// SRC codes raised and the system values when they changed.
    pub prompt: String,
    /// SRC codes.
    pub srcs: Vec<String>,
    /// The system values.
    pub sysvals: Option<Vec<u8>>,
}

wire_struct!(CmdReply {
    text,
    messages,
    escape,
    segments,
    prompt,
    srcs,
    sysvals
});

ipc::rpc! {
    request Request, service Service, client Client, dispatch dispatch;

    /// First request on a connection: who is speaking.
    0 => fn hello / Hello (job: JobInfo) -> Result<(), String>;
    /// A service console command, run in the service.
    1 => fn exec / Exec (line: String) -> ExecReply;
    /// Checkpoint and close the store; the service exits after the
    /// answer. Only `slic` sends it.
    2 => fn close / Close () -> Result<String, String>;
    /// Nothing; proves the service is up.
    3 => fn ping / Ping () -> ();
    /// A job number for a new job: the next after the last one
    /// handed out, skipping numbers in use. Kept in the store.
    4 => fn job_number / JobNumber () -> Result<u32, String>;
    /// Set the job's status (`Status::byte`) and function. The answer:
    /// whether the job holds an activity level in its pool now: a
    /// runnable status takes one if one is free, a wait gives it up.
    5 => fn set_status / SetStatus (status: u8, function: String) -> bool;
    /// Every active job.
    6 => fn active_jobs / ActiveJobs () -> Vec<JobInfo>;
    /// The job's own attributes (`RTVJOBA`).
    7 => fn rtvjoba / Rtvjoba () -> JobAttrs;
    /// `CHGJOB`: `job` is `number/user/name`, or blank for the job
    /// itself; each change is a keyword and its value.
    8 => fn chgjob / Chgjob (job: String, changes: Vec<(String, String)>) -> Result<(), String>;

    /// A `*PGM` (kind 0) or `*SRVPGM` (kind 1): its encoded object,
    /// name and the library it was found in.
    10 => fn program / Program (name: String, lib: String, kind: u8) -> Option<(Vec<u8>, String, String)>;
    /// A system pointer's identifier for `lib/name` of a type.
    11 => fn resolve / Resolve (name: String, lib: String, ty: String) -> Option<u64>;
    /// The `(name, library)` an identifier names.
    12 => fn by_id / ById (id: u64) -> Option<(String, String)>;

    /// A user space's bytes.
    20 => fn us_load / UsLoad (name: String, lib: String) -> Option<Vec<u8>>;
    /// Write a user space back.
    21 => fn us_store / UsStore (name: String, lib: String, data: Vec<u8>) -> bool;
    /// Create a user space; the message identifier on failure.
    22 => fn us_create / UsCreate (name: String, lib: String, size: u64, init: u8, text: String) -> Result<(), String>;
    /// Delete a user space.
    23 => fn us_delete / UsDelete (name: String, lib: String) -> bool;
    /// The library a user space was found in.
    24 => fn us_library_of / UsLibraryOf (name: String, lib: String) -> Option<String>;

    /// A message description.
    30 => fn describe / Describe (msgid: String, msgf: String, lib: String) -> Option<MsgDesc>;

    /// `dtaara.lock`.
    40 => fn da_lock / DaLock (name: String, lib: String) -> Result<(), IoFail>;
    /// `dtaara.unlock`.
    41 => fn da_unlock / DaUnlock (name: String, lib: String) -> Result<(), IoFail>;
    /// `dtaara.read`.
    42 => fn da_read / DaRead (name: String, lib: String) -> Result<DataAreaValue, IoFail>;
    /// `dtaara.write`.
    43 => fn da_write / DaWrite (name: String, lib: String, value: DataAreaValue) -> Result<(), IoFail>;
    /// `dtaq.snd`.
    44 => fn dq_send / DqSend (name: String, lib: String, data: Vec<u8>) -> Result<(), IoFail>;
    /// `dtaq.rcv`, without waiting: the job waits between tries.
    45 => fn dq_receive / DqReceive (name: String, lib: String) -> Result<Option<Vec<u8>>, IoFail>;

    /// Open a file.
    50 => fn open / Open (spec: OpenSpec) -> Result<(u32, IoFileInfo), IoFail>;
    /// A record operation.
    51 => fn io / Io (handle: u32, req: OwnedRequest) -> IoResult;
    /// Close a file.
    52 => fn close_file / CloseFile (handle: u32) -> IoResult;
    /// `COMMIT` or `ROLBK` of the job's files (`IoSource::commit`).
    54 => fn io_commit / IoCommit (rollback: bool, actgrp: String) -> Option<Result<(), IoFail>>;
    /// An embedded SQL statement (`sql::embed`): the scope, the
    /// encoded descriptor and the inputs; the reply in its wire form.
    53 => fn sql_run / SqlRun (scope: String, desc: String, inputs: Vec<(u8, String)>) -> sql::embed::WireReply;

    /// `Provider::list_objects`.
    60 => fn list_objects / ListObjects (lib: String, name: String, ty: String) -> Vec<ObjInfo>;
    /// `Provider::object`.
    61 => fn object / Object (lib: String, name: String, ty: String) -> Option<ObjInfo>;
    /// `Provider::change_object`.
    62 => fn change_object / ChangeObject (lib: String, name: String, ty: String, text: Option<String>, user_attr: Option<String>) -> Result<String, ApiFail>;
    /// `Provider::rename_object`.
    63 => fn rename_object / RenameObject (lib: String, name: String, ty: String, new_lib: String, new_name: String, replace: bool) -> Result<String, ApiFail>;
    /// `Provider::usridx_create`.
    64 => fn usridx_create / UsridxCreate (lib: String, name: String, attr: IdxAttr) -> Result<(), ApiFail>;
    /// `Provider::usridx_delete`.
    65 => fn usridx_delete / UsridxDelete (lib: String, name: String) -> Result<(), ApiFail>;
    /// `Provider::usridx_add`.
    66 => fn usridx_add / UsridxAdd (lib: String, name: String, entries: Vec<Vec<u8>>, replace: bool) -> Result<usize, ApiFail>;
    /// `Provider::usridx_find`.
    67 => fn usridx_find / UsridxFind (lib: String, name: String, rule: u32, key: Vec<u8>, key2: Vec<u8>, max: usize, remove: bool) -> Result<Vec<Vec<u8>>, ApiFail>;
    /// `Provider::usridx_attrs`.
    68 => fn usridx_attrs / UsridxAttrs (lib: String, name: String) -> Result<IdxAttrs, ApiFail>;
    /// `Provider::dtaq_send`.
    69 => fn api_dtaq_send / ApiDtaqSend (lib: String, name: String, key: Vec<u8>, data: Vec<u8>, sender: String) -> Result<(), ApiFail>;
    /// `Provider::dtaq_receive`, without waiting.
    70 => fn api_dtaq_receive / ApiDtaqReceive (lib: String, name: String, order: String, key: Vec<u8>, remove: bool) -> Result<Option<DtaqEntry>, ApiFail>;
    /// `Provider::dtaq_clear`.
    71 => fn api_dtaq_clear / ApiDtaqClear (lib: String, name: String, order: String, key: Vec<u8>) -> Result<(), ApiFail>;
    /// `Provider::dtaq_describe`.
    72 => fn api_dtaq_describe / ApiDtaqDescribe (lib: String, name: String) -> Result<DtaqInfo, ApiFail>;
    /// `Provider::dtaq_entries`.
    73 => fn api_dtaq_entries / ApiDtaqEntries (lib: String, name: String) -> Result<Vec<DtaqEntry>, ApiFail>;
    /// `Provider::dtaara_read`.
    74 => fn api_dtaara_read / ApiDtaaraRead (lib: String, name: String) -> Result<DataArea, ApiFail>;
    /// `Provider::dtaara_write`.
    75 => fn api_dtaara_write / ApiDtaaraWrite (lib: String, name: String, start: usize, bytes: Vec<u8>) -> Result<(), ApiFail>;
    /// `Provider::sysval`.
    76 => fn sysval / Sysval (name: String) -> Option<SysVal>;
    /// `Provider::netattr`.
    77 => fn netattr / Netattr (name: String) -> Option<SysVal>;
    /// `Provider::now`.
    78 => fn now / Now () -> LocalNow;
    /// `Provider::check_command`.
    79 => fn check_command / CheckCommand (text: String) -> Result<(), ApiFail>;
    /// `Provider::file`.
    80 => fn file / File (lib: String, name: String) -> Option<FileInfo>;
    /// `Provider::members`.
    81 => fn members / Members (lib: String, name: String) -> Vec<MemberInfo>;
    /// `Provider::formats`.
    82 => fn formats / Formats (lib: String, name: String) -> Vec<FormatInfo>;
    /// `Provider::dependents`.
    83 => fn dependents / Dependents (lib: String, name: String, member: String) -> Vec<DepInfo>;
    /// `Provider::commit_info`.
    84 => fn commit_info / CommitInfoReq () -> Option<CommitInfo>;
    /// `Provider::user_info`.
    85 => fn user_info / UserInfoReq (name: String) -> Option<UserInfo>;

    /// Send to a message queue (`LIB/NAME`, `*SYSOPR`); the key. The
    /// service fills in the sending job.
    90 => fn msgq_send / MsgqSend (queue: String, msg: QMsg) -> Result<u32, String>;
    /// Receive from a queue without waiting: the oldest new message of
    /// a kind, or the keyed one; removed when `remove`.
    91 => fn msgq_receive / MsgqReceive (queue: String, kind: Option<MsgKind>, key: Option<u32>, remove: bool) -> Result<Option<QMsg>, String>;
    /// Answer an inquiry on a queue; the reply's key.
    92 => fn msgq_reply / MsgqReply (queue: String, key: u32, reply: String) -> Result<u32, String>;
    /// Every message on a queue.
    93 => fn msgq_list / MsgqList (queue: String) -> Result<Vec<QMsg>, String>;
    /// Break messages waiting for the job: the queue, its break
    /// program (`*DSPMSG` for the system's) and the message.
    94 => fn breaks / Breaks () -> Vec<(String, (String, String), QMsg)>;
    /// The system reply list's answer to an inquiry: the reply and
    /// whether to dump the job, or `None` when no entry matches.
    95 => fn sysrpyl / Sysrpyl (msgid: String, data: Vec<u8>) -> Option<(String, bool)>;

    /// A subsystem that may be started: the library and the encoded
    /// description. `CPF1010` when it is active.
    100 => fn sbs_check / SbsCheck (name: String) -> Result<(String, Vec<u8>), String>;
    /// A monitor says its subsystem is starting: the encoded
    /// description.
    101 => fn sbs_register / SbsRegister (name: String, lib: String) -> Result<Vec<u8>, String>;
    /// What the calling monitor's subsystem is to do.
    102 => fn sbs_state / SbsState () -> SbsState;
    /// Every active subsystem.
    103 => fn sbs_list / SbsList () -> Vec<SbsInfo>;
    /// Every storage pool.
    104 => fn pools / Pools () -> Vec<PoolInfo>;
    /// `ENDSBS`.
    105 => fn sbs_end / SbsEnd (name: String, immediate: bool, delay: u32) -> Result<(), String>;
    /// A work management object's payload: `lib/name` of a type
    /// (`*JOBD`, `*CLS`, `*SBSD` ...).
    106 => fn wm_object / WmObject (qual: String, ty: String) -> Result<Vec<u8>, String>;
    /// A subsystem's private pool, defined by its monitor.
    107 => fn define_pool / DefinePool (name: String, size_kb: u64, actlvl: u32) -> ();
    /// `SBMJOB`: the job's number and the message saying where it went.
    108 => fn sbmjob / Sbmjob (req: SbmJob) -> Result<(u32, String), String>;
    /// The next job a monitor may start from a queue it holds: the
    /// priorities it may start (index 0-9) given its limits.
    109 => fn jobq_take / JobqTake (qual: String, allowed: Vec<bool>) -> Option<QJob>;
    /// The job is ending with this end code (0 normal, 20 and up
    /// abnormal); its completion message says so.
    110 => fn job_ending / JobEnding (code: u16) -> ();
    /// The job scheduler's tick: submit the schedule entries that are
    /// due; what was submitted.
    111 => fn scd_tick / ScdTick () -> Vec<String>;
    /// Entries the job logged since the last call, for the service's
    /// copy of its job log.
    112 => fn joblog_put / JoblogPut (entries: Vec<wm::joblog::Entry>) -> ();
    /// Open a spooled file for a printer file: its spool identifier.
    113 => fn splf_open / SplfOpen (attrs: SplfAttrs) -> Result<u32, String>;
    /// Append SCS data to an open spooled file, with its page and
    /// record counts so far.
    114 => fn splf_put / SplfPut (id: u32, data: Vec<u8>, pages: u32, records: u32) -> Result<(), String>;
    /// Close a spooled file, held when the printer file says `HOLD(*YES)`.
    115 => fn splf_close / SplfClose (id: u32, hold: bool) -> Result<(), String>;
    /// Open printer file `lib/file` from activation group `actgrp`, the
    /// job's overrides applied: the new spooled file and the attributes.
    116 => fn prt_open / PrtOpen (file: String, lib: String, actgrp: String) -> Result<(u32, Prtf), String>;
    /// The writer job `name`'s next step.
    117 => fn wtr_next / WtrNext (name: String) -> WtrStep;
    /// The writer printed spooled file `id`: its text and PDF.
    118 => fn wtr_printed / WtrPrinted (name: String, id: u32, text: String, pdf: Vec<u8>) -> Result<(), String>;
    /// `QSPLMAINT`'s cleanup: expired spooled files and captures of
    /// files long gone; what it did.
    119 => fn spool_maint / SpoolMaint () -> String;
    /// Where the last job of this name stands this IPL: 0 none yet, 1
    /// active or about to be, 2 ended.
    120 => fn job_state / JobState (name: String) -> u8;
    /// `QIPLSTS` for the next IPL, kept until then.
    121 => fn next_iplsts / NextIplsts (value: u8) -> Result<(), String>;
    /// `QWCRSSTS`: the receiver of a format, statistics reset when asked.
    122 => fn sys_status / SysStatus (format: String, reset: bool) -> Result<Vec<u8>, String>;

    /// Who the job is: its user, groups, special authorities, the
    /// owners it adopts.
    123 => fn whoami / Whoami () -> sec::auth::principal::Principal;
    /// The programs (`LIB/NAME`) whose owners the call stack adopts,
    /// nearest first; the service checks each is `USRPRF(*OWNER)`. The
    /// owners adopted.
    124 => fn adopt / Adopt (programs: Vec<String>) -> Result<Vec<String>, String>;
    /// Whether a user space is in the system domain.
    125 => fn us_domain / UsDomain (name: String, lib: String) -> bool;
    /// Sign on at display `device` (`console` for the system console):
    /// what to do next, or why not. `slic` sends it.
    126 => fn sign_on / SignOn (user: String, password: String, device: String, console: bool) -> Result<sec::signon::SignedOn, sec::signon::Refused>;
    /// Change the job's own password; `validated` says `QPWDVLDPGM`
    /// already accepted it in the job.
    127 => fn chg_pwd / ChgPwd (current: String, new: String, validated: bool) -> Result<(), String>;
    /// Service tools use `slic` saw (DST, the serial console): kind
    /// (`ST`, `DS`, `PW`), code, user; for the audit journal.
    128 => fn audit_lic / AuditLic (events: Vec<(String, String, String)>) -> ();
    /// `Provider::sec`: a security API's question, as the job.
    129 => fn sec_api / SecApi (q: sec::api::Query) -> Result<sec::api::Answer, ApiFail>;

    /// A `*CMD` by name, in `lib` or through the job's library list,
    /// with the authority to run it checked: its payload and the
    /// library it was found in; `None` when there is none; `Err` with
    /// the message identifier when it may not be used.
    130 => fn cmd_lookup / CmdLookup (name: String, lib: Option<String>) -> Result<Option<(Vec<u8>, String)>, String>;
    /// Run a system command's processing program in the service: the
    /// resolved command, its parameter block and which parameters the
    /// caller gave.
    131 => fn run_cmd / RunCmd (name: String, segments: Vec<Option<Vec<u8>>>, specified: Vec<bool>, typed: String) -> CmdReply;

    /// A display device for a TN5250E session: the one named (made
    /// under `QAUTOCFG` when missing), else a free or new virtual
    /// display within `QAUTOVRT`. `Err` is the RFC 2877 response code.
    132 => fn device_allocate / DeviceAllocate (requested: String, devtype: String, model: String, kbdtype: String) -> Result<String, String>;
    /// A display's status: 1 varied on, 2 sign-on display, 3 active, 4
    /// recovery pending.
    133 => fn device_status / DeviceStatus (device: String, status: u8) -> Result<(), String>;
    /// `PWRDWNSYS` from a job that is not at the console: the service
    /// hands it to `slic`.
    134 => fn power_down / PowerDown (immediate: bool, delay: u32, restart: bool) -> Result<(), String>;
    /// Sign on with an RFC 4777 substitute instead of a password.
    135 => fn sign_on_substitute / SignOnSubstitute (user: String, device: String, proof: sec::signon::Substitute) -> Result<sec::signon::SignedOn, sec::signon::Refused>;
    /// Open display file `lib/file` from activation group `actgrp`, the
    /// job's overrides applied: the file's payload and its real
    /// `lib/name`.
    136 => fn dsp_open / DspOpen (file: String, lib: String, actgrp: String) -> Result<(Vec<u8>, String), String>;
    /// A menu's attributes (encoded) and its real library.
    137 => fn menu_open / MenuOpen (name: String, lib: String) -> Result<(Vec<u8>, String), String>;
    /// A panel group's payload and its real library.
    138 => fn pnlgrp_open / PnlgrpOpen (name: String, lib: String) -> Result<(Vec<u8>, String), String>;
    /// Where writer `name` prints: the capture device, a LAN printer or
    /// a remote output queue.
    139 => fn wtr_target / WtrTarget (name: String) -> Result<WtrDest, String>;
    // --- panel data: work management (140-159)
    /// `WRKACTJOB`'s figures: the jobs and the machine's CPU since the
    /// display's last look, which moves to now when `advance`.
    140 => fn actjob_data / ActjobData (advance: bool) -> ActJobs;
    /// A job by `number/user/name` (blank for the caller): what
    /// `DSPJOB` and `WRKJOB` show.
    141 => fn job_detail / JobDetailReq (job: String) -> Result<JobDetail, String>;
    /// Spooled files, oldest first: of a user (blank for any) and a
    /// job (`number/user/name`, blank for any).
    142 => fn splf_list / SplfList (user: String, job: String) -> Vec<SplfAttrs>;
    /// The job queues subsystems hold: `lib/name` and the subsystem.
    143 => fn jobq_attach / JobqAttach () -> Vec<(String, String)>;
    /// The writers: name, device, status (`STR`, `WTR`, `HLD`, `END`,
    /// `JOBQ`), form type, output queue `lib/name`, files printed.
    144 => fn wtr_list / WtrList () -> Vec<(String, String, String, String, String, u32)>;
    /// Writer `name` could not print spooled file `id`: the message it
    /// sends (`msgid`, a line of text), and whether the file is held
    /// (the far end refused it) or waits to be tried again.
    145 => fn wtr_failed / WtrFailed (name: String, id: u32, msgid: String, text: String, hold: bool) -> Result<(), String>;
    // --- panel data: messages and objects (160-179)
    /// What `DSPOBJD DETAIL(*FULL)` shows beyond `object`: `(field,
    /// value)` pairs (`CREATOR`, `SYSTEM`, `RELEASE`, `SIZE` in bytes,
    /// `DOMAIN`, `DAMAGED`, `PGP`, `AUDIT`, `LASTUSED`, `DAYSUSED`,
    /// `RESET`; dates `CYYMMDDHHMMSS`), `None` when not found.
    160 => fn obj_detail / ObjDetail (lib: String, name: String, ty: String) -> Option<Vec<(String, String)>>;
    /// The objects a name and type select in a library (or `*LIBL`),
    /// with their size in bytes: library, name, type, size.
    161 => fn obj_sizes / ObjSizes (lib: String, name: String, ty: String) -> Vec<(String, String, String, u64)>;
    /// The system reply list: sequence, message identifier, compare
    /// data, its start position, reply, dump.
    162 => fn rpyl_entries / RpylEntries () -> Result<Vec<RpyRow>, String>;
    /// The history log, oldest first: date (`CYYMMDD`), time
    /// (`HHMMSS`), message identifier, severity, type, job name, user,
    /// number and text, in that order.
    163 => fn hst_entries / HstEntries () -> Result<Vec<Vec<String>>, String>;
    /// A message file's descriptions: identifier and first-level text.
    164 => fn msgf_list / MsgfList (msgf: String, lib: String) -> Result<Vec<(String, String)>, String>;
    // --- panel data: system and security (180-199)
    /// A system value for `DSPSYSVAL`: its text, its type (`*SEC` ...)
    /// and the value's elements as shown.
    180 => fn sysval_panel / SysvalPanel (name: String) -> Option<(String, String, Vec<String>)>;
    /// Every system value for `WRKSYSVAL`: name, type and text.
    181 => fn sysval_panel_list / SysvalPanelList () -> Vec<(String, String, String)>;
    /// The network attributes for `DSPNETA`, by keyword.
    182 => fn neta_panel / NetaPanel () -> Result<Vec<(String, String)>, String>;
    /// `DSPUSRPRF TYPE(*BASIC)`: the panel's variables and values.
    183 => fn usrprf_panel / UsrprfPanel (name: String) -> Result<Vec<(String, String)>, String>;
    /// `WRKUSRPRF`: name, class and text of every profile.
    184 => fn usrprf_panel_list / UsrprfPanelList () -> Result<Vec<(String, String, String)>, String>;
    /// `DSPAUTUSR`: user, group, password change date, `X` for no
    /// password and text, by user or by group.
    185 => fn autusr_panel / AutusrPanel (by_group: bool) -> Result<Vec<Vec<String>>, String>;
    /// `DSPSFWRSC`: resource ID, option, feature, description, feature
    /// type, library and release of each installed product.
    186 => fn sfwrsc_panel / SfwrscPanel () -> Vec<Vec<String>>;
    /// `DSPAUTL`: the list's owner and text, then each user with its
    /// authority and `X` for list management.
    187 => fn autl_panel / AutlPanel (name: String) -> Result<Vec<Vec<String>>, String>;
    /// `WRKAUTL`: every authorization list and its text.
    188 => fn autl_panel_list / AutlPanelList () -> Result<Vec<(String, String)>, String>;
    /// `DSPSECAUD`: the audit journal, its receiver and the auditing
    /// system values, by label.
    189 => fn secaud_panel / SecaudPanel () -> Result<Vec<(String, String)>, String>;
    /// `WRKCFGSTS CFGTYPE(*DEV)`: name, status and text of each device.
    190 => fn cfgsts_panel / CfgstsPanel () -> Result<Vec<Vec<String>>, String>;
    /// `DSPDEVD`: the panel's variables and values.
    191 => fn devd_panel / DevdPanel (name: String) -> Result<Vec<(String, String)>, String>;
    // --- system displays (200-219)
    /// `CHGGRPA`: the caller becomes group job `name` (its group made
    /// when it has none) with `text`; `*NONE` leaves the group. The
    /// group's number.
    200 => fn grp_chg / GrpChg (name: String, text: String) -> Result<u32, String>;
    /// The caller's group: its number, the caller's group job name and
    /// every member (group job name, job number, text).
    201 => fn grp_info / GrpInfo () -> Option<GroupInfo>;
    /// A job the session started for `TFRGRPJOB` joins `group` as `name`.
    202 => fn grp_join / GrpJoin (group: u32, name: String, text: String, number: u32) -> Result<(), String>;
    /// A job left its group: it ended, or `ENDGRPJOB` ended it.
    203 => fn grp_leave / GrpLeave (number: u32) -> ();
    /// The caller's group data area (`*GDA`), 512 bytes.
    204 => fn gda_read / GdaRead () -> Result<Vec<u8>, String>;
    /// Change the caller's `*GDA` from `start` (1-based).
    205 => fn gda_write / GdaWrite (start: u32, bytes: Vec<u8>) -> Result<(), String>;
    /// The jobs at a display device, from the session holding it: job
    /// number, user, name and side (1 primary, 2 secondary), the job
    /// the display shows first.
    206 => fn ws_set / WsSet (device: String, jobs: Vec<WsJob>) -> ();
    /// The jobs at a display device as its session last said.
    207 => fn ws_get / WsGet (device: String) -> Vec<WsJob>;
    // --- registration facility (210-219)
    /// A registration facility question: the APIs', `ADDEXITPGM`'s,
    /// `WRKREGINF`'s, and a server's looking up or reporting on its
    /// exit programs.
    210 => fn regfac / Regfac (q: regfac::Query) -> Result<regfac::Answer, regfac::RegFail>;
    // --- UIM APIs (220-239)
    /// A search index's entries: title, root words, panel group
    /// (`LIB/NAME`) and help module of each topic.
    220 => fn schidx_entries / SchidxEntries (name: String, lib: String) -> Result<Vec<SchidxEntry>, String>;
    /// `CRTSCHIDX`: an empty search index.
    221 => fn schidx_create / SchidxCreate (name: String, lib: String, text: String, replace: bool) -> Result<String, String>;
    /// `ADDSCHIDXE`: a panel group's `:ISCH.` topics into a search index.
    222 => fn schidx_add / SchidxAdd (name: String, lib: String, pnlgrp: String, pnlgrp_lib: String) -> Result<String, String>;
    // --- panels batch 2: work, spool, journals (240-269)
    /// A batch 2 display's data (`work/qsjob/src/panels/work2.rs`):
    /// `what` names the display (`SBMJOB`, `SHRPOOL`, `SYSSTS`, ...),
    /// `args` its parameters; the header's variables and the list's
    /// entries as `(variable, value)` pairs of the display's panel group.
    240 => fn panel2_data / Panel2Data (what: String, args: Vec<String>) -> Result<PanelData, String>;
    // --- panels batch 2: programs, config, security, editors (270-299)
    /// A display of the second panel batch, `what` naming it (`PGM`,
    /// `SRVPGM`, `MODULE`, `BNDDIR`, `CMD`: library and name; `DEVD`;
    /// `HDWRSC`: the type; `UPRF`: the user and the display type): the
    /// panel's variables and its lists' entries.
    270 => fn sys2_panel / Sys2Panel (what: String, args: Vec<String>) -> Result<Sys2Data, String>;
    // --- SEU (300-319)
    /// SEU: a source member read whole and locked for the job, `*EXCL`
    /// to edit and `*SHRRD` to browse; a missing member opens empty to
    /// edit.
    300 => fn seu_open / SeuOpen (lib: String, file: String, member: String, edit: bool) -> Result<SrcMember, String>;
    /// SEU: write a member's records (sequence, date, data) back, the
    /// member added when new, its source type and text set.
    301 => fn seu_save / SeuSave (lib: String, file: String, member: String, records: Vec<(u32, u32, String)>, srctype: String, text: String) -> Result<(), String>;
    /// SEU: release the member `seu_open` locked.
    302 => fn seu_close / SeuClose (lib: String, file: String, member: String) -> ();
    // --- PDM (320-339)
    /// PDM: a source member's lines; `None` when the file or member is
    /// missing.
    320 => fn pdm_read / PdmRead (lib: String, file: String, member: String) -> Result<Option<Vec<String>>, String>;
    /// PDM: a source member's lines replaced, file and member made when
    /// missing.
    321 => fn pdm_write / PdmWrite (lib: String, file: String, member: String, lines: Vec<String>, srctype: String) -> Result<(), String>;
    /// PDM: a user-defined options member (the shared `QGPL/QAUOOPT` as
    /// the system reads it).
    322 => fn pdm_options / PdmOptions (lib: String, file: String, member: String) -> Result<Option<Vec<String>>, String>;
    /// PDM: a user-defined options member written.
    323 => fn pdm_options_put / PdmOptionsPut (lib: String, file: String, member: String, lines: Vec<String>) -> Result<(), String>;
    /// PDM: the job's user's session defaults, blank when none.
    324 => fn pdm_defaults / PdmDefaults () -> String;
    /// PDM: the job's user's session defaults stored.
    325 => fn pdm_defaults_put / PdmDefaultsPut (text: String) -> Result<(), String>;
    // --- SDA (340-359)
    // --- DFU and RLU (360-379)
    /// DFU: each record format's key fields, in key order.
    360 => fn dfu_keys / DfuKeys (lib: String, file: String) -> Result<Vec<(String, Vec<String>)>, String>;
    /// DFU: a DFU program's real library and definition; `None` when
    /// there is none.
    361 => fn dfu_pgm / DfuPgm (lib: String, name: String) -> Result<Option<(String, String)>, String>;
    /// DFU: a DFU program created (or a DFU program of the name
    /// replaced); its real library.
    362 => fn dfu_pgm_put / DfuPgmPut (lib: String, name: String, def: String, text: String) -> Result<String, String>;
    /// DFU: a DFU program deleted.
    363 => fn dfu_pgm_delete / DfuPgmDelete (lib: String, name: String) -> Result<(), String>;
    // --- STRDBG screens (380-399)
    /// The debugger: a program's modules with their source and
    /// statement lines, for Display Module Source.
    380 => fn dbg_program / DbgProgramReq (lib: String, name: String) -> Result<DbgProgram, String>;
    /// `STRSRVJOB`: this job services the job named `number/user/name`.
    381 => fn srvjob_start / SrvJobStart (job: String) -> Result<String, String>;
    /// `ENDSRVJOB`: the serviced job is let go; its name.
    382 => fn srvjob_end / SrvJobEnd () -> Result<String, String>;
    /// The job this one services, `number/user/name`; empty for none.
    383 => fn srvjob_serving / SrvJobServing () -> String;
    /// The servicing job: an order for the serviced job.
    384 => fn srvjob_order / SrvJobOrder (order: DbgOrder) -> Result<(), String>;
    /// The serviced job: the orders waiting for it.
    385 => fn srvjob_orders / SrvJobOrders () -> Vec<DbgOrder>;
    /// The serviced job: news for the job servicing it.
    386 => fn srvjob_event / SrvJobEvent (event: DbgEvent) -> Result<(), String>;
    /// The servicing job: the news waiting for it.
    387 => fn srvjob_events / SrvJobEvents () -> Vec<DbgEvent>;
    // --- STRSQL (400-419)
    /// STRSQL: the job's SQL session started with its naming (`*SYS`,
    /// `*SQL`) and commitment control (`*NONE`, `*CHG`, `*CS`,
    /// `*ALL`); whether it started a definition (one already active is
    /// kept and left alone).
    400 => fn isql_start / IsqlStart (naming: String, commit: String) -> Result<bool, String>;
    /// STRSQL: one statement run (or with `check` only parsed), at most
    /// `max_rows` rows of a query returned.
    401 => fn isql_run / IsqlRun (text: String, naming: String, check: bool, max_rows: u32) -> IsqlReply;
    /// STRSQL: the session ended, pending changes committed or rolled
    /// back; how many there were.
    402 => fn isql_end / IsqlEnd (commit: bool) -> Result<u64, String>;
    /// STRSQL: the job's user's saved session (`QUSRSYS/QSQLSESS`).
    403 => fn isql_session / IsqlSession () -> Option<String>;
    /// STRSQL: the job's user's session saved, or with `None` dropped.
    404 => fn isql_session_put / IsqlSessionPut (text: Option<String>) -> Result<(), String>;
    // --- STRSST (420-439)
    /// SST: sign a service tools user on for this job (the job's user
    /// needs `*SERVICE`): the privileges, or why not.
    420 => fn sst_sign_on / SstSignOn (user: String, password: String) -> Result<svctools::Privilege, svctools::users::SignOnError>;
    /// SST: change an expired password, signing the user on.
    421 => fn sst_change_password / SstChangePassword (user: String, old: String, new: String) -> Result<svctools::Privilege, String>;
    /// SST: the job's service tools user signs off.
    422 => fn sst_sign_off / SstSignOff () -> ();
    /// SST: the configured units and the disks in no ASP.
    423 => fn sst_disks / SstDisks () -> Result<(Vec<svctools::env::DiskUnit>, Vec<svctools::env::NonConfigured>), String>;
    /// SST: a disk change as DST makes it (`add`, `remove`, `mirror`,
    /// `unmirror`, `suspend`, `resume`, `replace`, `threshold` with the
    /// percent as `unit`); the unit number a new unit got.
    424 => fn sst_disk_change / SstDiskChange (op: String, unit: u16, device: String) -> Result<u16, String>;
    /// SST: the ASP threshold and the parity-capable controllers.
    425 => fn sst_disk_facts / SstDiskFacts () -> Result<(u8, Vec<String>), String>;
    /// SST: the product activity log and the LIC log's lines.
    426 => fn sst_logs / SstLogs () -> Result<(Vec<svctools::env::LogEntry>, Vec<String>), String>;
    /// SST: the hardware resource table and the processor count.
    427 => fn sst_resources / SstResources () -> Result<(Vec<svctools::env::ResourceRow>, u32), String>;
    /// SST: a page of a unit (from 0), or an object header by ID.
    428 => fn sst_storage / SstStorage (unit: u16, page: u64, id: String) -> Result<Vec<u8>, String>;
    /// SST: the security flags as their byte.
    429 => fn sst_security / SstSecurity () -> Result<u8, String>;
    /// SST: the security flags changed.
    430 => fn sst_set_security / SstSetSecurity (flags: u8) -> Result<(), String>;
    /// SST: the user IDs without their password hashes, as the LIC
    /// record encodes them.
    431 => fn sst_users / SstUsers () -> Result<Vec<u8>, String>;
    /// SST: a change to the user IDs (`create`, `delete`, `enable`,
    /// `disable`, `privileges`, `reset`); the message of a refusal.
    432 => fn sst_user_change / SstUserChange (op: String, name: String, password: String, text: String, privileges: svctools::Privilege) -> Result<(), String>;
    // --- the integrated file system (440-459)
    /// One IFS operation for the job (`crate::ifs`): descriptors,
    /// directory streams and locks are the connection's and end with it.
    440 => fn ifs / IfsReq (op: crate::ifs::IfsOp) -> crate::ifs::IfsReply;

    // --- host servers (720-739; net/hostserver, stgsvc/hostsvr.rs)
    /// Prove `user` to a host server with a password substitute over
    /// the connection's seeds, or a profile token when `token` is not
    /// empty: checked as a sign-on (`QMAXSIGN`, audit). With `swap` the
    /// job then runs as the user (`QSYGETPH` and `QWTSETP`).
    720 => fn host_signon / HostSignon (user: String, proof: sec::signon::Substitute, token: Vec<u8>, swap: bool) -> Result<HostSignedOn, HostRefused>;
    /// A profile token for `user` proved as `host_signon` proves it:
    /// `kind` single use (`0xF1`), multiple use (`0xF2`) or regenerable
    /// (`0xF3`), good for `timeout` seconds.
    721 => fn host_token / HostToken (user: String, proof: sec::signon::Substitute, token: Vec<u8>, kind: u8, timeout: u32) -> Result<Vec<u8>, HostRefused>;
    /// Change `user`'s password: the old one proved by the substitute,
    /// the new one protected with the old one's token (`new_len` bytes
    /// for the SHA forms).
    722 => fn host_chgpwd / HostChgPwd (user: String, proof: sec::signon::Substitute, new_protected: Vec<u8>, new_len: u32) -> Result<(), HostRefused>;
    /// The job runs as its own user again (a prestart job between
    /// connections).
    723 => fn host_reset / HostReset () -> ();

    // --- communications objects (1180-1189; net/cmncfg, stgsvc/cmn.rs)
    /// A communications display's data: `LIND` (`WRKLIND`; args the
    /// name or generic name), `DSPLIND` (args the line), `CFGSTS`
    /// (`WRKCFGSTS`; args type, description, range, status filter).
    1180 => fn cmn_panel / CmnPanel (what: String, args: Vec<String>) -> Result<Sys2Data, String>;
    /// A line's, controller's or device's status for `RTVCFGSTS`
    /// (`ty` `*LIN`, `*CTL`, `*DEV`): its code and text, `None` when
    /// there is no such object.
    1181 => fn cfg_status / CfgStatus (name: String, ty: String) -> Option<(u16, String)>;
    // --- the small servers and their clients (1200-1209; SNDTCPSPLF, LPR)
    /// The spooled file `file` of `job` (`*` for the caller's,
    /// `number/user/name` otherwise) numbered `splnbr` (`*ONLY`,
    /// `*LAST`, a number), read as `DSPSPLF` reads it: its attributes
    /// and data.
    1200 => fn splf_fetch / SplfFetch (file: String, job: String, splnbr: String) -> Result<(SplfAttrs, Vec<u8>), String>;

    // --- certificates (1240-1249; net/certs admin, stgsvc/certs.rs)
    /// An application ID's certificate and trust from the `*SYSTEM`
    /// store, which only the service opens (its stash key is the LIC
    /// partition's). An empty ID asks for the trusted CAs alone (any
    /// job); a certificate goes only to the system's server profiles
    /// or a user with `*ALLOBJ` and `*IOSYSCFG`.
    1240 => fn cert_app / CertApp (app_id: String) -> Result<CertTrust, String>;
    /// A certificate display's data: `CRT` (`QSWRKCRT`; args the store
    /// and its password), `DSPCRT` (store, password, label), `APPID`
    /// (`QSWRKAPPID`).
    1241 => fn cert_panel / CertPanel (what: String, args: Vec<String>) -> Result<Sys2Data, String>;
    /// The expiry scan: warnings go to `QSYSOPR`; the lines for the log.
    1242 => fn cert_expiry / CertExpiry () -> Vec<String>;
    // --- the update service (1250; lic/slic/src/cpp/updcmd.rs)
    /// The hourly look at the update check schedule (`QSCHGUPDA`): the
    /// check runs when it is due; the lines for the log.
    1250 => fn upd_tick / UpdTick () -> Vec<String>;
    // --- TCP/IP (690; crate::tcp)
    /// TCP/IP configuration and operation: one operation and its answer.
    690 => fn tcp / Tcp (op: crate::tcp::TcpOp) -> crate::tcp::TcpReply;
    // --- QSH and environment variables (1300; crate::qsh, stgsvc/qsh.rs)
    /// System-level environment variables and the profiles' IDs.
    1300 => fn qsh / QshReq (op: crate::qsh::QshOp) -> crate::qsh::QshReply;
    // --- licensed programs (1320-1329; licpgm, cpp/lpcmd.rs)
    /// A licensed program display's rows: `INSTALLED` (GO LICPGM 10:
    /// product, option, status, text, release), `SFWRSC`, `LICINF`
    /// (args the product or none), `AGREEMENTS` (product, release, text,
    /// status), `AGRTEXT` (args product and release: the text, one row
    /// per line), `ACCEPT` and `DECLINE` (args product and release),
    /// `MEDIA` (args the save file `LIB/NAME`: product, option, type,
    /// load ID, release, text of each load in it).
    1320 => fn licpgm_panel / LicpgmPanel (what: String, args: Vec<String>) -> Result<Vec<Vec<String>>, String>;
    /// A product or license API (`QSZRTVPR`, `QSZCRTPD`, ... `QLZAREQ`)
    /// over its parameters' bytes: the receiver bytes, or the message
    /// ID and its data for the error code parameter.
    1321 => fn licpgm_api / LicpgmApi (api: String, parms: Vec<Vec<u8>>) -> Result<Vec<u8>, (String, Vec<u8>)>;
}

// --- certificate types

/// What `cert_app` answers: the certificate a server presents (none
/// when `label` is empty) and the CAs it trusts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CertTrust {
    /// The label in `*SYSTEM`; empty for none.
    pub label: String,
    /// The chain, leaf first, DER.
    pub chain: Vec<Vec<u8>>,
    /// The private key, PKCS#8 DER.
    pub key: Vec<u8>,
    /// Client authentication: 0 none, 1 optional, 2 required.
    pub client_auth: u8,
    /// The trusted CAs, DER.
    pub roots: Vec<Vec<u8>>,
}

wire_struct!(CertTrust {
    label,
    chain,
    key,
    client_auth,
    roots
});

// --- host server types

/// A host server sign-on that succeeded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostSignedOn {
    /// The profile.
    pub user: String,
    /// This sign-on, Unix seconds.
    pub now: u64,
    /// The one before, Unix seconds, 0 for none.
    pub previous: u64,
    /// When the password expires, Unix seconds, 0 for never.
    pub expires: u64,
    /// `QPWDEXPWRN`.
    pub warning_days: u32,
}

wire_struct!(HostSignedOn {
    user,
    now,
    previous,
    expires,
    warning_days
});

/// A host server sign-on refused: the datastream's return code and the
/// message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostRefused {
    /// Return code (`hostsvr::common::rc`).
    pub rc: u32,
    /// Message identifier.
    pub msgid: String,
    /// Its text.
    pub text: String,
}

wire_struct!(HostRefused { rc, msgid, text });

/// A search index topic: title, root words, panel group (`LIB/NAME`)
/// and help module.
pub type SchidxEntry = (String, Vec<String>, String, String);

// --- STRSQL types

/// What a statement run by `isql_run` left.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IsqlReply {
    /// `SQLCODE`.
    pub sqlcode: i32,
    /// `SQLSTATE`.
    pub sqlstate: String,
    /// The message's identifier, blank for a statement that ran.
    pub msgid: String,
    /// The message's text.
    pub text: String,
    /// The statement's name (`SELECT`, `INSERT`, ...).
    pub statement: String,
    /// Rows changed.
    pub count: u64,
    /// A query's columns: name and SQL type.
    pub columns: Vec<(String, String)>,
    /// A query's rows, `None` for a null.
    pub rows: Vec<Vec<Option<String>>>,
    /// More rows than `max_rows` were left out.
    pub truncated: bool,
}

wire_struct!(IsqlReply {
    sqlcode,
    sqlstate,
    msgid,
    text,
    statement,
    count,
    columns,
    rows,
    truncated
});

/// A system reply list entry for `rpyl_entries`: sequence, message
/// identifier, compare data, its start, reply, dump.
pub type RpyRow = (u16, String, String, u16, String, bool);

/// A job's group for `grp_info`: the group's number, the job's group
/// job name, and every member (group job name, job number, text).
pub type GroupInfo = (u32, String, Vec<(String, u32, String)>);

/// A job at a display for `ws_set`/`ws_get`: job number, user, name
/// and side (1 primary, 2 secondary).
pub type WsJob = (u32, String, String, u8);

// --- panel data: work management types

/// One `WRKACTJOB` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActJob {
    /// Shown under its subsystem.
    pub under: bool,
    /// The job.
    pub job: JobInfo,
    /// CPU % in the interval, tenths.
    pub cpu: u32,
    /// CPU used since it started, tenths of a second.
    pub cpu_secs: u64,
    /// Its pool's system number.
    pub pool: u16,
    /// `RUNPTY`.
    pub runpty: u8,
    /// Auxiliary I/O in the interval.
    pub aux_io: u64,
    /// Threads.
    pub threads: u32,
    /// Temporary storage, megabytes.
    pub tmp_mb: u64,
}

wire_struct!(ActJob {
    under,
    job,
    cpu,
    cpu_secs,
    pool,
    runpty,
    aux_io,
    threads,
    tmp_mb
});

/// `WRKACTJOB`'s data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActJobs {
    /// The machine's CPU % in the interval, tenths.
    pub cpu: u32,
    /// The interval, seconds.
    pub elapsed: u64,
    /// The jobs in display order.
    pub jobs: Vec<ActJob>,
}

wire_struct!(ActJobs { cpu, elapsed, jobs });

/// One job for `DSPJOB` and `WRKJOB`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobDetail {
    /// The job.
    pub job: JobInfo,
    /// Its attributes.
    pub attrs: JobAttrs,
    /// CPU used, milliseconds.
    pub cpu_ms: u64,
    /// Threads.
    pub threads: u32,
    /// Temporary storage, megabytes.
    pub tmp_mb: u64,
    /// Auxiliary I/O requests.
    pub aux_io: u64,
    /// Its subsystem's pool identifier, 0 for none.
    pub sbs_pool: u8,
}

wire_struct!(JobDetail {
    job,
    attrs,
    cpu_ms,
    threads,
    tmp_mb,
    aux_io,
    sbs_pool
});

/// A display's data for `panel2_data`: the header's variables, then
/// the list's entries.
pub type PanelData = (Vec<(String, String)>, Vec<Vec<(String, String)>>);

// --- panels batch 2: programs, config, security, editors types

/// A panel's data: its variables, then each list's name and entries
/// as `(variable, value)` pairs.
pub type Sys2Data = (Vec<(String, String)>, Vec<(String, Vec<Vec<(String, String)>>)>);
/// A source member as SEU opens it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SrcMember {
    /// The file's real library.
    pub lib: String,
    /// The file.
    pub file: String,
    /// The member.
    pub member: String,
    /// Its source type; blank for none.
    pub srctype: String,
    /// Its text.
    pub text: String,
    /// The data length of a record.
    pub width: u32,
    /// Whether the member exists (a new one is added when saved).
    pub exists: bool,
    /// Sequence in hundredths, date `yymmdd`, data.
    pub records: Vec<(u32, u32, String)>,
}

wire_struct!(SrcMember {
    lib,
    file,
    member,
    srctype,
    text,
    width,
    exists,
    records
});

// --- STRDBG screens types

/// One module of a program under debug.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DbgModule {
    /// The module.
    pub name: String,
    /// The source lines; empty when not available.
    pub lines: Vec<String>,
    /// `(line, statement)` pairs.
    pub stmts: Vec<(u32, u32)>,
    /// The view the module was compiled with (`DBGVIEW`).
    pub view: String,
}

wire_struct!(DbgModule {
    name,
    lines,
    stmts,
    view
});

/// A program under debug.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DbgProgram {
    /// Its library.
    pub lib: String,
    /// Its name.
    pub name: String,
    /// `*PGM` or `*SRVPGM`.
    pub kind: String,
    /// Its modules.
    pub modules: Vec<DbgModule>,
}

wire_struct!(DbgProgram {
    lib,
    name,
    kind,
    modules
});

/// What a servicing job asks of the job it services.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DbgOrder {
    /// `STRDBG`: a program, `LIB/NAME`, under debug.
    Start {
        /// The program.
        program: String,
        /// `UPDPROD`.
        updprod: bool,
    },
    /// `ENDDBG`.
    End,
    /// The breakpoints now: program and statement.
    Breakpoints {
        /// The list.
        list: Vec<(String, u32)>,
    },
    /// Go on from a stop.
    Resume {
        /// Step rather than run.
        step: bool,
        /// Into calls.
        into: bool,
    },
    /// Evaluate a variable at the stop.
    Eval {
        /// The expression with its format suffix.
        expr: String,
    },
}

ipc::wire_enum!(DbgOrder {
    0 => Start { program, updprod },
    1 => End,
    2 => Breakpoints { list },
    3 => Resume { step, into },
    4 => Eval { expr },
});

/// What a serviced job tells the job servicing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DbgEvent {
    /// Stopped at a statement.
    Stopped {
        /// `LIB/NAME`.
        program: String,
        /// The procedure.
        procedure: String,
        /// The statement.
        stmt: u32,
    },
    /// The answer to an order (`EVAL`'s value, `STRDBG`'s result).
    Answer {
        /// The text.
        text: String,
    },
    /// The program ended or the job went on running.
    Running,
}

ipc::wire_enum!(DbgEvent {
    0 => Stopped { program, procedure, stmt },
    1 => Answer { text },
    2 => Running,
});
