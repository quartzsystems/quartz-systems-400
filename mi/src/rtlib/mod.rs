// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The runtime library: one `extern "sysv64"` function per entry of
//! the call table in `codegen::rt`, and the table of their addresses.
//! Every function receives the environment first, reads its storage
//! operands through [`mem`], and reports a condition by raising it on
//! the job so the caller's poll sees it.

mod convert;
mod core;
mod date;
mod decimal;
mod heap;
pub(crate) mod mem;
mod msg;
mod string;

pub use convert::convert;
pub use core::{
    call_pgm, call_pgm_ptr, call_pgm_var, copy, deref, dsply, enter, leave, monitor_match, mov_str,
    proc_addr, proc_target, ptr_add, ptr_addr, ptr_diff, raise, unresolved,
};
pub use date::{dt_add, dt_diff, dt_extract, dt_now, dt_test};
pub use decimal::{dec_bin, dec_cmp, dec_to_float, dec_to_int, dec_un};
pub use heap::{alloc, dealloc, realloc};
pub use msg::{rcvmsg, rmvmsg, sndpgmmsg};
pub use string::{
    str_cat, str_check, str_cmp, str_len, str_replace, str_scan, str_setlen, str_subst, str_trim,
    str_xlate,
};

use codegen::{RT_FNS, RtFn};

/// The call table: the address of every runtime function, in index
/// order, as the loader writes them into generated code.
#[must_use]
pub fn table() -> Vec<usize> {
    RT_FNS
        .iter()
        .map(|f| match f {
            RtFn::Raise => raise as *const () as usize,
            RtFn::Enter => enter as *const () as usize,
            RtFn::Leave => leave as *const () as usize,
            RtFn::Deref => deref as *const () as usize,
            RtFn::Copy => copy as *const () as usize,
            RtFn::MovStr => mov_str as *const () as usize,
            RtFn::DecBin => dec_bin as *const () as usize,
            RtFn::DecUn => dec_un as *const () as usize,
            RtFn::DecCmp => dec_cmp as *const () as usize,
            RtFn::DecToInt => dec_to_int as *const () as usize,
            RtFn::DecToFloat => dec_to_float as *const () as usize,
            RtFn::Convert => convert as *const () as usize,
            RtFn::StrCat => str_cat as *const () as usize,
            RtFn::StrSubst => str_subst as *const () as usize,
            RtFn::StrScan => str_scan as *const () as usize,
            RtFn::StrCheck => str_check as *const () as usize,
            RtFn::StrXlate => str_xlate as *const () as usize,
            RtFn::StrTrim => str_trim as *const () as usize,
            RtFn::StrCmp => str_cmp as *const () as usize,
            RtFn::StrLen => str_len as *const () as usize,
            RtFn::StrSetLen => str_setlen as *const () as usize,
            RtFn::StrReplace => str_replace as *const () as usize,
            RtFn::DtAdd => dt_add as *const () as usize,
            RtFn::DtDiff => dt_diff as *const () as usize,
            RtFn::DtExtract => dt_extract as *const () as usize,
            RtFn::DtTest => dt_test as *const () as usize,
            RtFn::DtNow => dt_now as *const () as usize,
            RtFn::PtrAddr => ptr_addr as *const () as usize,
            RtFn::PtrAdd => ptr_add as *const () as usize,
            RtFn::PtrDiff => ptr_diff as *const () as usize,
            RtFn::ProcAddr => proc_addr as *const () as usize,
            RtFn::Dsply => dsply as *const () as usize,
            RtFn::MonitorMatch => monitor_match as *const () as usize,
            RtFn::CallPgm => call_pgm as *const () as usize,
            RtFn::CallPgmVar => call_pgm_var as *const () as usize,
            RtFn::CallPgmPtr => call_pgm_ptr as *const () as usize,
            RtFn::ProcTarget => proc_target as *const () as usize,
            RtFn::Alloc => alloc as *const () as usize,
            RtFn::Realloc => realloc as *const () as usize,
            RtFn::Dealloc => dealloc as *const () as usize,
            RtFn::SndPgmMsg => sndpgmmsg as *const () as usize,
            RtFn::RcvMsg => rcvmsg as *const () as usize,
            RtFn::RmvMsg => rmvmsg as *const () as usize,
        })
        .collect()
}
