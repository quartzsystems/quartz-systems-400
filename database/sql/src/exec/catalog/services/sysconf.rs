// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system's configuration as its commands show it:
//! `MEDIA_LIBRARY_INFO` (`WRKMLBSTS`), `COLLECTION_SERVICES_INFO`
//! (`CFGPFRCOL`), `EXIT_POINT_INFO` and `EXIT_PROGRAM_INFO`
//! (`WRKREGINF`), `ENVIRONMENT_VARIABLE_INFO` (`WRKENVVAR`),
//! `GROUP_PTF_INFO` and `SYSTOOLS.GROUP_PTF_DETAILS` (`WRKPTFGRP`),
//! `TCPIP_INFO` and `DNS_LOOKUP`.

use store::txn::Txn;

use super::super::super::SqlContext;
use super::{Args, Out, ask};
use crate::error::{Result, SqlError};
use crate::value::Value;

fn failed(e: String) -> SqlError {
    SqlError::Coded {
        code: -443,
        state: "38501",
        what: e,
    }
}

/// `MEDIA_LIBRARY_INFO`'s `NOT NULL` columns with no value here: the
/// virtual libraries keep no allocation priority or mount waits.
pub const MLB_NO_VALUE: &[&str] = &[
    "RESOURCE_ALLOCATION_PRIORITY",
    "INITIAL_MOUNT_WAIT_TIME",
    "END_OF_VOLUME_MOUNT_WAIT_TIME",
];

/// `MEDIA_LIBRARY_INFO`.
pub fn media_libraries(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    for m in ask(txn, ctx, "the media libraries", |h| h.media_libraries())? {
        out.row();
        out.text("DEVICE_NAME", m.device.as_str());
        out.text("DEVICE_STATUS", m.status.as_str());
        out.text("DEVICE_TYPE", m.type_model.0.as_str());
        out.text("DEVICE_MODEL", m.type_model.1.as_str());
        out.text_or_null("RESOURCE_NAME", &m.resource);
        out.text("DEVICE_DESCRIPTION", m.text.as_str());
    }
    Ok(())
}

/// `COLLECTION_SERVICES_INFO`'s `NOT NULL` columns with no value here:
/// QS/400 keeps no historical data.
pub const CS_NO_VALUE: &[&str] = &[
    "HISTORICAL_DATA_INTERVAL",
    "HISTORICAL_SUMMARY_DATA_RETENTION",
    "HISTORICAL_DETAILED_DATA_RETENTION",
];

/// `COLLECTION_SERVICES_INFO`.
pub fn collection_services(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let c = ask(txn, ctx, "Collection Services", |h| h.collection_services())?;
    out.row();
    if let Some((lib, name, prf, start)) = &c.active {
        out.text("ACTIVE_COLLECTION_LIBRARY", lib.as_str());
        out.text("ACTIVE_COLLECTION_NAME", name.as_str());
        out.text("ACTIVE_COLLECTION_PROFILE", prf.as_str());
        out.stamp("ACTIVE_COLLECTION_START_TIME", *start);
    }
    out.text("COLLECTION_LIBRARY", c.library.as_str());
    out.text("COLLECTION_PROFILE", c.profile.as_str());
    out.int("DEFAULT_COLLECTION_INTERVAL", c.interval);
    if let Some(h) = c.retention_hours {
        out.int("MGTCOL_RETENTION_PERIOD", h);
    }
    out.int("CYCLE_TIME", c.cycle.0);
    out.int("CYCLE_INTERVAL", c.cycle.1);
    out.yes("CREATE_STANDARD_DB_FILES", c.create_files);
    out.yes("CREATE_SUMMARY_DB_FILES", false);
    out.yes("CREATE_SYSTEM_MONITOR_DB_FILES", false);
    out.yes("ENABLE_SYSTEM_MONITORING", false);
    out.int("CATEGORY_LIST_COUNT", c.categories.len());
    out.text_or_null("CATEGORY_LIST", &c.categories.join(" "));
    out.int("EXCLUDED_LINE_COUNT", 0);
    out.yes("CREATE_HISTORICAL_DATA", false);
    out.yes("CREATE_HISTORICAL_DETAILED_DATA", false);
    Ok(())
}

/// `COLLECTION_SERVICES_INFO()` as a table function.
pub fn collection_services_function(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    _args: &Args,
    out: &mut Out,
) -> Result<()> {
    collection_services(txn, ctx, out)
}

/// `EXIT_POINT_INFO`.
pub fn exit_points(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    for p in ask(txn, ctx, "the registration facility", |h| h.exit_points())?.map_err(failed)? {
        out.row();
        out.text("EXIT_POINT_NAME", p.name.as_str());
        out.text("EXIT_POINT_FORMAT", p.format.as_str());
        out.yes("REGISTERED", p.registered);
        out.yes("ALLOW_DEREGISTRATION", p.allow_deregistration);
        out.yes("ALLOW_CHANGE", p.allow_change);
        out.int("EXIT_PROGRAMS", p.programs.len());
        if let Some(m) = p.max_programs {
            out.int("MAXIMUM_EXIT_PROGRAMS", m);
        }
        out.text_or_null("TEXT_DESCRIPTION", &p.text);
    }
    Ok(())
}

/// `EXIT_PROGRAM_INFO`'s `NOT NULL` columns with no value here.
pub const PGM_NO_VALUE: &[&str] = &["QMLTTHDACN_SYSTEM_VALUE"];

/// `EXIT_PROGRAM_INFO`.
pub fn exit_programs(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    for p in ask(txn, ctx, "the registration facility", |h| h.exit_points())?.map_err(failed)? {
        for g in &p.programs {
            out.row();
            out.text("EXIT_POINT_NAME", p.name.as_str());
            out.text("EXIT_POINT_FORMAT", p.format.as_str());
            out.yes("REGISTERED", p.registered);
            out.yes("COMPLETE", true);
            out.int("EXIT_PROGRAM_NUMBER", g.number);
            out.text("EXIT_PROGRAM_LIBRARY", g.library.as_str());
            out.text("EXIT_PROGRAM", g.name.as_str());
            out.text_or_null("TEXT_DESCRIPTION", &g.text);
            if !g.data.trim().is_empty() {
                out.text("EXIT_PROGRAM_DATA", g.data.as_str());
                out.int("EXIT_PROGRAM_DATA_CCSID", 37);
            }
            out.text_or_null("THREADSAFE", g.threadsafe.trim_start_matches('*'));
            out.text("MULTITHREADED_JOB_ACTION", g.action.trim_start_matches('*'));
        }
    }
    Ok(())
}

/// `ENVIRONMENT_VARIABLE_INFO`: the job's variables, then the system's.
pub fn environment(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let mut vars = ask(txn, ctx, "the environment variables", |h| {
        h.environment_variables()
    })?;
    vars.sort_by_key(|v| v.system);
    for v in vars {
        out.row();
        out.text(
            "ENVIRONMENT_VARIABLE_TYPE",
            if v.system { "SYSTEM" } else { "JOB" },
        );
        out.text("ENVIRONMENT_VARIABLE_NAME", v.name.as_str());
        out.text("ENVIRONMENT_VARIABLE_VALUE", v.value.as_str());
        let encode = |s: &str| match ccsid::conv::Codec::of(v.ccsid).ok().flatten() {
            Some(c) if v.ccsid != 1208 => {
                let mut b = Vec::new();
                for ch in s.chars() {
                    c.encode(ch, &mut b);
                }
                b
            }
            _ => s.as_bytes().to_vec(),
        };
        out.set(
            "ENVIRONMENT_VARIABLE_BINARY_NAME",
            Value::Bin(encode(&v.name)),
        );
        out.set(
            "ENVIRONMENT_VARIABLE_BINARY_VALUE",
            Value::Bin(encode(&v.value)),
        );
        out.int("ENVIRONMENT_VARIABLE_CCSID", v.ccsid);
    }
    Ok(())
}

/// `GROUP_PTF_INFO`.
pub fn ptf_groups(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let now = ctx.clock();
    for g in ask(txn, ctx, "the PTF groups", |h| h.ptf_groups())?.map_err(failed)? {
        out.row();
        if out.has("COLLECTED_TIME") {
            out.stamp("COLLECTED_TIME", now);
        }
        out.text("PTF_GROUP_NAME", g.name.as_str());
        out.text_or_null("PTF_GROUP_DESCRIPTION", &g.text);
        out.int("PTF_GROUP_LEVEL", g.level);
        out.text("PTF_GROUP_TARGET_RELEASE", g.release.as_str());
        out.text("PTF_GROUP_STATUS", g.status.as_str());
    }
    Ok(())
}

/// `GROUP_PTF_INFO()` as a table function.
pub fn ptf_groups_function(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    _args: &Args,
    out: &mut Out,
) -> Result<()> {
    ptf_groups(txn, ctx, out)
}

/// `GROUP_PTF_DETAILS`' `NOT NULL` columns with no value here: QS/400
/// groups name no APARs or cumulative packages.
pub const DETAIL_NO_VALUE: &[&str] = &[
    "APAR_NAME",
    "INCLUDED_IN_GROUP",
    "PTF_CUM_PACKAGE",
    "PTF_INCLUDED_IN_GROUP_DATE",
];

/// `SYSTOOLS.GROUP_PTF_DETAILS`: the PTFs of the system's groups, from
/// the local group definitions (IBM's reads its service web site).
pub fn ptf_details(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    details_of(txn, ctx, None, out)
}

/// `SYSTOOLS.GROUP_PTF_DETAILS(P_GROUP_PTF_ID)`.
pub fn ptf_details_function(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    args: &Args,
    out: &mut Out,
) -> Result<()> {
    let g = args.word("P_GROUP_PTF_ID");
    details_of(txn, ctx, g.as_deref(), out)
}

fn details_of(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    group: Option<&str>,
    out: &mut Out,
) -> Result<()> {
    for g in ask(txn, ctx, "the PTF groups", |h| h.ptf_groups())?.map_err(failed)? {
        if group.is_some_and(|n| n != g.name) {
            continue;
        }
        for (id, product, status) in &g.ptfs {
            out.row();
            out.text("PTF_GROUP_DESCRIPTION", g.text.as_str());
            out.text("PTF_GROUP_NAME", g.name.as_str());
            out.text("PTF_STATUS", status.as_str());
            out.text("PTF_PRODUCT_ID", product.as_str());
            out.text("PTF_IDENTIFIER", id.as_str());
            out.text("PTF_RELEASE_LEVEL", g.release.as_str());
        }
    }
    Ok(())
}

/// `TCPIP_INFO`: the host name and the asking job's connection. The
/// operating system's version is QS/400's own and not shown.
pub fn tcpip(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let c = ask(txn, ctx, "the connection", |h| h.connection())?;
    out.row();
    out.stamp("COLLECTED_TIME", ctx.clock());
    out.text("LOCAL_HOST_NAME", c.host_name.as_str());
    for (side, addr) in [("CLIENT", c.client), ("SERVER", c.server)] {
        if let Some(a) = addr {
            out.text(
                &format!("{side}_IP_ADDRESS_TYPE"),
                if a.is_ipv4() { "IPV4" } else { "IPV6" },
            );
            out.text(&format!("{side}_IP_ADDRESS"), a.ip().to_string());
            out.int(&format!("{side}_PORT_NUMBER"), a.port());
        }
    }
    Ok(())
}

/// `TCPIP_INFO()` as a table function.
pub fn tcpip_function(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    _args: &Args,
    out: &mut Out,
) -> Result<()> {
    tcpip(txn, ctx, out)
}

/// `DNS_LOOKUP(SEARCH_NAME, DOMAIN_SERVER)`: the name's addresses. The
/// resolver answers no authority or authentication flags: null.
pub fn dns(txn: &mut Txn<'_>, ctx: &SqlContext, args: &Args, out: &mut Out) -> Result<()> {
    let name = args.text("SEARCH_NAME").unwrap_or_default();
    let server = args.text("DOMAIN_SERVER").filter(|s| !s.trim().is_empty());
    if server.is_some() {
        // BLOCKED(rel): the system's resolver answers; asking one server
        // needs a DNS client of our own.
        return Err(SqlError::Coded {
            code: -84,
            state: "42612",
            what: "DNS_LOOKUP DOMAIN_SERVER is not supported".into(),
        });
    }
    let addrs = ask(txn, ctx, "the resolver", |h| {
        h.dns_lookup(name.trim(), server.as_deref().map(str::trim))
    })?
    .map_err(failed)?;
    for a in addrs {
        out.row();
        out.text(
            "ADDRESS_SPACE_TYPE",
            if a.is_ipv4() { "IPV4" } else { "IPV6" },
        );
        out.text("IP_ADDRESS", a.to_string());
    }
    Ok(())
}
