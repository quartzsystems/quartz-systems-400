// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The query options file (`QAQQINI`) as `QSYS2.OVERRIDE_QAQQINI` sets
//! it for a job: option 1 starts the job's own copy, 2 changes an
//! option in it, 3 drops it. QS/400's query engine has no temporary
//! indexes, parallelism or plan cache to tune, so an option that only
//! tunes those is checked and kept without effect; one that would
//! change a result or a message is refused (SQL0084) at any value but
//! its default until the engine honours it. The query governor's
//! limits (`QUERY_TIME_LIMIT`, `QUERY_STORAGE_LIMIT`, `STORAGE_LIMIT`)
//! are honoured (`plan::govern`).

use std::collections::BTreeMap;

use store::txn::Txn;

use super::super::super::SqlContext;
use super::Args;
use crate::error::{Result, SqlError};
use crate::value::Value;

/// The job's options, when it overrides them.
#[derive(Clone, Debug, Default)]
pub struct Overrides {
    /// The options set, by name; `None` when the job uses the system's.
    pub options: Option<BTreeMap<String, String>>,
}

impl Overrides {
    /// An option's value: the job's, else `*DEFAULT`.
    #[must_use]
    pub fn get(&self, name: &str) -> &str {
        self.options.as_ref().and_then(|o| o.get(name)).map_or("*DEFAULT", String::as_str)
    }
}

/// What an option does here.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Effect {
    /// Tunes what QS/400 does not have: kept, no effect.
    Tuning,
    /// Would change results or messages: only `*DEFAULT`.
    NotYet,
    /// Honoured by the engine (`plan::govern`).
    Honoured,
}

/// The options and their values: `*` before a word, `#` for a number.
const OPTIONS: &[(&str, &[&str], Effect)] = &[
    ("ALLOW_TEMPORARY_INDEXES", &["*YES", "*ONLY_REQUIRED"], Effect::Tuning),
    ("APPLY_REMOTE", &["*YES", "*NO"], Effect::Tuning),
    ("CACHE_RESULTS", &["*SYSTEM", "*JOB", "*NONE"], Effect::Tuning),
    ("COMMITMENT_CONTROL_LOCK_LIMIT", &["#"], Effect::NotYet),
    ("FORCE_JOIN_ORDER", &["*YES", "*NO", "*SQL", "*PRIMARY"], Effect::Tuning),
    ("IGNORE_DERIVED_INDEX", &["*YES", "*NO"], Effect::Tuning),
    ("IGNORE_LIKE_REDUNDANT_SHIFTS", &["*ALWAYS", "*OPTIMIZE"], Effect::NotYet),
    ("LIMIT_PREDICATE_OPTIMIZATION", &["*NO", "#"], Effect::Tuning),
    ("MATERIALIZED_QUERY_TABLE_REFRESH_AGE", &["*ANY", "#"], Effect::Tuning),
    ("MATERIALIZED_QUERY_TABLE_USAGE", &["*ALL", "*NONE", "*USER"], Effect::Tuning),
    ("MEMORY_POOL_PREFERENCE", &["*JOB", "*BASE", "#"], Effect::Tuning),
    ("MESSAGES_DEBUG", &["*YES", "*NO"], Effect::NotYet),
    ("NORMALIZE_DATA", &["*YES", "*NO"], Effect::NotYet),
    ("OPEN_CURSOR_CLOSE_COUNT", &["#"], Effect::Tuning),
    ("OPEN_CURSOR_THRESHOLD", &["#"], Effect::Tuning),
    ("OPTIMIZATION_GOAL", &["*FIRSTIO", "*ALLIO"], Effect::Tuning),
    ("OPTIMIZE_STATISTIC_LIMITATION", &["*NO", "*PERCENTAGE", "*MAX_NUMBER_OF_RECORDS_ALLOWED"], Effect::Tuning),
    ("PARALLEL_DEGREE", &["*NONE", "*IO", "*OPTIMIZE", "*MAX", "*SYSVAL", "*NBRTASKS"], Effect::Tuning),
    ("PARAMETER_MARKER_CONVERSION", &["*YES", "*NO"], Effect::Tuning),
    ("QUERY_TIME_LIMIT", &["*NOMAX", "#"], Effect::Honoured),
    ("QUERY_STORAGE_LIMIT", &["*NOMAX", "#"], Effect::Honoured),
    ("REOPTIMIZE_ACCESS_PLAN", &["*NO", "*YES", "*FORCE", "*ONLY_REQUIRED", "*REBUILD"], Effect::Tuning),
    ("SQL_CONCURRENT_ACCESS_RESOLUTION", &["*USECURCMT", "*WAIT"], Effect::NotYet),
    ("SQL_DECFLOAT_WARNINGS", &["*YES", "*NO"], Effect::NotYet),
    ("SQL_FAST_DELETE_ROW_COUNT", &["*NONE", "#"], Effect::Tuning),
    ("SQL_STMT_COMPRESS_MAX", &["#"], Effect::Tuning),
    ("SQL_STMT_REUSE", &["#"], Effect::Tuning),
    ("SQL_SUPPRESS_WARNINGS", &["*YES", "*NO"], Effect::NotYet),
    ("SQL_TRANSLATE_ASCII_TO_JOB", &["*YES", "*NO"], Effect::NotYet),
    ("STAR_JOIN", &["*NO", "*COST", "*FORCE"], Effect::Tuning),
    ("STORAGE_LIMIT", &["*NOMAX", "#"], Effect::Honoured),
    ("SYSTEM_SQL_STATEMENT_CACHE", &["*YES", "*NO"], Effect::Tuning),
    ("UDF_TIME_OUT", &["*MAX", "#"], Effect::NotYet),
    ("VISUAL_EXPLAIN_DIAGRAM", &["*DETAIL", "*BASIC"], Effect::Tuning),
];

fn invalid(what: String) -> SqlError {
    SqlError::Coded { code: -443, state: "22023", what }
}

/// `OVERRIDE_QAQQINI(OVERRIDE_OPTION, OPTION_NAME, OPTION_VALUE)`.
pub fn override_qaqqini(_txn: &mut Txn<'_>, ctx: &mut SqlContext, args: &Args) -> Result<Vec<Value>> {
    match args.int("OVERRIDE_OPTION") {
        Some(1) => {
            ctx.qaqqini.options.get_or_insert_with(BTreeMap::new);
        }
        Some(2) => {
            let name = args.word("OPTION_NAME").unwrap_or_default();
            let value = args.word("OPTION_VALUE").unwrap_or_default();
            let Some((_, values, effect)) = OPTIONS.iter().find(|(n, _, _)| *n == name) else {
                return Err(invalid(format!("{name} is not a query option")));
            };
            let ok = value == "*DEFAULT" || values.contains(&value.as_str()) || (values.contains(&"#") && value.parse::<u64>().is_ok());
            if !ok {
                return Err(invalid(format!("{value} is not a value of {name}")));
            }
            if *effect == Effect::NotYet && value != "*DEFAULT" {
                // BLOCKED(rel): these options change results or messages
                // and the engine does not honour them yet.
                return Err(SqlError::Coded { code: -84, state: "42612", what: format!("query option {name} other than *DEFAULT is not supported") });
            }
            let Some(opts) = ctx.qaqqini.options.as_mut() else {
                return Err(invalid("OVERRIDE_OPTION 2 before OVERRIDE_OPTION 1".into()));
            };
            opts.insert(name, value);
        }
        Some(3) => ctx.qaqqini.options = None,
        other => return Err(invalid(format!("OVERRIDE_OPTION {}: 1, 2 or 3", other.map_or_else(|| "NULL".into(), |n| n.to_string())))),
    }
    Ok(Vec::new())
}
