/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTSQLRPGI: precompile ILE RPG source with embedded SQL, then make */
/* a program, a service program or a module of it. Parameters, order  */
/* and defaults as pub400's prompt (tools/conformance/golden/sql/).   */
             CMD        PROMPT('Create SQL ILE RPG Object') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(CRTSQLRPGI)
             PARM       KWD(OBJ) TYPE(Q1) MIN(1) PROMPT('Object')
             PARM       KWD(SRCFILE) TYPE(Q2) PROMPT('Source file')
             PARM       KWD(SRCMBR) TYPE(*NAME) LEN(10) DFT(*OBJ) +
                          SPCVAL((*OBJ)) PROMPT('Source member')
             PARM       KWD(SRCSTMF) TYPE(*PNAME) LEN(5000) +
                          CASE(*MIXED) PROMPT('Source stream file')
             PARM       KWD(COMMIT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*CHG) VALUES(*CHG *UR *ALL *RS *CS *NONE +
                          *NC *RR) PROMPT('Commitment control')
             PARM       KWD(RDB) TYPE(*CHAR) LEN(18) DFT(*LOCAL) +
                          SPCVAL((*LOCAL)) PROMPT('Relational database')
             PARM       KWD(OBJTYPE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*PGM) VALUES(*PGM *SRVPGM *MODULE) +
                          PROMPT('Compile type')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *PRINT) +
                          PROMPT('Listing output')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SRCMBRTXT) +
                          SPCVAL((*SRCMBRTXT) (*BLANK ' ')) +
                          PROMPT('Text ''description''')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*XREF *GEN *NOSECLVL *SYS *JOB *NOCVTDT +
                          *NOEVENTF *NOEXTIND) MAX(12) VALUES(*XREF +
                          *NOXREF *GEN *NOGEN *SECLVL *NOSECLVL *SYS +
                          *SQL *JOB *SYSVAL *PERIOD *COMMA *CVTDT +
                          *NOCVTDT *EVENTF *NOEVENTF *EXTIND *NOEXTIND +
                          *NOLSTDBG *LSTDBG) PMTCTL(*PMTRQS) +
                          PROMPT('Precompiler options')
             PARM       KWD(RPGPPOPT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *LVL1 *LVL2) +
                          PMTCTL(*PMTRQS) PROMPT('RPG preprocessor options')
             PARM       KWD(TGTRLS) TYPE(*CHAR) LEN(8) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*PRV)) PMTCTL(*PMTRQS) +
                          PROMPT('Target release')
             PARM       KWD(INCFILE) TYPE(Q7) PMTCTL(*PMTRQS) +
                          PROMPT('INCLUDE file')
             PARM       KWD(INCDIR) TYPE(*PNAME) LEN(1024) DFT(*NONE) +
                          SPCVAL((*NONE)) CASE(*MIXED) PMTCTL(*PMTRQS) +
                          PROMPT('SQL INCLUDE directory')
             PARM       KWD(ALWCPYDTA) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*OPTIMIZE) VALUES(*OPTIMIZE *YES *NO) +
                          PMTCTL(*PMTRQS) PROMPT('Allow copy of data')
             PARM       KWD(CLOSQLCSR) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ENDACTGRP) VALUES(*ENDACTGRP *ENDMOD) +
                          PMTCTL(*PMTRQS) PROMPT('Close SQL cursor')
             PARM       KWD(ALWBLK) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*ALLREAD) VALUES(*ALLREAD *NONE *READ) +
                          PMTCTL(*PMTRQS) PROMPT('Allow blocking')
             PARM       KWD(DLYPRP) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Delay PREPARE')
             PARM       KWD(CONACC) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*DFT) VALUES(*DFT *CURCMT *WAIT) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Concurrent access resolution')
             PARM       KWD(GENLVL) TYPE(*DEC) LEN(2 0) DFT(10) +
                          RANGE(0 40) PMTCTL(*PMTRQS) +
                          PROMPT('Severity level')
             PARM       KWD(DATFMT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*JOB) VALUES(*JOB *USA *ISO *EUR *JIS +
                          *MDY *DMY *YMD *JUL) PMTCTL(*PMTRQS) +
                          PROMPT('Date format')
             PARM       KWD(DATSEP) TYPE(*CHAR) LEN(4) DFT(*JOB) +
                          SPCVAL((*JOB) (*BLANK ' ')) PMTCTL(*PMTRQS) +
                          PROMPT('Date separator character')
             PARM       KWD(TIMFMT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*HMS) VALUES(*HMS *USA *ISO *EUR *JIS) +
                          PMTCTL(*PMTRQS) PROMPT('Time format')
             PARM       KWD(TIMSEP) TYPE(*CHAR) LEN(4) DFT(*JOB) +
                          SPCVAL((*JOB) (*BLANK ' ')) PMTCTL(*PMTRQS) +
                          PROMPT('Time separator character')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Replace')
             PARM       KWD(RDBCNNMTH) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*DUW) VALUES(*DUW *RUW) PMTCTL(*PMTRQS) +
                          PROMPT('RDB connect method')
             PARM       KWD(DFTRDBCOL) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Default collection')
             PARM       KWD(DYNDFTCOL) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Dynamic default collection')
             PARM       KWD(SQLPKG) TYPE(Q4) DFT(*OBJ) SNGVAL((*OBJ)) +
                          PMTCTL(*PMTRQS) PROMPT('Package')
             PARM       KWD(SQLPATH) TYPE(*NAME) LEN(10) DFT(*NAMING) +
                          SPCVAL((*NAMING) (*LIBL)) MAX(268) +
                          PMTCTL(*PMTRQS) PROMPT('SQL path')
             PARM       KWD(SQLCURRULE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*DB2) VALUES(*DB2 *STD) PMTCTL(*PMTRQS) +
                          PROMPT('SQL rules')
             PARM       KWD(SAAFLAG) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NOFLAG) VALUES(*NOFLAG *FLAG) +
                          PMTCTL(*PMTRQS) PROMPT('IBM SQL flagging')
             PARM       KWD(FLAGSTD) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ANS) PMTCTL(*PMTRQS) +
                          PROMPT('ANS flagging')
             PARM       KWD(PRTFILE) TYPE(Q6) PMTCTL(*PMTRQS) PROMPT('Print file')
             PARM       KWD(DBGVIEW) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *SOURCE *STMT *LIST) +
                          PMTCTL(*PMTRQS) PROMPT('Debugging view')
             PARM       KWD(DBGENCKEY) TYPE(*CHAR) LEN(16) DFT(*NONE) +
                          SPCVAL((*NONE)) CASE(*MIXED) PMTCTL(*PMTRQS) +
                          PROMPT('Debug encryption key')
             PARM       KWD(USRPRF) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NAMING) VALUES(*NAMING *USER *OWNER) +
                          PMTCTL(*PMTRQS) PROMPT('User profile')
             PARM       KWD(DYNUSRPRF) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*USER) VALUES(*USER *OWNER) +
                          PMTCTL(*PMTRQS) PROMPT('Dynamic user profile')
             PARM       KWD(SRTSEQ) TYPE(Q3) DFT(*JOB) SNGVAL((*JOB) +
                          (*HEX) (*JOBRUN) (*LANGIDUNQ) (*LANGIDSHR)) +
                          PMTCTL(*PMTRQS) PROMPT('Sort sequence')
             PARM       KWD(LANGID) TYPE(*NAME) LEN(7) DFT(*JOB) +
                          SPCVAL((*JOB) (*JOBRUN)) PMTCTL(*PMTRQS) +
                          PROMPT('Language id')
             PARM       KWD(CVTCCSID) TYPE(*INT4) DFT(*NONE) +
                          SPCVAL((*NONE 0) (*JOB -1)) PMTCTL(*PMTRQS) +
                          PROMPT('Conversion CCSID')
             PARM       KWD(TOSRCFILE) TYPE(Q5) PMTCTL(*PMTRQS) +
                          PROMPT('To source file')
             PARM       KWD(DECRESULT) TYPE(E1) PMTCTL(*PMTRQS) +
                          PROMPT('Decimal result options')
             PARM       KWD(DECFLTRND) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*HALFEVEN) VALUES(*HALFEVEN *UP *DOWN +
                          *CEILING *FLOOR *HALFUP *HALFDOWN) +
                          PMTCTL(*PMTRQS) PROMPT('Decimal float rounding mode')
             PARM       KWD(COMPILEOPT) TYPE(*CHAR) LEN(5000) +
                          DFT(*NONE) SPCVAL((*NONE)) CASE(*MIXED) +
                          PMTCTL(*PMTRQS) PROMPT('Compiler options')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) DFT(QRPGLESRC)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q3:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q4:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*OBJLIB) +
                          SPCVAL((*OBJLIB)) PROMPT('Library')
             Q5:        QUAL       TYPE(*NAME) LEN(10) DFT(*CALC) +
                          SPCVAL((*CALC))
                        QUAL       TYPE(*NAME) LEN(10) DFT(QTEMP) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q6:        QUAL       TYPE(*NAME) LEN(10) DFT(QSYSPRT)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q7:        QUAL       TYPE(*NAME) LEN(10) DFT(*SRCFILE) +
                          SPCVAL((*SRCFILE))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*INT4) DFT(31) RANGE(31 63) +
                          PROMPT('Maximum precision')
                        ELEM       TYPE(*INT4) DFT(31) RANGE(31 63) +
                          PROMPT('Maximum scale')
                        ELEM       TYPE(*INT4) DFT(0) RANGE(0 9) +
                          PROMPT('Minimum divide scale')
