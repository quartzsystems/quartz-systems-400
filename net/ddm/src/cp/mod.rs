// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The code points: commands, reply messages, command and reply data
//! objects, parameters, managers, the System/38 record access
//! extensions IBM i speaks between systems, and the values of `SECMEC`
//! and `SECCHKCD`. Names are DDM's.

/// Commands.
pub mod cmd {
    consts! {
        /// Exchange server attributes.
        EXCSAT = 0x1041,
        /// Access security.
        ACCSEC = 0x106D,
        /// Security check.
        SECCHK = 0x106E,
        /// Access the relational database.
        ACCRDB = 0x2001,
        /// Begin binding a package.
        BGNBND = 0x2002,
        /// Bind an SQL statement.
        BNDSQLSTT = 0x2004,
        /// Close a query.
        CLSQRY = 0x2005,
        /// Continue a query.
        CNTQRY = 0x2006,
        /// Drop a package.
        DRPPKG = 0x2007,
        /// Describe an SQL statement.
        DSCSQLSTT = 0x2008,
        /// End binding.
        ENDBND = 0x2009,
        /// Execute an SQL statement immediately.
        EXCSQLIMM = 0x200A,
        /// Execute a prepared SQL statement.
        EXCSQLSTT = 0x200B,
        /// Open a query.
        OPNQRY = 0x200C,
        /// Prepare an SQL statement.
        PRPSQLSTT = 0x200D,
        /// Commit the unit of work.
        RDBCMM = 0x200E,
        /// Roll back the unit of work.
        RDBRLLBCK = 0x200F,
        /// Rebind a package.
        REBIND = 0x2010,
        /// Describe an RDB table.
        DSCRDBTBL = 0x2012,
        /// Execute SQL SET statements.
        EXCSQLSET = 0x2014,
        /// Commit a unit of work (DDM files).
        CMMUOW = 0x105A,
        /// Roll back a unit of work (DDM files).
        RLLBCKUOW = 0x105B,
        /// Start commitment control (DDM files).
        STRCMMCTL = 0x105C,
        /// Declare a file name.
        DCLFIL = 0x102C,
    }
}

/// Reply data objects that are collections.
pub mod rpy {
    consts! {
        /// Server attributes reply data.
        EXCSATRD = 0x1443,
        /// Security access reply data.
        ACCSECRD = 0x14AC,
    }
}

/// Reply messages.
pub mod rm {
    consts! {
        /// Security check (`JTOpen`'s name `SECCHKRD`).
        SECCHKRM = 0x1219,
        /// The RDB was accessed.
        ACCRDBRM = 0x2201,
        /// Query not open.
        QRYNOPRM = 0x2202,
        /// Not authorized to the RDB.
        RDBATHRM = 0x2203,
        /// RDB not accessed.
        RDBNACRM = 0x2204,
        /// Query opened.
        OPNQRYRM = 0x2205,
        /// RDB currently accessed.
        RDBACCRM = 0x2207,
        /// End of query.
        ENDQRYRM = 0x220B,
        /// End of unit of work.
        ENDUOWRM = 0x220C,
        /// Abnormal end of unit of work.
        ABNUOWRM = 0x220D,
        /// Data descriptor mismatch.
        DTAMCHRM = 0x220E,
        /// Query previously opened.
        QRYPOPRM = 0x220F,
        /// RDB not found.
        RDBNFNRM = 0x2211,
        /// Open query failure.
        OPNQFLRM = 0x2212,
        /// SQL error condition.
        SQLERRRM = 0x2213,
        /// RDB update reply.
        RDBUPDRM = 0x2218,
        /// RDB access failed.
        RDBAFLRM = 0x221A,
        /// Manager level conflict.
        MGRLVLRM = 0x1210,
        /// Manager dependency error.
        MGRDEPRM = 0x1218,
        /// Not authorized to the command.
        CMDATHRM = 0x121C,
        /// Permanent agent error.
        AGNPRMRM = 0x1232,
        /// Resource limits reached.
        RSCLMTRM = 0x1233,
        /// Conversational protocol error.
        PRCCNVRM = 0x1245,
        /// Command completed (DDM files).
        CMDCMPRM = 0x124B,
        /// Data stream syntax error.
        SYNTAXRM = 0x124C,
        /// Command not supported.
        CMDNSPRM = 0x1250,
        /// Parameter not supported.
        PRMNSPRM = 0x1251,
        /// Parameter value not supported.
        VALNSPRM = 0x1252,
        /// Object not supported.
        OBJNSPRM = 0x1253,
        /// Command check.
        CMDCHKRM = 0x1254,
        /// Target not supported.
        TRGNSPRM = 0x125F,
        /// IBM i message reply (System/38 extension).
        S38MSGRM = 0xD201,
    }
}

/// Command and reply data objects (FD:OCA data and descriptors).
pub mod obj {
    consts! {
        /// Type definition name.
        TYPDEFNAM = 0x002F,
        /// Type definition overrides.
        TYPDEFOVR = 0x0035,
        /// FD:OCA descriptor.
        FDODSC = 0x0010,
        /// Extra data (LOBs).
        EXTDTA = 0x146C,
        /// FD:OCA data.
        FDODTA = 0x147A,
        /// SQL communications area reply data.
        SQLCARD = 0x2408,
        /// SQL continuation information.
        SQLCINRD = 0x240B,
        /// SQL result set reply data.
        SQLRSLRD = 0x240E,
        /// SQL descriptor area reply data.
        SQLDARD = 0x2411,
        /// SQL program variable data.
        SQLDTA = 0x2412,
        /// SQL data reply data.
        SQLDTARD = 0x2413,
        /// SQL statement.
        SQLSTT = 0x2414,
        /// SQL statement variable descriptions.
        SQLSTTVRB = 0x2419,
        /// Query answer set description.
        QRYDSC = 0x241A,
        /// Query answer set data.
        QRYDTA = 0x241B,
        /// SQL statement attributes.
        SQLATTR = 0x2450,
    }
}

/// Parameters.
pub mod param {
    consts! {
        /// Code point.
        CODPNT = 0x000C,
        /// File name.
        FILNAM = 0x110E,
        /// Product-specific identifier.
        PRDID = 0x112E,
        /// Declared name.
        DCLNAM = 0x1136,
        /// Conversational protocol error code.
        PRCCNVCD = 0x113F,
        /// Release file lock.
        RLSFILLK = 0x1143,
        /// Request file lock.
        RQSFILLK = 0x1145,
        /// Server class name.
        SRVCLSNM = 0x1147,
        /// Severity code.
        SVRCOD = 0x1149,
        /// Syntax error code.
        SYNERRCD = 0x114A,
        /// Server diagnostic information.
        SRVDGN = 0x1153,
        /// Return inactive records.
        RTNINA = 0x1155,
        /// Server product release level.
        SRVRLSLV = 0x115A,
        /// Supervisor name.
        SPVNAM = 0x115D,
        /// External name.
        EXTNAM = 0x115E,
        /// Directory name.
        DRCNAM = 0x1165,
        /// Server name.
        SRVNAM = 0x116D,
        /// Security manager name.
        SECMGRNM = 0x1196,
        /// Single-byte CCSID.
        CCSIDSBC = 0x119C,
        /// Double-byte CCSID.
        CCSIDDBC = 0x119D,
        /// Mixed-byte CCSID.
        CCSIDMBC = 0x119E,
        /// User ID.
        USRID = 0x11A0,
        /// Password.
        PASSWORD = 0x11A1,
        /// Security mechanism.
        SECMEC = 0x11A2,
        /// Security check code.
        SECCHKCD = 0x11A4,
        /// Hold cursors over a unit of work.
        LUWHLDCSR = 0x11B5,
        /// Commitment control type.
        CMMCTLTYP = 0x11BA,
        /// Security token.
        SECTKN = 0x11DC,
        /// New password.
        NEWPASSWORD = 0x11DE,
        /// Manager-level list.
        MGRLVLLS = 0x1404,
        /// Encryption algorithm.
        ENCALG = 0x1909,
        /// Encryption key length.
        ENCKEYLEN = 0x190A,
        /// Monitor events.
        MONITOR = 0x1900,
        /// Query protocol type.
        QRYPRCTYP = 0x2102,
        /// RDB interrupt token.
        RDBINTTKN = 0x2103,
        /// Product-specific data.
        PRDDTA = 0x2104,
        /// RDB commit allowed.
        RDBCMTOK = 0x2105,
        /// RDB collection identifier.
        RDBCOLID = 0x2108,
        /// Package identifier.
        PKGID = 0x2109,
        /// Package name.
        PKGNAM = 0x210A,
        /// Package section number.
        PKGSN = 0x210C,
        /// Package consistency token.
        PKGCNSTKN = 0x210D,
        /// Return SQL statement set.
        RTNSETSTT = 0x210E,
        /// RDB access manager class.
        RDBACCCL = 0x210F,
        /// Relational database name.
        RDBNAM = 0x2110,
        /// Output expected.
        OUTEXP = 0x2111,
        /// Package name and consistency token.
        PKGNAMCT = 0x2112,
        /// Package name, consistency token and section number.
        PKGNAMCSN = 0x2113,
        /// Query block size.
        QRYBLKSZ = 0x2114,
        /// Unit of work disposition.
        UOWDSP = 0x2115,
        /// Return SQL descriptor area.
        RTNSQLDA = 0x2116,
        /// Statement string delimiter.
        STTSTRDEL = 0x2118,
        /// Statement decimal delimiter.
        STTDECDEL = 0x2119,
        /// RDB allow updates.
        RDBALWUPD = 0x211A,
        /// SQL cursor hold.
        SQLCSRHLD = 0x211F,
        /// Package default character subtype.
        PKGDFTCST = 0x2125,
        /// Default RDB collection.
        DFTRDBCOL = 0x2128,
        /// Query block protocol control.
        QRYBLKCTL = 0x2132,
        /// Correlation token.
        CRRTKN = 0x2135,
        /// Number of rows.
        NBRROW = 0x213A,
        /// Query row number.
        QRYROWNBR = 0x213D,
        /// Maximum result set count.
        MAXRSLCNT = 0x2140,
        /// Maximum number of extra blocks.
        MAXBLKEXT = 0x2141,
        /// Result set flags.
        RSLSETFLG = 0x2142,
        /// Type of SQL descriptor area.
        TYPSQLDA = 0x2146,
        /// Output override option.
        OUTOVROPT = 0x2147,
        /// Return extra data.
        RTNEXTDTA = 0x2148,
        /// Query attribute for scrollability.
        QRYATTSCR = 0x2149,
        /// Query relative scrolling action.
        QRYRELSCR = 0x214A,
        /// Query attribute for updatability.
        QRYATTUPD = 0x2150,
        /// Query scroll orientation.
        QRYSCRORN = 0x2152,
        /// Query row sensitivity.
        QRYROWSNS = 0x2153,
        /// Query block reset.
        QRYBLKRST = 0x2154,
        /// Query returns data.
        QRYRTNDTA = 0x2155,
        /// Query row set.
        QRYROWSET = 0x2156,
        /// Query attribute for sensitivity.
        QRYATTSNS = 0x2157,
        /// Query instance identifier.
        QRYINSID = 0x215B,
        /// Query close implicit.
        QRYCLSIMP = 0x215D,
        /// Query close lock release.
        QRYCLSRLS = 0x215E,
        /// IBM i additional authentication factor.
        SXXFACTOR = 0xD142,
        /// IBM i verification ID.
        SXXVERID = 0xD143,
        /// IBM i client IP address.
        SXXCLTIP = 0xD144,
        /// System/38 user file control block.
        S38UFCB = 0xD11F,
        /// System/38 operation list.
        S38OPTL = 0xD119,
        /// System/38 control list.
        S38CTLL = 0xD105,
        /// System/38 record buffer.
        S38BUF = 0xD405,
        /// System/38 I/O feedback.
        S38IOFB = 0xD402,
        /// System/38 open feedback.
        S38OPNFB = 0xD404,
        /// System/38 record count.
        S38LCNRD = 0xD406,
        /// System/38 close type.
        S38CLOST = 0xD121,
        /// System/38 command string.
        S38CMDST = 0xD103,
        /// System/38 message data.
        S38MDATA = 0xD10E,
        /// System/38 message file.
        S38MFILE = 0xD111,
        /// System/38 message ID.
        S38MID = 0xD112,
        /// System/38 message text.
        S38MTEXT = 0xD116,
        /// System/38 message type.
        S38MTYPE = 0xD117,
        /// System/38 lock request list.
        S38LRQS = 0xD10B,
        /// System/38 lock wait time.
        S38LWAIT = 0xD10C,
        /// System/38 lock release list.
        S38LRLS = 0xD115,
    }
}

/// Managers named in `MGRLVLLS`.
pub mod mgr {
    consts! {
        /// Agent.
        AGENT = 0x1403,
        /// Combined access method.
        CMBACCAM = 0x1405,
        /// Combined keyed access method.
        CMBKEYAM = 0x1406,
        /// Combined record number access method.
        CMBRNBAM = 0x1407,
        /// Direct file.
        DIRFIL = 0x140C,
        /// Directory access method.
        DRCAM = 0x1419,
        /// Keyed file.
        KEYFIL = 0x141E,
        /// Lock manager.
        LCKMGR = 0x1422,
        /// Alternate index file.
        ALTINDF = 0x1423,
        /// Relative by key access method.
        RELKEYAM = 0x1432,
        /// Relative by record number access method.
        RELRNBAM = 0x1433,
        /// Random by key access method.
        RNDKEYAM = 0x1434,
        /// Random by record number access method.
        RNDRNBAM = 0x1435,
        /// Sequential file.
        SEQFIL = 0x143B,
        /// Supervisor.
        SUPERVISOR = 0x143C,
        /// Security manager.
        SECMGR = 0x1440,
        /// APPC conversation manager.
        CMNAPPC = 0x1444,
        /// Directory.
        DIRECTORY = 0x1457,
        /// Dictionary.
        DICTIONARY = 0x1458,
        /// Stream access method.
        STRAM = 0x1463,
        /// Stream file.
        STRFIL = 0x1465,
        /// File isolation level.
        FILISOLVL = 0x1472,
        /// TCP/IP communications manager.
        CMNTCPIP = 0x1474,
        /// System command manager.
        SYSCMDMGR = 0x147F,
        /// Recovery manager.
        RSCRCVM = 0x14A0,
        /// Sync point manager.
        SYNCPTMGR = 0x14C0,
        /// Resynchronization manager.
        RSYNCMGR = 0x14C1,
        /// CCSID manager.
        CCSIDMGR = 0x14CC,
        /// XA manager.
        XAMGR = 0x1C01,
        /// Unicode manager.
        UNICODEMGR = 0x1C08,
        /// SQL application manager.
        SQLAM = 0x2407,
        /// Relational database.
        RDB = 0x240F,
    }
}

/// The System/38 record access commands.
pub mod s38 {
    consts! {
        /// Allocate an object.
        S38ALCOB = 0xD002,
        /// Close a file.
        S38CLOSE = 0xD004,
        /// Run a command.
        S38CMD = 0xD006,
        /// Delete the record last read.
        S38DEL = 0xD007,
        /// Deallocate an object.
        S38DLCOB = 0xD008,
        /// Force end of data.
        S38FEOD = 0xD00B,
        /// Get by position.
        S38GET = 0xD00C,
        /// Get by relative record number.
        S38GETD = 0xD00D,
        /// Get by key.
        S38GETK = 0xD00E,
        /// Get several records.
        S38GETM = 0xD00F,
        /// Open a file.
        S38OPEN = 0xD011,
        /// Put a record.
        S38PUT = 0xD012,
        /// Put several records.
        S38PUTM = 0xD013,
        /// Update the record last read.
        S38UPDAT = 0xD019,
        /// Put with direct positioning.
        SXXPUTDR = 0xD01C,
        /// ASP group request.
        SXXASPRQ = 0xD02A,
    }
}

/// `SECMEC` values.
pub mod secmec {
    consts! {
        /// DCE.
        DCESEC = 1,
        /// User ID and password.
        USRIDPWD = 3,
        /// User ID only.
        USRIDONL = 4,
        /// User ID, password and new password.
        USRIDNWPWD = 5,
        /// User ID and password substitute (DES).
        USRSBSPWD = 6,
        /// User ID and encrypted password.
        USRENCPWD = 7,
        /// User ID and strong password substitute (SHA).
        USRSSBPWD = 8,
        /// Encrypted user ID and password.
        EUSRIDPWD = 9,
        /// Encrypted user ID, password and new password.
        EUSRIDNWPWD = 10,
        /// Kerberos.
        KERSEC = 11,
        /// Encrypted user ID and security-sensitive data.
        EUSRIDDTA = 12,
        /// Encrypted user ID, password and security-sensitive data.
        EUSRPWDDTA = 13,
        /// Encrypted user ID only.
        EUSRIDONL = 16,
    }
}

/// `SECCHKCD` values.
pub mod secchkcd {
    consts! {
        /// Security information is good.
        OK = 0x00,
        /// The security mechanism is not supported.
        SECMEC_NOT_SUPPORTED = 0x01,
        /// The security token is missing or not valid.
        SECTKN_MISSING = 0x0B,
        /// The password has expired.
        PASSWORD_EXPIRED = 0x0E,
        /// The password is not valid.
        PASSWORD_INVALID = 0x0F,
        /// The password is missing.
        PASSWORD_MISSING = 0x10,
        /// The user ID is missing.
        USERID_MISSING = 0x12,
        /// The user ID is not valid.
        USERID_INVALID = 0x13,
        /// The user ID is revoked.
        USERID_REVOKED = 0x14,
        /// The new password is not valid.
        NEWPASSWORD_INVALID = 0x15,
        /// The encryption algorithm is not supported.
        ENCALG_NOT_SUPPORTED = 0x1B,
    }
}

/// `SVRCOD` values.
pub mod svrcod {
    consts! {
        /// Information.
        INFO = 0,
        /// Warning.
        WARNING = 4,
        /// Error.
        ERROR = 8,
        /// Severe.
        SEVERE = 16,
        /// Access damage.
        ACCDMG = 32,
        /// Permanent damage.
        PRMDMG = 64,
        /// Session damage.
        SESDMG = 128,
    }
}

/// Whether objects with this code point hold further objects.
pub fn is_collection(cp: u16) -> bool {
    contains(cmd::ALL, cp)
        || contains(rpy::ALL, cp)
        || contains(rm::ALL, cp)
        || contains(s38::ALL, cp)
        || cp == obj::TYPDEFOVR
        || cp == obj::SQLDTA
}

fn contains(t: &[(&str, u16)], cp: u16) -> bool {
    t.iter().any(|&(_, v)| v == cp)
}

/// The DDM name of a code point.
pub fn name(cp: u16) -> Option<&'static str> {
    [
        cmd::ALL,
        rpy::ALL,
        rm::ALL,
        obj::ALL,
        param::ALL,
        mgr::ALL,
        s38::ALL,
    ]
    .iter()
    .flat_map(|t| t.iter())
    .find(|&&(_, v)| v == cp)
    .map(|&(n, _)| n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_unique_per_value() {
        let mut seen = std::collections::HashMap::new();
        for t in [
            cmd::ALL,
            rpy::ALL,
            rm::ALL,
            obj::ALL,
            param::ALL,
            mgr::ALL,
            s38::ALL,
        ] {
            for &(n, v) in t {
                if let Some(other) = seen.insert(v, n) {
                    panic!("{n} and {other} share {v:04X}");
                }
            }
        }
        assert_eq!(name(0x200D), Some("PRPSQLSTT"));
    }
}
