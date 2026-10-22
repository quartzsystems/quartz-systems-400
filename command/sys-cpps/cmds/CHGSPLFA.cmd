/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGSPLFA: change the attributes of a spooled file, or of the files */
/* SELECT picks when FILE(*SELECT) is given.                           */
             CMD        PROMPT('Change Spooled File Attr') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHSPCMD) HLPID(CHGSPLFA)
             PARM       KWD(FILE) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*SELECT)) PROMPT('Spooled file')
             PARM       KWD(JOB) TYPE(QJ) DFT(*) SNGVAL((*)) +
                          PROMPT('Job name')
             PARM       KWD(SPLNBR) TYPE(*INT4) DFT(*ONLY) +
                          RANGE(1 999999) SPCVAL((*ONLY 0) (*LAST -1) +
                          (*ANY -2)) PROMPT('Spooled file number')
             PARM       KWD(JOBSYSNAME) TYPE(*CHAR) LEN(8) DFT(*ONLY) +
                          SPCVAL((*ONLY) (*CURRENT) (*ANY)) +
                          PROMPT('Job system name')
             PARM       KWD(CRTDATE) TYPE(EC) DFT(*ONLY) +
                          SNGVAL((*ONLY) (*LAST)) +
                          PROMPT('Spooled file created')
             PARM       KWD(SELECT) TYPE(ES) PMTCTL(PS) +
                          PROMPT('Select files for')
             PARM       KWD(PRTDEV) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*OUTQ)) PROMPT('Printer')
             PARM       KWD(OUTQ) TYPE(QO) DFT(*SAME) +
                          SNGVAL((*SAME) (*DEV)) PROMPT('Output queue')
             PARM       KWD(FORMTYPE) TYPE(*CHAR) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*STD)) PROMPT('Form type')
             PARM       KWD(COPIES) TYPE(*INT2) DFT(*SAME) RANGE(1 255) +
                          SPCVAL((*SAME 0)) PROMPT('Copies')
             PARM       KWD(RESTART) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 99999999) SPCVAL((*SAME 0) +
                          (*STRPAGE -1) (*ENDPAGE -2) (*NEXT -3)) +
                          PROMPT('Restart printing')
             PARM       KWD(PAGERANGE) TYPE(EP) DFT(*SAME) +
                          SNGVAL((*SAME)) PROMPT('Page range to print')
             PARM       KWD(OUTPTY) TYPE(*INT2) DFT(*SAME) RANGE(1 9) +
                          SPCVAL((*SAME 0) (*JOB -1)) +
                          PROMPT('Output priority')
             PARM       KWD(FILESEP) TYPE(*INT2) DFT(*SAME) RANGE(0 9) +
                          SPCVAL((*SAME -1) (*FILE -2)) +
                          PROMPT('File separators')
             PARM       KWD(SCHEDULE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *JOBEND *FILEEND +
                          *IMMED) PMTCTL(*PMTRQS) +
                          PROMPT('Spooled output schedule')
             PARM       KWD(SAVE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PMTCTL(*PMTRQS) PROMPT('Save file')
             PARM       KWD(OUTBIN) TYPE(*INT2) DFT(*SAME) +
                          RANGE(1 65535) SPCVAL((*SAME 0) (*DEVD -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Output bin')
             PARM       KWD(PRTQLTY) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *STD *DRAFT *NLQ +
                          *FASTDRAFT) PMTCTL(*PMTRQS) +
                          PROMPT('Print quality')
             PARM       KWD(FORMFEED) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *DEVD *CONT *CUT +
                          *AUTOCUT *CONT2) PMTCTL(*PMTRQS) +
                          PROMPT('Form feed')
             PARM       KWD(DRAWER) TYPE(*INT2) DFT(*SAME) RANGE(1 255) +
                          SPCVAL((*SAME 0) (*E1 -1) (*FORMDF -2)) +
                          PMTCTL(*PMTRQS) PROMPT('Source drawer')
             PARM       KWD(FONT) TYPE(EF) DFT(*SAME) +
                          SNGVAL((*SAME) (*DEVD) (*CPI)) +
                          PMTCTL(*PMTRQS) PROMPT('Font')
             PARM       KWD(PRTTXT) TYPE(*CHAR) LEN(30) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ') (*JOB)) +
                          PMTCTL(*PMTRQS) PROMPT('Print text')
             PARM       KWD(ALIGN) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PMTCTL(*PMTRQS) PROMPT('Align page')
             PARM       KWD(USRDTA) TYPE(*CHAR) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) PMTCTL(*PMTRQS) +
                          PROMPT('User data')
             PARM       KWD(PAGDFN) TYPE(QO) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Page definition')
             PARM       KWD(FORMDF) TYPE(QO) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE) (*DEVD)) +
                          PMTCTL(*PMTRQS) PROMPT('Form definition')
             PARM       KWD(USRDFNOPT) TYPE(*CHAR) LEN(10) MAX(4) +
                          DFT(*SAME) SNGVAL((*SAME) (*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('User defined options')
             PARM       KWD(USRDFNDTA) TYPE(*CHAR) LEN(255) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('User defined data')
             PARM       KWD(USRDFNOBJ) TYPE(EU) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('User defined object')
             PARM       KWD(EXPDATE) TYPE(*DATE) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE) (*DAYS)) +
                          PMTCTL(*PMTRQS) PROMPT('Expiration date')
             PARM       KWD(DAYS) TYPE(*INT2) DFT(*SAME) RANGE(1 366) +
                          SPCVAL((*SAME 0) (*NONE -1)) PMTCTL(*PMTRQS) +
                          PROMPT('Days to keep file')
             PS:        PMTCTL     CTL(FILE) COND((*EQ *SELECT))
             QJ:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) RANGE(000000 999999) +
                          FULL(*YES) PROMPT('Number')
             QO:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             EC:        ELEM       TYPE(*DATE) PROMPT('Creation date')
                        ELEM       TYPE(*TIME) DFT(*ONLY) +
                          SPCVAL((*ONLY) (*LAST)) PROMPT('Creation time')
             ES:        ELEM       TYPE(*NAME) LEN(10) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*ALL)) PROMPT('User')
                        ELEM       TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*OUTQ)) PROMPT('Print device')
                        ELEM       TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*STD)) PROMPT('Form type')
                        ELEM       TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('User data')
                        ELEM       TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*ASPDEV) (*CURASPGRP) (*SYSBAS)) +
                          PROMPT('ASP')
             EP:        ELEM       TYPE(*INT4) DFT(1) RANGE(1 99999999) +
                          PROMPT('Start page')
                        ELEM       TYPE(*INT4) DFT(*END) RANGE(1 99999999) +
                          SPCVAL((*END 0)) PROMPT('End page')
             EF:        ELEM       TYPE(*CHAR) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*DEVD) (*CPI)) +
                          PROMPT('Identifier')
                        ELEM       TYPE(*DEC) LEN(5 1) DFT(*NONE) +
                          RANGE(0.1 999.9) SPCVAL((*NONE 0)) +
                          PROMPT('Point size')
             EU:        ELEM       TYPE(QO) PROMPT('Object')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          VALUES(*DTAARA *DTAQ *FILE *PSFCFG *USRIDX +
                          *USRQ *USRSPC) PROMPT('Object type')
