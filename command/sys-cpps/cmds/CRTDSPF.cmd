/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTDSPF: create a display file from DDS source. */
             CMD        PROMPT('Create Display File') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(CRTDSPF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(SRCFILE) TYPE(Q2) DFT(QDDSSRC) +
                          PROMPT('Source file')
             PARM       KWD(SRCMBR) TYPE(*NAME) LEN(10) DFT(*FILE) +
                          SPCVAL((*FILE)) PROMPT('Source member')
             PARM       KWD(GENLVL) TYPE(*INT2) DFT(20) RANGE(0 30) +
                          PROMPT('Generation severity level')
             PARM       KWD(FLAG) TYPE(*INT2) DFT(0) RANGE(0 30) +
                          PROMPT('Flagging severity level')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MAX(50) DFT(*NONE) +
                          SNGVAL((*NONE) (*REQUESTER)) PROMPT('Device')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Description')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SRC) VALUES(*SRC *SOURCE *NOSRC +
                          *NOSOURCE *LIST *NOLIST *SECLVL *NOSECLVL +
                          *EVENTF *NOEVENTF) MAX(4) PMTCTL(*PMTRQS) +
                          PROMPT('Source listing options')
             PARM       KWD(MAXDEV) TYPE(*DEC) LEN(3) DFT(1) RANGE(1 256) +
                          PMTCTL(*PMTRQS) PROMPT('Maximum devices')
             PARM       KWD(ENHDSP) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Enhanced display')
             PARM       KWD(RSTDSP) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Restore display')
             PARM       KWD(DFRWRT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Defer write')
             PARM       KWD(CHRID) TYPE(E1) DFT(*DEVD) SNGVAL((*DEVD) +
                          (*JOBCCSID) (*CHRIDCTL)) PMTCTL(*PMTRQS) +
                          PROMPT('Character identifier')
             PARM       KWD(DECFMT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*FILE) VALUES(*FILE *JOB) PMTCTL(*PMTRQS) +
                          PROMPT('Decimal format')
             PARM       KWD(SFLENDTXT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*FILE) VALUES(*FILE *MSG) PMTCTL(*PMTRQS) +
                          PROMPT('Subfile end text')
             PARM       KWD(WAITFILE) TYPE(*DEC) LEN(5) DFT(*IMMED) +
                          RANGE(1 32767) SPCVAL((*IMMED 0) (*CLS -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Maximum file wait time')
             PARM       KWD(WAITRCD) TYPE(*DEC) LEN(5) DFT(*NOMAX) +
                          RANGE(1 32767) SPCVAL((*NOMAX -1) (*IMMED 0)) +
                          PMTCTL(*PMTRQS) PROMPT('Maximum record wait time')
             PARM       KWD(DTAQ) TYPE(Q2) DFT(*NONE) SNGVAL((*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('Data queue')
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
             E1:        ELEM       TYPE(*DEC) LEN(5) MIN(1) +
                          RANGE(1 32767) PROMPT('Graphic character set')
                        ELEM       TYPE(*DEC) LEN(5) MIN(1) +
                          RANGE(1 32767) PROMPT('Code page')
