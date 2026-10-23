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
use wm::wtr::WtrStep;

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
    // --- UIM APIs (220-239)
    // --- panels batch 2: work, spool, journals (240-269)
    // --- panels batch 2: programs, config, security, editors (270-299)
    // --- SEU (300-319)
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
    // --- STRDBG screens (380-399)
    // --- STRSQL (400-419)
    // --- STRSST (420-439)
}

/// A system reply list entry for `rpyl_entries`: sequence, message
/// identifier, compare data, its start, reply, dump.
pub type RpyRow = (u16, String, String, u16, String, bool);

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
