// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system's resources as `WRKSYSSTS`, `WRKDSKSTS` and `WRKHDWRSC`
//! show them: `MEMORY_POOL_INFO` and `MEMORY_POOL`,
//! `SYSTEM_ACTIVITY_INFO`, `ASP_INFO`, `SYSDISKSTAT` and
//! `HARDWARE_RESOURCE_INFO`. QS/400 has one ASP, no expert cache, no
//! pool tuning and no compressed or multipath units; what it does not
//! measure is null.

use store::txn::Txn;

use super::super::super::SqlContext;
use super::{Args, Out, ask};
use crate::error::{Result, SqlError};

fn failed(e: String) -> SqlError {
    SqlError::Coded {
        code: -443,
        state: "38501",
        what: e,
    }
}

#[allow(clippy::cast_precision_loss)] // sizes far below 2^52
fn mb(kb: u64) -> f64 {
    kb as f64 / 1024.0
}

/// `MEMORY_POOL_INFO`.
pub fn pools(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    for p in ask(txn, ctx, "the storage pools", |h| h.memory_pools())? {
        out.row();
        out.int("SYSTEM_POOL_ID", p.number);
        let name = p.name.rsplit('/').next().unwrap_or(&p.name).to_string();
        out.text("POOL_NAME", name);
        out.float("CURRENT_SIZE", mb(p.size_kb), 2);
        out.float("RESERVED_SIZE", mb(p.reserved_kb), 2);
        out.float("DEFINED_SIZE", mb(p.size_kb), 2);
        out.int("MAXIMUM_ACTIVE_THREADS", p.max_active);
        out.int("CURRENT_THREADS", p.active);
        out.int("CURRENT_INELIGIBLE_THREADS", p.ineligible);
        if out.has("STATUS") {
            out.text("STATUS", "ACTIVE");
        }
        if let Some((lib, sbs)) = &p.subsystem {
            out.text("SUBSYSTEM_LIBRARY_NAME", lib.as_str());
            out.text("SUBSYSTEM_NAME", sbs.as_str());
        }
        out.text("PAGING_OPTION", "*FIXED");
        out.int("ELAPSED_TIME", p.elapsed_ms / 1000);
        out.float("ELAPSED_DATABASE_FAULTS", p.db_faults, 1);
        out.float("ELAPSED_NON_DATABASE_FAULTS", p.ndb_faults, 1);
        out.float("ELAPSED_TOTAL_FAULTS", p.db_faults + p.ndb_faults, 1);
        out.float("ELAPSED_DATABASE_PAGES", p.db_pages, 1);
        out.float("ELAPSED_NON_DATABASE_PAGES", p.ndb_pages, 1);
        out.float("ELAPSED_ACTIVE_TO_WAIT", p.transitions[0], 1);
        out.float("ELAPSED_WAIT_TO_INELIGIBLE", p.transitions[1], 1);
        out.float("ELAPSED_ACTIVE_TO_INELIGIBLE", p.transitions[2], 1);
    }
    Ok(())
}

/// `MEMORY_POOL(RESET_STATISTICS)`: the host keeps one interval;
/// `RESET_STATISTICS => 'YES'` is refused.
pub fn pool_function(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    args: &Args,
    out: &mut Out,
) -> Result<()> {
    reset_refused(args)?;
    pools(txn, ctx, out)
}

fn reset_refused(args: &Args) -> Result<()> {
    if args.word("RESET_STATISTICS").is_some_and(|w| w == "YES") {
        return Err(SqlError::Coded {
            code: -443,
            state: "38501",
            what: "RESET_STATISTICS => 'YES' is not supported".into(),
        });
    }
    Ok(())
}

/// `SYSTEM_ACTIVITY_INFO`: the CPU used over the host's interval. The
/// processors are not measured one by one: the lowest and highest are
/// null, as is the rate against the nominal speed.
pub fn activity(txn: &mut Txn<'_>, ctx: &SqlContext, _args: &Args, out: &mut Out) -> Result<()> {
    let s = ask(txn, ctx, "the system status", |h| h.system_status())?;
    out.row();
    out.float("AVERAGE_CPU_UTILIZATION", s.cpu_pct, 2);
    Ok(())
}

/// `ASP_INFO`'s `NOT NULL` columns with no value here.
pub const ASP_NO_VALUE: &[&str] = &[
    "STORAGE_THRESHOLD_PERCENTAGE",
    "COMPRESSION_RECOVERY_POLICY",
];

/// `ASP_INFO`: ASP 1, from its units.
pub fn asp(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let units = ask(txn, ctx, "the disk units", |h| h.disk_units())?;
    let cap: u64 = units.iter().map(|u| u.capacity).sum();
    let free: u64 = units.iter().map(|u| u.free).sum();
    let mirrored = units.iter().any(|u| u.protection.starts_with("Mirrored"));
    out.row();
    out.int("ASP_NUMBER", 1);
    out.text("ASP_STATE", "AVAILABLE");
    out.int("NUMBER_OF_DISK_UNITS", units.len());
    out.text("DISK_UNITS_PRESENT", "ALL");
    out.yes("ENCRYPTED_ASP", false);
    out.int("TOTAL_CAPACITY", cap >> 20);
    out.int("TOTAL_CAPACITY_AVAILABLE", free >> 20);
    let (p, u) = if mirrored {
        ("PROTECTED", "UNPROTECTED")
    } else {
        ("UNPROTECTED", "PROTECTED")
    };
    out.int(&format!("{p}_CAPACITY"), cap >> 20);
    out.int(&format!("{p}_CAPACITY_AVAILABLE"), free >> 20);
    out.int(&format!("{u}_CAPACITY"), 0);
    out.int(&format!("{u}_CAPACITY_AVAILABLE"), 0);
    out.int("SYSTEM_STORAGE", (cap - free.min(cap)) >> 20);
    out.int("OVERFLOW_STORAGE", 0);
    out.text("COMPRESSED_DISK_UNITS", "NONE");
    out.yes("CHANGES_WRITTEN_TO_DISK", true);
    out.yes("MULTIPLE_CONNECTION_DISK_UNITS", false);
    Ok(())
}

/// `ASP_INFO()` as a table function.
pub fn asp_function(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    _args: &Args,
    out: &mut Out,
) -> Result<()> {
    asp(txn, ctx, out)
}

/// `SYSDISKSTAT`'s `NOT NULL` columns with no value here.
pub const DISK_NO_VALUE: &[&str] = &[
    "RESOURCE_NAME",
    "UNMAP_CAPABLE",
    "IOP_NAME",
    "STORAGE_FOR_SYSTEM",
    "TOTAL_SAMPLE_COUNT",
    "TOTAL_NOT_BUSY_COUNT",
    "TOTAL_PERMANENT_BLOCKS_WRITTEN",
    "TOTAL_PERMANENT_WRITE_REQUESTS",
];

/// `SYSDISKSTAT`.
#[allow(clippy::cast_precision_loss)] // counters far below 2^52
pub fn disks(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    for u in ask(txn, ctx, "the disk units", |h| h.disk_units())? {
        out.row();
        out.int("ASP_NUMBER", 1);
        out.text("DISK_TYPE", u.type_code.as_str());
        out.text("DISK_MODEL", u.model.as_str());
        out.int("UNIT_NUMBER", u.unit);
        out.text("SERIAL_NUMBER", u.serial.as_str());
        out.text("RESOURCE_STATUS", u.status.to_ascii_uppercase());
        out.text(
            "HARDWARE_STATUS",
            if u.status == "Active" {
                "OPERATIONAL"
            } else {
                "DEGRADED"
            },
        );
        out.yes("IS_ZERO", false);
        out.int("BLOCK_SIZE", 4096);
        out.yes("MULTIPLE_PATH_UNIT", false);
        out.int(
            "UNIT_TYPE",
            i32::from(u.type_code == "SSD" || u.type_code == "NVME"),
        );
        out.int("UNIT_NVME", i32::from(u.type_code == "NVME"));
        out.int("UNIT_STORAGE_CAPACITY", u.capacity);
        out.int("UNIT_SPACE_AVAILABLE", u.free);
        out.int("UNIT_SPACE_AVAILABLE_GB", u.free / 1_000_000_000);
        if u.capacity > 0 {
            out.float(
                "PERCENT_USED",
                (u.capacity - u.free.min(u.capacity)) as f64 * 100.0 / u.capacity as f64,
                3,
            );
        }
        out.int("UNIT_MEDIA_CAPACITY", u.capacity);
        out.int("UNIT_MEDIA_CAPACITY_GB", u.capacity / 1_000_000_000);
        out.yes("STORAGE_ALLOCATION_ALLOWED", true);
        out.text_or_null(
            "PROTECTION_TYPE",
            if u.protection.starts_with("Mirrored") {
                "MIRROR"
            } else {
                ""
            },
        );
        out.text("PROTECTION_STATUS", u.status.to_ascii_uppercase());
        out.yes("AVAILABILITY_PARITY_SET_UNIT", false);
        out.yes("HYPERSWAP", false);
        let [r, w, rs, ws, _] = u.totals;
        out.int("TOTAL_READ_REQUESTS", r);
        out.int("TOTAL_WRITE_REQUESTS", w);
        out.int("TOTAL_BLOCKS_READ", rs);
        out.int("TOTAL_BLOCKS_WRITTEN", ws);
        let secs = u.elapsed_ms as f64 / 1000.0;
        out.int("ELAPSED_TIME", u.elapsed_ms / 1000);
        let [er, ew, ers, ews, ebusy] = u.elapsed;
        if secs > 0.0 {
            let per_s = |n: u64| n as f64 / secs;
            let kb = |sectors: u64, n: u64| {
                if n == 0 {
                    0.0
                } else {
                    sectors as f64 / 2.0 / n as f64
                }
            };
            out.float("ELAPSED_IO_REQUESTS", per_s(er + ew), 1);
            out.float("ELAPSED_REQUEST_SIZE", kb(ers + ews, er + ew), 1);
            out.float("ELAPSED_READ_REQUESTS", per_s(er), 1);
            out.float("ELAPSED_WRITE_REQUESTS", per_s(ew), 1);
            out.float("ELAPSED_DATA_READ", kb(ers, er), 1);
            out.float("ELAPSED_DATA_WRITTEN", kb(ews, ew), 1);
            out.float(
                "ELAPSED_PERCENT_BUSY",
                (ebusy as f64 * 100.0 / u.elapsed_ms as f64).min(100.0),
                1,
            );
        }
    }
    Ok(())
}

/// `SYSDISKSTAT(RESET_STATISTICS)`.
pub fn disk_function(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    args: &Args,
    out: &mut Out,
) -> Result<()> {
    reset_refused(args)?;
    disks(txn, ctx, out)
}

/// `HARDWARE_RESOURCE_INFO`'s `NOT NULL` columns with no value here:
/// the machine's internal resource kinds.
pub const HW_NO_VALUE: &[&str] = &[
    "RESOURCE_KIND",
    "RESOURCE_KIND1",
    "RESOURCE_KIND2",
    "RESOURCE_KIND3",
];

/// `HARDWARE_RESOURCE_INFO`.
pub fn hardware(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    hardware_of(txn, ctx, None, out)
}

/// `HARDWARE_RESOURCE_INFO(RESOURCE_CATEGORY, ...)`: `*ALL` or one
/// category.
pub fn hardware_function(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    args: &Args,
    out: &mut Out,
) -> Result<()> {
    // `WRKHDWRSC`'s type names the categories too.
    let cat = args
        .word("RESOURCE_CATEGORY")
        .filter(|c| c != "*ALL" && c != "ALL")
        .map(|c| {
            match c.as_str() {
                "*CMN" => "COMM",
                "*LWS" => "LWS",
                "*PRC" => "PROCESSOR",
                "*STG" => "STORAGE",
                other => other,
            }
            .to_string()
        });
    hardware_of(txn, ctx, cat.as_deref(), out)
}

fn hardware_of(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    category: Option<&str>,
    out: &mut Out,
) -> Result<()> {
    let list = ask(txn, ctx, "the hardware resources", |h| h.hardware())?.map_err(failed)?;
    for (i, r) in list.iter().enumerate() {
        if category.is_some_and(|c| c != r.category) {
            continue;
        }
        out.row();
        out.int("ID", i + 1);
        if let Some(p) = list.iter().position(|x| x.name == r.parent) {
            out.int("PARENT_ID", p + 1);
        }
        out.text("RESOURCE_CATEGORY", r.category.as_str());
        out.text("RESOURCE_NAME", r.name.as_str());
        out.text(
            "STATUS",
            if r.operational {
                "OPERATIONAL"
            } else {
                "NOT DETECTED"
            },
        );
        out.text_or_null("DEVICE_TYPE", &r.type_model.0);
        out.text_or_null("DEVICE_MODEL", &r.type_model.1);
        out.text_or_null("TEXT_DESCRIPTION", &r.text);
        out.text_or_null("LAN_ADAPTER_ADDRESS", &r.mac);
        out.text("SERIAL_NUMBER", r.serial.as_str());
        out.text(
            "SERIAL_NUMBER_SHORT",
            r.serial
                .chars()
                .rev()
                .take(10)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<String>(),
        );
        out.text_or_null("LOCATION_CODE", &r.location);
    }
    Ok(())
}
