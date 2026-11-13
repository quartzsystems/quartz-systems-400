// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `QSYS2` and `SYSTOOLS` services: views, table functions, scalar
//! functions and procedures, with pub400's columns ([`shape`]) and
//! values from the store or the [`host::SystemHost`]. A column QS/400
//! has no value for is null; each service lists the columns of the
//! oracle's that are `NOT NULL` and still have none here (`NO_VALUE`),
//! and those are nullable in its description.

// Every service's rows function has the one signature, failing or not.
#![allow(clippy::unnecessary_wraps)]

pub mod host;
pub mod http;
pub mod shape;

mod args;
mod out;

mod absent;
mod admin;
mod gensql;
mod history;
mod ifs;
mod joblog;
mod jobs;
mod journals;
mod jrnent;
mod libl;
mod locks;
mod msgfdata;
mod netstat;
mod netsvr;
mod nfs;
mod objects;
pub mod plancache;
pub mod procs;
mod products;
pub mod qaqqini;
mod secinfo;
mod security;
mod spool;
mod sysconf;
mod sysres;
mod sysval;
mod users;
mod wmobj;

pub use args::Args;
pub use out::Out;

use store::txn::Txn;

use super::super::SqlContext;
use super::Table;
use crate::ast::ObjectName;
use crate::error::{Result, SqlError};

/// A view's rows.
type ViewFn = fn(&mut Txn<'_>, &SqlContext, &mut Out) -> Result<()>;
/// A table function's rows for its arguments.
type TableFn = fn(&mut Txn<'_>, &SqlContext, &Args, &mut Out) -> Result<()>;

/// A view: schema, name, the oracle's `NOT NULL` columns with no value
/// here, its rows.
struct View {
    schema: &'static str,
    name: &'static str,
    no_value: &'static [&'static str],
    rows: ViewFn,
}

/// A table function, by specific name.
struct Function {
    schema: &'static str,
    specific: &'static str,
    no_value: &'static [&'static str],
    rows: TableFn,
}

/// The views.
const VIEWS: &[View] = &[
    View {
        schema: "QSYS2",
        name: "LIBRARY_LIST_INFO",
        no_value: &[],
        rows: libl::rows,
    },
    View {
        schema: "QSYS2",
        name: "SYSTEM_VALUE_INFO",
        no_value: &[],
        rows: sysval::rows,
    },
    View {
        schema: "QSYS2",
        name: "SYSTEM_STATUS_INFO",
        no_value: jobs::STATUS_NO_VALUE,
        rows: jobs::status,
    },
    View {
        schema: "QSYS2",
        name: "SYSTEM_STATUS_INFO_BASIC",
        no_value: jobs::STATUS_NO_VALUE,
        rows: jobs::status,
    },
    View {
        schema: "QSYS2",
        name: "USER_INFO",
        no_value: users::NO_VALUE,
        rows: users::info,
    },
    View {
        schema: "QSYS2",
        name: "USER_INFO_BASIC",
        no_value: users::NO_VALUE,
        rows: users::info,
    },
    View {
        schema: "QSYS2",
        name: "GROUP_PROFILE_ENTRIES",
        no_value: &[],
        rows: users::groups,
    },
    View {
        schema: "QSYS2",
        name: "OUTPUT_QUEUE_ENTRIES",
        no_value: spool::ENTRY_NO_VALUE,
        rows: spool::entries,
    },
    View {
        schema: "QSYS2",
        name: "OUTPUT_QUEUE_ENTRIES_BASIC",
        no_value: spool::ENTRY_NO_VALUE,
        rows: spool::entries,
    },
    View {
        schema: "QSYS2",
        name: "OUTPUT_QUEUE_INFO",
        no_value: spool::QUEUE_NO_VALUE,
        rows: spool::queues,
    },
    View {
        schema: "QSYS2",
        name: "JOB_DESCRIPTION_INFO",
        no_value: wmobj::JOBD_NO_VALUE,
        rows: wmobj::jobds,
    },
    View {
        schema: "QSYS2",
        name: "JOURNAL_INFO",
        no_value: journals::JOURNAL_NO_VALUE,
        rows: journals::journals,
    },
    View {
        schema: "QSYS2",
        name: "JOURNAL_RECEIVER_INFO",
        no_value: &[],
        rows: journals::receivers,
    },
    View {
        schema: "QSYS2",
        name: "JOURNALED_OBJECTS",
        no_value: &[],
        rows: journals::objects,
    },
    View {
        schema: "QSYS2",
        name: "OBJECT_PRIVILEGES",
        no_value: &[],
        rows: security::privileges,
    },
    View {
        schema: "QSYS2",
        name: "OBJECT_OWNERSHIP",
        no_value: &[],
        rows: security::ownership,
    },
    View {
        schema: "QSYS2",
        name: "AUTHORIZATION_LIST_INFO",
        no_value: &[],
        rows: security::autl_objects,
    },
    View {
        schema: "QSYS2",
        name: "AUTHORIZATION_LIST_USER_INFO",
        no_value: &[],
        rows: security::autl_users,
    },
    View {
        schema: "QSYS2",
        name: "USER_STORAGE",
        no_value: security::STORAGE_NO_VALUE,
        rows: security::storage,
    },
    View {
        schema: "QSYS2",
        name: "JOB_QUEUE_INFO",
        no_value: &[],
        rows: wmobj::jobqs,
    },
    View {
        schema: "QSYS2",
        name: "SUBSYSTEM_INFO",
        no_value: wmobj::SBS_NO_VALUE,
        rows: wmobj::subsystems,
    },
    View {
        schema: "QSYS2",
        name: "SUBSYSTEM_POOL_INFO",
        no_value: &[],
        rows: wmobj::subsystem_pools,
    },
    View {
        schema: "QSYS2",
        name: "REPLY_LIST_INFO",
        no_value: &[],
        rows: wmobj::reply_list,
    },
    View {
        schema: "QSYS2",
        name: "MESSAGE_QUEUE_INFO",
        no_value: &[],
        rows: wmobj::messages,
    },
    View {
        schema: "QSYS2",
        name: "SCHEDULED_JOB_INFO",
        no_value: &[],
        rows: wmobj::schedule,
    },
    View {
        schema: "QSYS2",
        name: "SERVER_SHARE_INFO",
        no_value: &[],
        rows: netsvr::shares,
    },
    View {
        schema: "QSYS2",
        name: "NFS_EXPORT_INFO",
        no_value: &[],
        rows: nfs::exports,
    },
    View {
        schema: "QSYS2",
        name: "MOUNTED_FS_INFO",
        no_value: &[],
        rows: nfs::mounts,
    },
    View {
        schema: "QSYS2",
        name: "SOFTWARE_PRODUCT_INFO",
        no_value: products::PRODUCT_NO_VALUE,
        rows: products::products,
    },
    View {
        schema: "QSYS2",
        name: "PTF_INFO",
        no_value: &[],
        rows: products::ptfs,
    },
    View {
        schema: "QSYS2",
        name: "NETSTAT_INFO",
        no_value: netstat::INFO_NO_VALUE,
        rows: netstat::info,
    },
    View {
        schema: "QSYS2",
        name: "NETSTAT_JOB_INFO",
        no_value: &[],
        rows: netstat::jobs,
    },
    View {
        schema: "QSYS2",
        name: "NETSTAT_INTERFACE_INFO",
        no_value: &[],
        rows: netstat::interfaces,
    },
    View {
        schema: "QSYS2",
        name: "NETSTAT_ROUTE_INFO",
        no_value: netstat::ROUTE_NO_VALUE,
        rows: netstat::routes,
    },
    View {
        schema: "QSYS2",
        name: "MESSAGE_FILE_DATA",
        no_value: msgfdata::NO_VALUE,
        rows: msgfdata::view,
    },
    View {
        schema: "QSYS2",
        name: "SECURITY_INFO",
        no_value: secinfo::NO_VALUE,
        rows: secinfo::view,
    },
    View {
        schema: "QSYS2",
        name: "FUNCTION_INFO",
        no_value: &[],
        rows: absent::view,
    },
    View {
        schema: "QSYS2",
        name: "FUNCTION_USAGE",
        no_value: &[],
        rows: absent::view,
    },
    View {
        schema: "QSYS2",
        name: "LICENSE_INFO",
        no_value: &[],
        rows: absent::view,
    },
    View {
        schema: "QSYS2",
        name: "SYSLIMTBL",
        no_value: &[],
        rows: absent::view,
    },
    View {
        schema: "QSYS2",
        name: "SYSLIMITS",
        no_value: &[],
        rows: absent::view,
    },
    View {
        schema: "QSYS2",
        name: "ELECTRONIC_SERVICE_AGENT_INFO",
        no_value: &[],
        rows: absent::view,
    },
    View {
        schema: "QSYS2",
        name: "ASP_VARY_INFO",
        no_value: &[],
        rows: absent::view,
    },
    View {
        schema: "QSYS2",
        name: "SERVER_SBS_ROUTING",
        no_value: &[],
        rows: absent::view,
    },
    View {
        schema: "QSYS2",
        name: "OBJECT_LOCK_INFO",
        no_value: &[],
        rows: locks::view,
    },
    View {
        schema: "QSYS2",
        name: "MEMORY_POOL_INFO",
        no_value: &[],
        rows: sysres::pools,
    },
    View {
        schema: "QSYS2",
        name: "ASP_INFO",
        no_value: sysres::ASP_NO_VALUE,
        rows: sysres::asp,
    },
    View {
        schema: "QSYS2",
        name: "SYSDISKSTAT",
        no_value: sysres::DISK_NO_VALUE,
        rows: sysres::disks,
    },
    View {
        schema: "QSYS2",
        name: "HARDWARE_RESOURCE_INFO",
        no_value: sysres::HW_NO_VALUE,
        rows: sysres::hardware,
    },
    View {
        schema: "QSYS2",
        name: "MEDIA_LIBRARY_INFO",
        no_value: sysconf::MLB_NO_VALUE,
        rows: sysconf::media_libraries,
    },
    View {
        schema: "QSYS2",
        name: "COLLECTION_SERVICES_INFO",
        no_value: sysconf::CS_NO_VALUE,
        rows: sysconf::collection_services,
    },
    View {
        schema: "QSYS2",
        name: "EXIT_POINT_INFO",
        no_value: &[],
        rows: sysconf::exit_points,
    },
    View {
        schema: "QSYS2",
        name: "EXIT_PROGRAM_INFO",
        no_value: sysconf::PGM_NO_VALUE,
        rows: sysconf::exit_programs,
    },
    View {
        schema: "QSYS2",
        name: "ENVIRONMENT_VARIABLE_INFO",
        no_value: &[],
        rows: sysconf::environment,
    },
    View {
        schema: "QSYS2",
        name: "GROUP_PTF_INFO",
        no_value: &[],
        rows: sysconf::ptf_groups,
    },
    View {
        schema: "SYSTOOLS",
        name: "GROUP_PTF_DETAILS",
        no_value: sysconf::DETAIL_NO_VALUE,
        rows: sysconf::ptf_details,
    },
    View {
        schema: "QSYS2",
        name: "TCPIP_INFO",
        no_value: &[],
        rows: sysconf::tcpip,
    },
];

/// The table functions.
const FUNCTIONS: &[Function] = &[
    Function {
        schema: "QSYS2",
        specific: "QSQACTJOB",
        no_value: &[],
        rows: jobs::active,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQJOBINFO",
        no_value: &[],
        rows: jobs::job_info,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQOBSTAT",
        no_value: &[],
        rows: objects::rows,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQLIBINF",
        no_value: &[],
        rows: objects::library_info,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQJOBLOG",
        no_value: &[],
        rows: joblog::rows,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQHSTLOG",
        no_value: &[],
        rows: history::rows,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQIFSOBJS",
        no_value: &[],
        rows: ifs::statistics,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQIFSREAD",
        no_value: &[],
        rows: ifs::read_text,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQIFSREADUTF8",
        no_value: &[],
        rows: ifs::read_text,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQIFSREADBINARY",
        no_value: &[],
        rows: ifs::read_binary,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "SPOOL_FILE",
        no_value: &[],
        rows: spool::data,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQSPLFINF",
        no_value: &[],
        rows: spool::info,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQSYSSTAT",
        no_value: jobs::STATUS_NO_VALUE,
        rows: jobs::status_function,
    },
    Function {
        schema: "QSYS2",
        specific: "HTTP_GETV",
        no_value: &[],
        rows: http::get_v,
    },
    Function {
        schema: "QSYS2",
        specific: "HTTP_GETBV",
        no_value: &[],
        rows: http::get_bv,
    },
    Function {
        schema: "QSYS2",
        specific: "HTTP_POSTV",
        no_value: &[],
        rows: http::post_v,
    },
    Function {
        schema: "QSYS2",
        specific: "HTTP_POSTBV",
        no_value: &[],
        rows: http::post_bv,
    },
    Function {
        schema: "QSYS2",
        specific: "HTTP_PUTV",
        no_value: &[],
        rows: http::put_v,
    },
    Function {
        schema: "QSYS2",
        specific: "HTTP_PUTBV",
        no_value: &[],
        rows: http::put_bv,
    },
    Function {
        schema: "QSYS2",
        specific: "HTTP_PATCHV",
        no_value: &[],
        rows: http::patch_v,
    },
    Function {
        schema: "QSYS2",
        specific: "HTTP_PTCBV",
        no_value: &[],
        rows: http::patch_bv,
    },
    Function {
        schema: "QSYS2",
        specific: "HTTP_DELV",
        no_value: &[],
        rows: http::delete_v,
    },
    Function {
        schema: "QSYS2",
        specific: "HTTP_DELBV",
        no_value: &[],
        rows: http::delete_bv,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQMSGFDTA",
        no_value: &[],
        rows: msgfdata::function,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQSECINF",
        no_value: &[],
        rows: secinfo::function,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQSACONN",
        no_value: &[],
        rows: absent::function,
    },
    Function {
        schema: "QSYS2",
        specific: "QJOJRNTF",
        no_value: &[],
        rows: jrnent::display,
    },
    Function {
        schema: "QSYS2",
        specific: "VARYINFO",
        no_value: &[],
        rows: absent::function,
    },
    Function {
        schema: "QSYS2",
        specific: "QSYSVRSBSR",
        no_value: &[],
        rows: absent::function,
    },
    Function {
        schema: "QSYS2",
        specific: "OBJECT_LOCK_INFO",
        no_value: &[],
        rows: locks::function,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQPOOL2",
        no_value: &[],
        rows: sysres::pool_function,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQSYSACTI",
        no_value: &[],
        rows: sysres::activity,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQASPINFO",
        no_value: &[],
        rows: sysres::asp_function,
    },
    Function {
        schema: "QSYS2",
        specific: "SYSDSKSTAT",
        no_value: &[],
        rows: sysres::disk_function,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQHDWRSC",
        no_value: &[],
        rows: sysres::hardware_function,
    },
    Function {
        schema: "QSYS2",
        specific: "QYPSCSINFO",
        no_value: &[],
        rows: sysconf::collection_services_function,
    },
    Function {
        schema: "QSYS2",
        specific: "GROUP_PTF_INFO",
        no_value: &[],
        rows: sysconf::ptf_groups_function,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "GROUP00001",
        no_value: &[],
        rows: sysconf::ptf_details_function,
    },
    Function {
        schema: "QSYS2",
        specific: "TCPIP_INFO_FUNC",
        no_value: &[],
        rows: sysconf::tcpip_function,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQDNSLKUP",
        no_value: &[],
        rows: sysconf::dns,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "CHGUSRPRF",
        no_value: &[],
        rows: admin::change_user_profile,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQIFSOBJP",
        no_value: &[],
        rows: ifs::privileges,
    },
    Function {
        schema: "QSYS2",
        specific: "QSQIFSOBJL",
        no_value: &[],
        rows: ifs::locks,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_AD",
        no_value: &[],
        rows: jrnent::audit::<b'A', b'D'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_AF",
        no_value: &[],
        rows: jrnent::audit::<b'A', b'F'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_AP",
        no_value: &[],
        rows: jrnent::audit::<b'A', b'P'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_AU",
        no_value: &[],
        rows: jrnent::audit::<b'A', b'U'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_AX",
        no_value: &[],
        rows: jrnent::audit::<b'A', b'X'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_CA",
        no_value: &[],
        rows: jrnent::audit::<b'C', b'A'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_CD",
        no_value: &[],
        rows: jrnent::audit::<b'C', b'D'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_CO",
        no_value: &[],
        rows: jrnent::audit::<b'C', b'O'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_CP",
        no_value: &[],
        rows: jrnent::audit::<b'C', b'P'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_DO",
        no_value: &[],
        rows: jrnent::audit::<b'D', b'O'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_DS",
        no_value: &[],
        rows: jrnent::audit::<b'D', b'S'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_EV",
        no_value: &[],
        rows: jrnent::audit::<b'E', b'V'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_GR",
        no_value: &[],
        rows: jrnent::audit::<b'G', b'R'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_IM",
        no_value: &[],
        rows: jrnent::audit::<b'I', b'M'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_JD",
        no_value: &[],
        rows: jrnent::audit::<b'J', b'D'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_JS",
        no_value: &[],
        rows: jrnent::audit::<b'J', b'S'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_LD",
        no_value: &[],
        rows: jrnent::audit::<b'L', b'D'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_M0",
        no_value: &[],
        rows: jrnent::audit::<b'M', b'0'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_M6",
        no_value: &[],
        rows: jrnent::audit::<b'M', b'6'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_M7",
        no_value: &[],
        rows: jrnent::audit::<b'M', b'7'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_M8",
        no_value: &[],
        rows: jrnent::audit::<b'M', b'8'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_M9",
        no_value: &[],
        rows: jrnent::audit::<b'M', b'9'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_NA",
        no_value: &[],
        rows: jrnent::audit::<b'N', b'A'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_OM",
        no_value: &[],
        rows: jrnent::audit::<b'O', b'M'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_OR",
        no_value: &[],
        rows: jrnent::audit::<b'O', b'R'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_OW",
        no_value: &[],
        rows: jrnent::audit::<b'O', b'W'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_PA",
        no_value: &[],
        rows: jrnent::audit::<b'P', b'A'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_PF",
        no_value: &[],
        rows: jrnent::audit::<b'P', b'F'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_PG",
        no_value: &[],
        rows: jrnent::audit::<b'P', b'G'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_PO",
        no_value: &[],
        rows: jrnent::audit::<b'P', b'O'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_PS",
        no_value: &[],
        rows: jrnent::audit::<b'P', b'S'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_PU",
        no_value: &[],
        rows: jrnent::audit::<b'P', b'U'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_PW",
        no_value: &[],
        rows: jrnent::audit::<b'P', b'W'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_RA",
        no_value: &[],
        rows: jrnent::audit::<b'R', b'A'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_RO",
        no_value: &[],
        rows: jrnent::audit::<b'R', b'O'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_RP",
        no_value: &[],
        rows: jrnent::audit::<b'R', b'P'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_RU",
        no_value: &[],
        rows: jrnent::audit::<b'R', b'U'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_RZ",
        no_value: &[],
        rows: jrnent::audit::<b'R', b'Z'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_SF",
        no_value: &[],
        rows: jrnent::audit::<b'S', b'F'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_SK",
        no_value: &[],
        rows: jrnent::audit::<b'S', b'K'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_SM",
        no_value: &[],
        rows: jrnent::audit::<b'S', b'M'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_ST",
        no_value: &[],
        rows: jrnent::audit::<b'S', b'T'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_SV",
        no_value: &[],
        rows: jrnent::audit::<b'S', b'V'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_VO",
        no_value: &[],
        rows: jrnent::audit::<b'V', b'O'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_VP",
        no_value: &[],
        rows: jrnent::audit::<b'V', b'P'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_ZC",
        no_value: &[],
        rows: jrnent::audit::<b'Z', b'C'>,
    },
    Function {
        schema: "SYSTOOLS",
        specific: "AUDIT_ZR",
        no_value: &[],
        rows: jrnent::audit::<b'Z', b'R'>,
    },
];

fn schema_of(name: &ObjectName) -> Option<&str> {
    name.schema.as_ref().map(|s| s.value.as_str())
}

fn view_of(name: &ObjectName) -> Option<&'static View> {
    VIEWS
        .iter()
        .find(|v| v.name == name.name.value && schema_of(name).is_none_or(|s| s == v.schema))
}

/// Whether `name` is one of the service views.
pub fn has(name: &ObjectName) -> bool {
    view_of(name).is_some()
}

/// A service view's columns and rows.
pub fn table(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    name: &ObjectName,
    alias: &str,
) -> Result<Option<Table>> {
    let Some(v) = view_of(name) else {
        return Ok(None);
    };
    let shape = shape::view(v.schema, v.name).ok_or_else(|| missing(v.name))?;
    let mut out = Out::new(shape, alias, v.no_value);
    (v.rows)(txn, ctx, &mut out)?;
    Ok(Some(out.finish()))
}

/// A service view's columns only, its rows not computed: what a
/// `DESCRIBE` needs.
#[must_use]
pub fn columns(name: &ObjectName, alias: &str) -> Option<Table> {
    let v = view_of(name)?;
    let shape = shape::view(v.schema, v.name)?;
    Some(Out::new(shape, alias, v.no_value).finish())
}

fn missing(name: &str) -> SqlError {
    SqlError::Database {
        what: format!("the shape of {name} is not in the shapes table"),
        msgid: None,
    }
}

/// A table function's columns and rows for `args` (positional, or
/// `name => value`), `None` when `name` is not a service table
/// function.
pub fn table_function(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    name: &ObjectName,
    args: Vec<(Option<String>, crate::value::Value)>,
    alias: &str,
) -> Result<Option<Table>> {
    let schema = schema_of(name);
    for f in FUNCTIONS {
        if schema.is_some_and(|s| s != f.schema) {
            continue;
        }
        let Some(shape) = shape::routines(f.schema, &name.name.value)
            .iter()
            .find(|s| s.specific == f.specific)
        else {
            continue;
        };
        let bound = Args::bind(shape, args, ctx)?;
        let mut out = Out::new(shape, alias, f.no_value);
        (f.rows)(txn, ctx, &bound, &mut out)?;
        return Ok(Some(out.finish()));
    }
    Ok(None)
}

/// The `TABLE (f(args))` hook ([`super::FunctionProvider`]): the
/// arguments' names from their `name => value` markers.
pub(crate) fn call(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    name: &ObjectName,
    args: &[crate::value::Value],
    exprs: &[crate::ast::Expr],
    alias: &str,
) -> Result<Option<Table>> {
    let named = args
        .iter()
        .zip(exprs.iter().map(Some).chain(std::iter::repeat(None)))
        .map(|(v, e)| {
            let n = match e {
                Some(crate::ast::Expr::Function(f)) => {
                    f.name.strip_prefix("=>").map(str::to_string)
                }
                _ => None,
            };
            (n, v.clone())
        })
        .collect();
    table_function(txn, ctx, name, named, alias)
}

/// Whether `name` is a service table function.
#[must_use]
pub fn is_table_function(name: &ObjectName) -> bool {
    let schema = schema_of(name);
    FUNCTIONS.iter().any(|f| {
        schema.is_none_or(|s| s == f.schema)
            && shape::routines(f.schema, &name.name.value)
                .iter()
                .any(|s| s.specific == f.specific)
    })
}

/// The host, or the error a service needing it raises: SQL0443 as IBM
/// raises a service's failure, with our text.
pub(crate) fn host(ctx: &SqlContext) -> Result<&dyn host::SystemHost> {
    ctx.host
        .as_deref()
        .ok_or_else(|| unreachable_here("the system's state"))
}

/// SQL0443: a service could not get what it reads.
pub(crate) fn unreachable_here(what: &str) -> SqlError {
    SqlError::Coded {
        code: -443,
        state: "38501",
        what: format!("{what} cannot be read where this statement runs"),
    }
}

/// Ask the host with the statement's transaction given back, so the
/// host may use what the store lends (the system values) meanwhile.
pub(crate) fn ask<T>(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    what: &str,
    f: impl FnOnce(&dyn host::SystemHost) -> Option<T>,
) -> Result<T> {
    let h = host(ctx)?;
    let suspended = txn.suspend();
    let r = f(h);
    if suspended {
        txn.resume()?;
    }
    r.ok_or_else(|| unreachable_here(what))
}

/// Every service run once, its default arguments: the columns a
/// service set that its shape lacks (outside the `_BASIC` views) and the
/// `NOT NULL` columns it left null. For the tests, over a full host.
pub fn self_check(txn: &mut Txn<'_>, ctx: &SqlContext) -> Vec<String> {
    let mut faults = Vec::new();
    let mut note = |name: &str, r: Result<(Table, Vec<String>)>| match r {
        Ok(((schema, rows), unknown)) => {
            if !name.ends_with("_BASIC") {
                faults.extend(
                    unknown
                        .into_iter()
                        .map(|c| format!("{name}: no column {c}")),
                );
            }
            for row in &rows {
                for (c, v) in schema.cols.iter().zip(row) {
                    if !c.nullable && v.is_null() {
                        faults.push(format!("{name}: NOT NULL column {} is null", c.name));
                    }
                }
            }
        }
        Err(e) => faults.push(format!("{name}: {e}")),
    };
    for v in VIEWS {
        let r = shape::view(v.schema, v.name)
            .ok_or_else(|| missing(v.name))
            .and_then(|shape| {
                let mut out = Out::new(shape, v.name, v.no_value);
                (v.rows)(txn, ctx, &mut out)?;
                Ok(out.finish_noting())
            });
        note(v.name, r);
    }
    for f in FUNCTIONS {
        let r = shape::index_specific(f.schema, f.specific)
            .ok_or_else(|| missing(f.specific))
            .and_then(|shape| {
                // A parameter with no default gets a sample: `*ALL` for a
                // type list, QSYS for a library, a path or URL for other
                // text, 1 for a number.
                let sample = shape
                    .parms
                    .iter()
                    .filter(|p| p.mode != "OUT" && (p.default.is_empty() || p.default == "-"))
                    .map(|p| {
                        let v = if matches!(
                            p.ty.as_str(),
                            "INTEGER" | "SMALLINT" | "BIGINT" | "DECIMAL"
                        ) {
                            crate::value::Value::int(1)
                        } else if p.name == "SEARCH_NAME" {
                            crate::value::Value::Str("localhost".into())
                        } else if p.name == "JOURNAL_NAME" {
                            crate::value::Value::Str("QAUDJRN".into())
                        } else if p.name == "JOB_NAME" {
                            crate::value::Value::Str("*".into())
                        } else if p.name.contains("TYPE") || p.name.contains("LIST") {
                            crate::value::Value::Str("*ALL".into())
                        } else if p.name.contains("SCHEMA") || p.name.contains("LIBRARY") {
                            crate::value::Value::Str("QSYS".into())
                        } else {
                            crate::value::Value::Str("/".into())
                        };
                        (Some(p.name.clone()), v)
                    })
                    .collect();
                let bound = Args::bind(shape, sample, ctx)?;
                let mut out = Out::new(shape, f.specific, f.no_value);
                (f.rows)(txn, ctx, &bound, &mut out)?;
                Ok(out.finish_noting())
            });
        note(f.specific, r);
    }
    faults.sort();
    faults.dedup();
    faults
}
