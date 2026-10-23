/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTRPGMOD: compile ILE RPG source into a module. */
             CMD        PROMPT('Create RPG Module') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(CRTRPGMOD)
             PARM       KWD(MODULE) TYPE(Q1) DFT(*CTLSPEC) +
                          SNGVAL((*CTLSPEC)) PROMPT('Module')
             PARM       KWD(SRCFILE) TYPE(Q2) PROMPT('Source file')
             PARM       KWD(SRCMBR) TYPE(*NAME) LEN(10) DFT(*MODULE) +
                          SPCVAL((*MODULE)) PROMPT('Source member')
             PARM       KWD(SRCSTMF) TYPE(*PNAME) LEN(5000) +
                          CASE(*MIXED) PROMPT('Source stream file')
             PARM       KWD(GENLVL) TYPE(*DEC) LEN(2 0) DFT(10) +
                          RANGE(0 20) PROMPT('Severity that stops +
                          creation')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SRCMBRTXT) +
                          SPCVAL((*SRCMBRTXT) (*BLANK ' ')) +
                          PROMPT('Description')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*PRINT) VALUES(*PRINT *NONE) +
                          PMTCTL(*PMTRQS) PROMPT('Listing')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*XREF *GEN *NOSECLVL *SHOWCPY *EXPDDS +
                          *EXT *NOSHOWSKP *NOSRCSTMT *DEBUGIO *NOEVENTF) +
                          MAX(12) VALUES(*XREF *NOXREF *GEN *NOGEN +
                          *SECLVL *NOSECLVL *SHOWCPY *NOSHOWCPY +
                          *EXPDDS *NOEXPDDS *EXT *NOEXT *SHOWSKP +
                          *NOSHOWSKP *SRCSTMT *NOSRCSTMT *DEBUGIO +
                          *NODEBUGIO *UNREF *NOUNREF *EVENTF *NOEVENTF) +
                          PMTCTL(*PMTRQS) PROMPT('Compile options')
             PARM       KWD(DBGVIEW) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*STMT) VALUES(*STMT *SOURCE *LIST *COPY +
                          *ALL *NONE) PMTCTL(*PMTRQS) +
                          PROMPT('Debugging view')
             PARM       KWD(DBGENCKEY) TYPE(*CHAR) LEN(16) DFT(*NONE) +
                          SPCVAL((*NONE)) CASE(*MIXED) PMTCTL(*PMTRQS) +
                          PROMPT('Debug encryption key')
             PARM       KWD(OPTIMIZE) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *BASIC *FULL) +
                          PMTCTL(*PMTRQS) PROMPT('Optimize')
             PARM       KWD(INDENT) TYPE(*CHAR) LEN(2) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Listing nesting mark')
             PARM       KWD(CVTOPT) TYPE(*CHAR) LEN(13) RSTD(*YES) +
                          DFT(*NONE) SNGVAL((*NONE)) MAX(4) +
                          VALUES(*DATETIME *NODATETIME *GRAPHIC +
                          *NOGRAPHIC *VARCHAR *NOVARCHAR *VARGRAPHIC +
                          *NOVARGRAPHIC) PMTCTL(*PMTRQS) +
                          PROMPT('External types to convert')
             PARM       KWD(SRTSEQ) TYPE(Q3) DFT(*HEX) SNGVAL((*HEX) +
                          (*JOB) (*JOBRUN) (*LANGIDUNQ) (*LANGIDSHR)) +
                          PMTCTL(*PMTRQS) PROMPT('Sort sequence')
             PARM       KWD(LANGID) TYPE(*NAME) LEN(3) DFT(*JOBRUN) +
                          SPCVAL((*JOBRUN) (*JOB)) PMTCTL(*PMTRQS) +
                          PROMPT('Language ID')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Replace module')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) PROMPT('Authority')
             PARM       KWD(TRUNCNBR) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Truncate numbers that overflow')
             PARM       KWD(FIXNBR) TYPE(*CHAR) LEN(14) RSTD(*YES) +
                          DFT(*NONE) SNGVAL((*NONE)) MAX(2) +
                          VALUES(*ZONED *NOZONED *INPUTPACKED +
                          *NOINPUTPACKED) PMTCTL(*PMTRQS) +
                          PROMPT('Repair bad decimal data')
             PARM       KWD(TGTRLS) TYPE(*CHAR) LEN(8) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*PRV)) PMTCTL(*PMTRQS) +
                          PROMPT('Target release')
             PARM       KWD(ALWNULL) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *INPUTONLY *USRCTL *YES) +
                          PMTCTL(*PMTRQS) PROMPT('Null-capable fields')
             PARM       KWD(DEFINE) TYPE(*CHAR) LEN(50) DFT(*NONE) +
                          SNGVAL((*NONE)) MAX(32) PMTCTL(*PMTRQS) +
                          PROMPT('Condition names to define')
             PARM       KWD(ENBPFRCOL) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*PEP) VALUES(*PEP *ENTRYEXIT *FULL) +
                          PMTCTL(*PMTRQS) PROMPT('Performance collection')
             PARM       KWD(STGMDL) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*INHERIT) VALUES(*SNGLVL *TERASPACE +
                          *INHERIT) PMTCTL(*PMTRQS) PROMPT('Storage model')
             PARM       KWD(BNDDIR) TYPE(Q4) MAX(50) DFT(*NONE) +
                          SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Binding directory')
             PARM       KWD(PRFDTA) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*NOCOL) VALUES(*NOCOL *COL) +
                          PMTCTL(*PMTRQS) PROMPT('Profiling data')
             PARM       KWD(LICOPT) TYPE(*CHAR) LEN(1024) DFT(*NONE) +
                          SPCVAL((*NONE)) CASE(*MIXED) PMTCTL(*PMTRQS) +
                          PROMPT('Code generator options')
             PARM       KWD(INCDIR) TYPE(*PNAME) LEN(1024) DFT(*NONE) +
                          SNGVAL((*NONE)) MAX(32) CASE(*MIXED) +
                          PMTCTL(*PMTRQS) PROMPT('Include directories')
             PARM       KWD(PGMINFO) TYPE(E1) DFT(*NO) SNGVAL((*NO)) +
                          PMTCTL(*PMTRQS) PROMPT('Interface information')
             PARM       KWD(INFOSTMF) TYPE(*PNAME) LEN(5000) +
                          CASE(*MIXED) PMTCTL(*PMTRQS) +
                          PROMPT('Interface stream file')
             PARM       KWD(PPGENOPT) TYPE(*CHAR) LEN(14) RSTD(*YES) +
                          DFT(*NONE) SNGVAL((*NONE) (*DFT)) MAX(4) +
                          VALUES(*RMVCOMMENT *NORMVCOMMENT +
                          *EXPINCLUDE *NOEXPINCLUDE *NOSEQSRC +
                          *SEQSRC) PMTCTL(*PMTRQS) +
                          PROMPT('Preprocessor output options')
             PARM       KWD(PPSRCFILE) TYPE(Q5) PMTCTL(*PMTRQS) +
                          PROMPT('Preprocessor output file')
             PARM       KWD(PPSRCMBR) TYPE(*NAME) LEN(10) DFT(*MODULE) +
                          SPCVAL((*MODULE)) PMTCTL(*PMTRQS) +
                          PROMPT('Preprocessor output member')
             PARM       KWD(PPSRCSTMF) TYPE(*PNAME) LEN(5000) +
                          DFT(*SRCSTMF) SPCVAL((*SRCSTMF)) CASE(*MIXED) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Preprocessor stream file')
             PARM       KWD(TGTCCSID) TYPE(*INT4) DFT(*SRC) +
                          SPCVAL((*SRC 0) (*JOB -1)) PMTCTL(*PMTRQS) +
                          PROMPT('CCSID to compile in')
             PARM       KWD(REQPREXP) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *WARN *REQUIRE) +
                          PMTCTL(*PMTRQS) PROMPT('Prototypes for exports')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) DFT(QRPGLESRC)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q3:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q4:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL)) +
                          PROMPT('Library')
             Q5:        QUAL       TYPE(*NAME) LEN(10) DFT(QSQLPRE)
                        QUAL       TYPE(*NAME) LEN(10) DFT(QTEMP) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          VALUES(*PCML) PROMPT('Kind')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*STMF) VALUES(*STMF *MODULE *ALL) +
                          PROMPT('Kept in')
