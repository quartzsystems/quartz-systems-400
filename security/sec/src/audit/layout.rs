// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The entry-specific data of each audit entry type QS/400 writes:
//! IBM's model-file field names (`QASYxxJ5`, without the journal
//! heading), fixed-width CP037 text. Generated with the tables in
//! `docs/SECURITY.md` §7, which `tests/spec.rs` compares.

use crate::audit::kind::Kind;
use crate::rec::{Layout, field};

/// `AF`: authority failure.
pub static AF: Layout = Layout {
    name: "AuditAF",
    size: Some(135),
    fields: &[
        field("AFVIOL", 0, 1),
        field("AFONAM", 1, 10),
        field("AFOLIB", 11, 10),
        field("AFOTYP", 21, 8),
        field("AFOPER", 29, 24),
        field("AFSTEP", 53, 16),
        field("AFHAVE", 69, 8),
        field("AFPGMN", 77, 10),
        field("AFPGML", 87, 10),
        field("AFOFID", 97, 32),
        field("AFAPIN", 129, 1),
        field("AFPNLN", 130, 5),
    ],
};

/// `AD`: auditing change.
pub static AD: Layout = Layout {
    name: "AuditAD",
    size: Some(267),
    fields: &[
        field("ADETYP", 0, 1),
        field("ADONAM", 1, 10),
        field("ADOLIB", 11, 10),
        field("ADOTYP", 21, 8),
        field("ADUSRP", 29, 10),
        field("ADOVAL", 39, 10),
        field("ADNVAL", 49, 10),
        field("ADCMDN", 59, 10),
        field("ADALVL", 69, 160),
        field("ADOFID", 229, 32),
        field("ADAPIN", 261, 1),
        field("ADPNLN", 262, 5),
    ],
};

/// `AU`: attribute change.
pub static AU: Layout = Layout {
    name: "AuditAU",
    size: Some(139),
    fields: &[
        field("AUETYP", 0, 1),
        field("AUONAM", 1, 10),
        field("AUOLIB", 11, 10),
        field("AUOTYP", 21, 8),
        field("AUATTR", 29, 10),
        field("AUOVAL", 39, 50),
        field("AUNVAL", 89, 50),
    ],
};

/// `CA`: authority change.
pub static CA: Layout = Layout {
    name: "AuditCA",
    size: Some(113),
    fields: &[
        field("CAETYP", 0, 1),
        field("CAONAM", 1, 10),
        field("CAOLIB", 11, 10),
        field("CAOTYP", 21, 8),
        field("CAUSRP", 29, 10),
        field("CAAUTL", 39, 10),
        field("CAOAUT", 49, 8),
        field("CANAUT", 57, 8),
        field("CACMDN", 65, 10),
        field("CAOFID", 75, 32),
        field("CAAPIN", 107, 1),
        field("CAPNLN", 108, 5),
    ],
};

/// `CO`: object created.
pub static CO: Layout = Layout {
    name: "AuditCO",
    size: Some(95),
    fields: &[
        field("COETYP", 0, 1),
        field("COONAM", 1, 10),
        field("COOLIB", 11, 10),
        field("COOTYP", 21, 8),
        field("COOWNR", 29, 10),
        field("COPGPN", 39, 10),
        field("COPUBA", 49, 8),
        field("COOFID", 57, 32),
        field("COAPIN", 89, 1),
        field("COPNLN", 90, 5),
    ],
};

/// `CP`: user profile changed.
pub static CP: Layout = Layout {
    name: "AuditCP",
    size: Some(153),
    fields: &[
        field("CPCMDN", 0, 10),
        field("CPUSRP", 10, 10),
        field("CPPWDC", 20, 1),
        field("CPPWDN", 21, 1),
        field("CPPWDX", 22, 1),
        field("CPSTAT", 23, 10),
        field("CPCLAS", 33, 10),
        field("CPSPCA", 43, 80),
        field("CPGRPP", 123, 10),
        field("CPOWNR", 133, 10),
        field("CPGAUT", 143, 10),
    ],
};

/// `DO`: object deleted.
pub static DO: Layout = Layout {
    name: "AuditDO",
    size: Some(77),
    fields: &[
        field("DOETYP", 0, 1),
        field("DOONAM", 1, 10),
        field("DOOLIB", 11, 10),
        field("DOOTYP", 21, 8),
        field("DOOWNR", 29, 10),
        field("DOOFID", 39, 32),
        field("DOAPIN", 71, 1),
        field("DOPNLN", 72, 5),
    ],
};

/// `DS`: service tools password reset.
pub static DS: Layout = Layout {
    name: "AuditDS",
    size: Some(11),
    fields: &[field("DSUSRP", 0, 10), field("DSRTYP", 10, 1)],
};

/// `GR`: generic record.
pub static GR: Layout = Layout {
    name: "AuditGR",
    size: Some(42),
    fields: &[
        field("GRETYP", 0, 1),
        field("GREXPT", 1, 20),
        field("GREXPN", 21, 10),
        field("GREXPL", 31, 10),
        field("GRRTCD", 41, 1),
    ],
};

/// `JD`: job description user changed.
pub static JD: Layout = Layout {
    name: "AuditJD",
    size: Some(40),
    fields: &[
        field("JDONAM", 0, 10),
        field("JDOLIB", 10, 10),
        field("JDOUSR", 20, 10),
        field("JDNUSR", 30, 10),
    ],
};

/// `JS`: job change.
pub static JS: Layout = Layout {
    name: "AuditJS",
    size: Some(88),
    fields: &[
        field("JSETYP", 0, 1),
        field("JSJTYP", 1, 1),
        field("JSJNAM", 2, 10),
        field("JSJUSR", 12, 10),
        field("JSJNBR", 22, 6),
        field("JSORUS", 28, 10),
        field("JSJOBD", 38, 20),
        field("JSJOBQ", 58, 20),
        field("JSCMDN", 78, 10),
    ],
};

/// `OM`: object moved or renamed.
pub static OM: Layout = Layout {
    name: "AuditOM",
    size: Some(92),
    fields: &[
        field("OMETYP", 0, 1),
        field("OMONAM", 1, 10),
        field("OMOLIB", 11, 10),
        field("OMOTYP", 21, 8),
        field("OMNNAM", 29, 10),
        field("OMNLIB", 39, 10),
        field("OMOFID", 49, 32),
        field("OMAPIN", 81, 1),
        field("OMPNLN", 82, 5),
        field("OM2PNL", 87, 5),
    ],
};

/// `OW`: owner changed.
pub static OW: Layout = Layout {
    name: "AuditOW",
    size: Some(86),
    fields: &[
        field("OWONAM", 0, 10),
        field("OWOLIB", 10, 10),
        field("OWOTYP", 20, 8),
        field("OWOOWN", 28, 10),
        field("OWNOWN", 38, 10),
        field("OWOFID", 48, 32),
        field("OWAPIN", 80, 1),
        field("OWPNLN", 81, 5),
    ],
};

/// `PA`: adopting program changed.
pub static PA: Layout = Layout {
    name: "AuditPA",
    size: Some(49),
    fields: &[
        field("PAETYP", 0, 1),
        field("PAONAM", 1, 10),
        field("PAOLIB", 11, 10),
        field("PAOTYP", 21, 8),
        field("PAOWNR", 29, 10),
        field("PAUPRF", 39, 10),
    ],
};

/// `PG`: primary group changed.
pub static PG: Layout = Layout {
    name: "AuditPG",
    size: Some(94),
    fields: &[
        field("PGONAM", 0, 10),
        field("PGOLIB", 10, 10),
        field("PGOTYP", 20, 8),
        field("PGOPGP", 28, 10),
        field("PGNPGP", 38, 10),
        field("PGPAUT", 48, 8),
        field("PGOFID", 56, 32),
        field("PGAPIN", 88, 1),
        field("PGPNLN", 89, 5),
    ],
};

/// `PS`: profile swap.
pub static PS: Layout = Layout {
    name: "AuditPS",
    size: Some(31),
    fields: &[
        field("PSETYP", 0, 1),
        field("PSUSRP", 1, 10),
        field("PSNUSR", 11, 10),
        field("PSDEVN", 21, 10),
    ],
};

/// `PW`: password or user not valid.
pub static PW: Layout = Layout {
    name: "AuditPW",
    size: Some(21),
    fields: &[
        field("PWETYP", 0, 1),
        field("PWUSRP", 1, 10),
        field("PWDEVN", 11, 10),
    ],
};

/// `SE`: subsystem routing changed.
pub static SE: Layout = Layout {
    name: "AuditSE",
    size: Some(85),
    fields: &[
        field("SEETYP", 0, 1),
        field("SESBSN", 1, 10),
        field("SESBSL", 11, 10),
        field("SESEQN", 21, 4),
        field("SEPGMN", 25, 10),
        field("SEPGML", 35, 10),
        field("SECLSN", 45, 10),
        field("SECLSL", 55, 10),
        field("SECMPD", 65, 20),
    ],
};

/// `SF`: spooled file action.
pub static SF: Layout = Layout {
    name: "AuditSF",
    size: Some(73),
    fields: &[
        field("SFETYP", 0, 1),
        field("SFFILE", 1, 10),
        field("SFSPNB", 11, 6),
        field("SFJNAM", 17, 10),
        field("SFJUSR", 27, 10),
        field("SFJNBR", 37, 6),
        field("SFOUTQ", 43, 10),
        field("SFOQLB", 53, 10),
        field("SFOWNR", 63, 10),
    ],
};

/// `ST`: service tools used.
pub static ST: Layout = Layout {
    name: "AuditST",
    size: Some(13),
    fields: &[
        field("STETYP", 0, 1),
        field("STSRVT", 1, 2),
        field("STUSRP", 3, 10),
    ],
};

/// `SV`: system value changed.
pub static SV: Layout = Layout {
    name: "AuditSV",
    size: Some(511),
    fields: &[
        field("SVETYP", 0, 1),
        field("SVSYSV", 1, 10),
        field("SVNVAL", 11, 250),
        field("SVOVAL", 261, 250),
    ],
};

/// `ZC`: object changed.
pub static ZC: Layout = Layout {
    name: "AuditZC",
    size: Some(91),
    fields: &[
        field("ZCONAM", 0, 10),
        field("ZCOLIB", 10, 10),
        field("ZCOTYP", 20, 8),
        field("ZCACCY", 28, 1),
        field("ZCOPER", 29, 24),
        field("ZCOFID", 53, 32),
        field("ZCAPIN", 85, 1),
        field("ZCPNLN", 86, 5),
    ],
};

/// `ZR`: object read.
pub static ZR: Layout = Layout {
    name: "AuditZR",
    size: Some(91),
    fields: &[
        field("ZRONAM", 0, 10),
        field("ZROLIB", 10, 10),
        field("ZROTYP", 20, 8),
        field("ZRACCY", 28, 1),
        field("ZROPER", 29, 24),
        field("ZROFID", 53, 32),
        field("ZRAPIN", 85, 1),
        field("ZRPNLN", 86, 5),
    ],
};

/// The layout of an entry type QS/400 writes.
#[must_use]
pub fn of(kind: Kind) -> Option<&'static Layout> {
    Some(match kind {
        Kind::Af => &AF,
        Kind::Ad => &AD,
        Kind::Au => &AU,
        Kind::Ca => &CA,
        Kind::Co => &CO,
        Kind::Cp => &CP,
        Kind::Do => &DO,
        Kind::Ds => &DS,
        Kind::Gr => &GR,
        Kind::Jd => &JD,
        Kind::Js => &JS,
        Kind::Om => &OM,
        Kind::Ow => &OW,
        Kind::Pa => &PA,
        Kind::Pg => &PG,
        Kind::Ps => &PS,
        Kind::Pw => &PW,
        Kind::Se => &SE,
        Kind::Sf => &SF,
        Kind::St => &ST,
        Kind::Sv => &SV,
        Kind::Zc => &ZC,
        Kind::Zr => &ZR,
        _ => return None,
    })
}

/// Every layout.
#[must_use]
pub fn all() -> Vec<&'static Layout> {
    vec![
        &AF, &AD, &AU, &CA, &CO, &CP, &DO, &DS, &GR, &JD, &JS, &OM, &OW, &PA, &PG, &PS, &PW, &SE,
        &SF, &ST, &SV, &ZC, &ZR,
    ]
}
