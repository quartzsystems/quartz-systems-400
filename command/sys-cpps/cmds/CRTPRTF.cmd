/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTPRTF: create a printer file, from DDS source or as a plain one. */
             CMD        PROMPT('Create Printer File') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(CRTPRTF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(SRCFILE) TYPE(Q2) DFT(*NONE) SNGVAL((*NONE)) +
                          PROMPT('Source file')
             PARM       KWD(SRCMBR) TYPE(*NAME) LEN(10) DFT(*FILE) +
                          SPCVAL((*FILE)) PROMPT('Source member')
             PARM       KWD(GENLVL) TYPE(*INT2) DFT(20) RANGE(0 30) +
                          PROMPT('Generation severity level')
             PARM       KWD(FLAG) TYPE(*INT2) DFT(0) RANGE(0 30) +
                          PROMPT('Flagging severity level')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) DFT(*JOB) +
                          SPCVAL((*JOB) (*SYSVAL)) PROMPT('Device')
             PARM       KWD(DEVTYPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SCS) VALUES(*SCS *IPDS *USERASCII *AFPDS +
                          *LINE *AFPDSLINE) PROMPT('Printer device type')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Description')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SRC) VALUES(*SRC *SOURCE *NOSRC +
                          *NOSOURCE *LIST *NOLIST *SECLVL *NOSECLVL +
                          *EVENTF *NOEVENTF) MAX(4) PMTCTL(*PMTRQS) +
                          PROMPT('Source listing options')
             PARM       KWD(PAGESIZE) TYPE(E1) PMTCTL(*PMTRQS) +
                          PROMPT('Page size')
             PARM       KWD(LPI) TYPE(*DEC) LEN(3 1) DFT(6) RSTD(*YES) +
                          VALUES(3 4 6 7.5 8 9 12) PMTCTL(*PMTRQS) +
                          PROMPT('Lines per inch')
             PARM       KWD(CPI) TYPE(*DEC) LEN(3 1) DFT(10) RSTD(*YES) +
                          VALUES(5 10 12 13.3 15 16.7 18 20) +
                          PMTCTL(*PMTRQS) PROMPT('Characters per inch')
             PARM       KWD(FRONTMGN) TYPE(E2) DFT(*DEVD) SNGVAL((*DEVD)) +
                          PMTCTL(*PMTRQS) PROMPT('Front margin')
             PARM       KWD(BACKMGN) TYPE(E2) DFT(*FRONTMGN) +
                          SNGVAL((*FRONTMGN) (*DEVD)) PMTCTL(*PMTRQS) +
                          PROMPT('Back margin')
             PARM       KWD(OVRFLW) TYPE(*DEC) LEN(3) DFT(60) +
                          RANGE(1 255) PMTCTL(*PMTRQS) +
                          PROMPT('Overflow line number')
             PARM       KWD(FOLD) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Fold records')
             PARM       KWD(RPLUNPRT) TYPE(E3) PMTCTL(*PMTRQS) +
                          PROMPT('Replace unprintable characters')
             PARM       KWD(ALIGN) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Align page')
             PARM       KWD(CTLCHAR) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *FCFC *MACHINE) +
                          PMTCTL(*PMTRQS) PROMPT('Control character')
             PARM       KWD(CHLVAL) TYPE(E4) MAX(12) DFT(*NORMAL) +
                          SNGVAL((*NORMAL)) PMTCTL(*PMTRQS) +
                          PROMPT('Channel values')
             PARM       KWD(FONT) TYPE(E5) DFT(*CPI) SNGVAL((*CPI) (*DEVD)) +
                          PMTCTL(*PMTRQS) PROMPT('Font')
             PARM       KWD(FORMFEED) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*DEVD) VALUES(*DEVD *CONT *CUT *AUTOCUT +
                          *CONT2) PMTCTL(*PMTRQS) PROMPT('Form feed')
             PARM       KWD(DRAWER) TYPE(*DEC) LEN(3) DFT(1) RANGE(1 255) +
                          SPCVAL((*E1 0) (*FORMDF -1)) PMTCTL(*PMTRQS) +
                          PROMPT('Source drawer')
             PARM       KWD(OUTBIN) TYPE(*DEC) LEN(5) DFT(*DEVD) +
                          RANGE(1 65535) SPCVAL((*DEVD 0)) +
                          PMTCTL(*PMTRQS) PROMPT('Output bin')
             PARM       KWD(PRTQLTY) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*STD) VALUES(*STD *DRAFT *NLQ *FASTDRAFT +
                          *DEVD) PMTCTL(*PMTRQS) PROMPT('Print quality')
             PARM       KWD(FORMTYPE) TYPE(*CHAR) LEN(10) DFT(*STD) +
                          SPCVAL((*STD)) PMTCTL(*PMTRQS) PROMPT('Form type')
             PARM       KWD(COPIES) TYPE(*DEC) LEN(3) DFT(1) RANGE(1 255) +
                          PMTCTL(*PMTRQS) PROMPT('Copies')
             PARM       KWD(PAGRTT) TYPE(*DEC) LEN(3) DFT(*AUTO) +
                          RSTD(*YES) VALUES(0 90 180 270) +
                          SPCVAL((*AUTO -1) (*DEVD -2) (*COR -3)) +
                          PMTCTL(*PMTRQS) PROMPT('Degree of page rotation')
             PARM       KWD(MULTIUP) TYPE(*DEC) LEN(1) DFT(1) RSTD(*YES) +
                          VALUES(1 2 4) PMTCTL(*PMTRQS) +
                          PROMPT('Pages per side')
             PARM       KWD(REDUCE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*TEXT) VALUES(*TEXT *NONE) PMTCTL(*PMTRQS) +
                          PROMPT('Reduce output')
             PARM       KWD(PRTTXT) TYPE(*CHAR) LEN(30) DFT(*JOB) +
                          SPCVAL((*JOB) (*BLANK ' ')) PMTCTL(*PMTRQS) +
                          PROMPT('Print text')
             PARM       KWD(JUSTIFY) TYPE(*DEC) LEN(3) DFT(0) RSTD(*YES) +
                          VALUES(0 50 100) PMTCTL(*PMTRQS) +
                          PROMPT('Hardware justification')
             PARM       KWD(DUPLEX) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES *TUMBLE *FORMDF) +
                          PMTCTL(*PMTRQS) PROMPT('Print on both sides')
             PARM       KWD(UOM) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*INCH) VALUES(*INCH *CM) PMTCTL(*PMTRQS) +
                          PROMPT('Unit of measure')
             PARM       KWD(FRONTOVL) TYPE(E6) DFT(*NONE) SNGVAL((*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('Front side overlay')
             PARM       KWD(BACKOVL) TYPE(E6) DFT(*FRONTOVL) +
                          SNGVAL((*FRONTOVL) (*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Back side overlay')
             PARM       KWD(SPOOL) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Spool the data')
             PARM       KWD(OUTQ) TYPE(Q2) DFT(*JOB) SNGVAL((*JOB) (*DEV)) +
                          PMTCTL(*PMTRQS) PROMPT('Output queue')
             PARM       KWD(MAXRCDS) TYPE(*DEC) LEN(6) DFT(100000) +
                          RANGE(1 999999) SPCVAL((*NOMAX 0)) +
                          PMTCTL(*PMTRQS) PROMPT('Max spooled output records')
             PARM       KWD(FILESEP) TYPE(*DEC) LEN(1) DFT(0) RANGE(0 9) +
                          SPCVAL((*FILE -1)) PMTCTL(*PMTRQS) +
                          PROMPT('File separators')
             PARM       KWD(SCHEDULE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*FILEEND) VALUES(*FILEEND *JOBEND *IMMED) +
                          PMTCTL(*PMTRQS) PROMPT('Spooled output schedule')
             PARM       KWD(HOLD) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Hold spooled file')
             PARM       KWD(SAVE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Save spooled file')
             PARM       KWD(OUTPTY) TYPE(*DEC) LEN(1) DFT(*JOB) +
                          RANGE(1 9) SPCVAL((*JOB 0)) PMTCTL(*PMTRQS) +
                          PROMPT('Output priority on OUTQ')
             PARM       KWD(USRDTA) TYPE(*CHAR) LEN(10) DFT(*SOURCE) +
                          SPCVAL((*SOURCE) (*BLANK ' ')) PMTCTL(*PMTRQS) +
                          PROMPT('User data')
             PARM       KWD(SPLFOWN) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*CURUSRPRF) VALUES(*CURUSRPRF *JOB +
                          *CURGRPPRF *JOBGRPPRF) PMTCTL(*PMTRQS) +
                          PROMPT('Spooled file owner')
             PARM       KWD(USRDFNOPT) TYPE(*CHAR) LEN(10) MAX(4) +
                          DFT(*NONE) SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('User defined option')
             PARM       KWD(USRDFNDTA) TYPE(*CHAR) LEN(255) DFT(*NONE) +
                          SPCVAL((*NONE ' ')) PMTCTL(*PMTRQS) +
                          PROMPT('User defined data')
             PARM       KWD(USRDFNOBJ) TYPE(E7) DFT(*NONE) SNGVAL((*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('User defined object')
             PARM       KWD(SPLFNAME) TYPE(*NAME) LEN(10) DFT(*FILE) +
                          SPCVAL((*FILE)) PMTCTL(*PMTRQS) +
                          PROMPT('Spooled file name')
             PARM       KWD(IGCDTA) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('DBCS data')
             PARM       KWD(WAITFILE) TYPE(*DEC) LEN(5) DFT(*IMMED) +
                          RANGE(1 32767) SPCVAL((*IMMED 0) (*CLS -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Maximum file wait time')
             PARM       KWD(SHARE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Share open data path')
             PARM       KWD(LVLCHK) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Record format level check')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) +
                          PROMPT('Public authority')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Replace file')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*DEC) LEN(7 3) DFT(66) +
                          RANGE(1 255) PROMPT('Length')
                        ELEM       TYPE(*DEC) LEN(7 3) DFT(132) +
                          RANGE(1 378) PROMPT('Width')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ROWCOL) VALUES(*ROWCOL *UOM) +
                          PROMPT('Measurement method')
             E2:        ELEM       TYPE(*DEC) LEN(5 3) DFT(0) +
                          RANGE(0 57.790) PROMPT('Offset down')
                        ELEM       TYPE(*DEC) LEN(5 3) DFT(0) +
                          RANGE(0 57.790) PROMPT('Offset across')
             E3:        ELEM       TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PROMPT('Replace')
                        ELEM       TYPE(*CHAR) LEN(1) DFT(' ') +
                          PROMPT('Replacement character')
             E4:        ELEM       TYPE(*DEC) LEN(2) MIN(1) RANGE(1 12) +
                          PROMPT('Channel')
                        ELEM       TYPE(*DEC) LEN(3) MIN(1) RANGE(1 255) +
                          PROMPT('Line number')
             E5:        ELEM       TYPE(*DEC) LEN(5) MIN(1) +
                          RANGE(1 65535) PROMPT('Identifier')
                        ELEM       TYPE(*DEC) LEN(4 1) DFT(*NONE) +
                          RANGE(0.1 999.9) SPCVAL((*NONE 0)) +
                          PROMPT('Point size')
             E6:        ELEM       TYPE(Q2) MIN(1) PROMPT('Overlay')
                        ELEM       TYPE(*DEC) LEN(5 3) DFT(0) +
                          RANGE(0 57.790) PROMPT('Offset down')
                        ELEM       TYPE(*DEC) LEN(5 3) DFT(0) +
                          RANGE(0 57.790) PROMPT('Offset across')
             E7:        ELEM       TYPE(Q2) MIN(1) PROMPT('Object')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) MIN(1) +
                          VALUES(*DTAARA *DTAQ *FILE *PSFCFG *USRIDX +
                          *USRQ *USRSPC) PROMPT('Object type')
