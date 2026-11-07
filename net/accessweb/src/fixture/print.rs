// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The prototype's printer output: the eight spooled files of
//! `web/printer-output.html`.

use crate::env::print::Print;

/// A sample spooled file: file, user, user data, status, pages,
/// copies, form type, output queue, created, job.
pub type Splf = [&'static str; 10];

/// The user's spooled files, newest first.
pub const SPOOLED: &[Splf] = &[
    ["INVRPT", "CWELLMAN", "INVBATCH", "RDY", "12", "1", "*STD", "QGPL/QPRINT", "10/29/26 06:14", "123402/CWELLMAN/INVBATCH"],
    ["QPJOBLOG", "CWELLMAN", "INVLOAD", "RDY", "3", "1", "*STD", "QUSRSYS/QEZJOBLOG", "10/29/26 06:15", "123402/CWELLMAN/INVBATCH"],
    ["QPRINT", "CWELLMAN", "", "HLD", "1", "1", "*STD", "QGPL/QPRINT", "10/28/26 18:00", "123311/CWELLMAN/QPADEV0003"],
    ["QSYSPRT", "CWELLMAN", "DSPJOB", "RDY", "7", "1", "*STD", "QGPL/QPRINT", "10/28/26 17:02", "123311/CWELLMAN/QPADEV0003"],
    ["INVLABEL", "CWELLMAN", "LABELS", "RDY", "118", "2", "*LABEL", "QGPL/PRT02", "10/27/26 09:40", "123290/CWELLMAN/INVBATCH"],
    ["QPDSPJOB", "CWELLMAN", "", "SAV", "2", "1", "*STD", "QGPL/QPRINT", "10/24/26 11:11", "123101/CWELLMAN/QPADEV0003"],
    ["QPJOBLOG", "CWELLMAN", "QPADEV0003", "MSGW", "1", "1", "*STD", "QUSRSYS/QEZJOBLOG", "10/22/26 09:50", "123050/CWELLMAN/QPADEV0003"],
    ["ASSETEXP", "CWELLMAN", "EXPORT", "WTR", "44", "1", "*STD", "HLINV/INVRPT", "10/20/26 15:00", "122990/CWELLMAN/INVBATCH"],

];

/// The sample printer output.
#[derive(Debug, Default)]
pub struct FixturePrint;

impl Print for FixturePrint {}
