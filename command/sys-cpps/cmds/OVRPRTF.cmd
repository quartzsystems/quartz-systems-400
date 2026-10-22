/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* OVRPRTF: override a printer file's name and attributes for the */
/* programs that run after it.                                      */
             CMD        PROMPT('Override with Printer File') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(OVRPRTF)
             PARM       KWD(FILE) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*PRTF)) PROMPT('File being overridden')
             PARM       KWD(TOFILE) TYPE(Q1) DFT(*FILE) SNGVAL((*FILE)) +
                          PROMPT('Overriding to printer file')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) +
                          SPCVAL((*JOB) (*SYSVAL)) PROMPT('Device')
             PARM       KWD(DEVTYPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          VALUES(*SCS *IPDS *USERASCII *AFPDS *LINE +
                          *AFPDSLINE) PROMPT('Printer device type')
             PARM       KWD(PAGESIZE) TYPE(E1) PROMPT('Page size')
             PARM       KWD(LPI) TYPE(*DEC) LEN(3 1) RSTD(*YES) +
                          VALUES(3 4 6 7.5 8 9 12) PROMPT('Lines per inch')
             PARM       KWD(CPI) TYPE(*DEC) LEN(3 1) RSTD(*YES) +
                          VALUES(5 10 12 13.3 15 16.7 18 20) +
                          PROMPT('Characters per inch')
             PARM       KWD(FRONTMGN) TYPE(E2) SNGVAL((*DEVD)) +
                          PROMPT('Front margin')
             PARM       KWD(BACKMGN) TYPE(E2) SNGVAL((*FRONTMGN) (*DEVD)) +
                          PROMPT('Back margin')
             PARM       KWD(OVRFLW) TYPE(*DEC) LEN(3) RANGE(1 255) +
                          PROMPT('Overflow line number')
             PARM       KWD(FOLD) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*NO *YES) PROMPT('Fold records')
             PARM       KWD(RPLUNPRT) TYPE(E3) +
                          PROMPT('Replace unprintable characters')
             PARM       KWD(ALIGN) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*NO *YES) PROMPT('Align page')
             PARM       KWD(CTLCHAR) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          VALUES(*NONE *FCFC *MACHINE) +
                          PROMPT('Control character')
             PARM       KWD(CHLVAL) TYPE(E4) MAX(12) SNGVAL((*NORMAL)) +
                          PROMPT('Channel values')
             PARM       KWD(FONT) TYPE(E5) SNGVAL((*CPI) (*DEVD)) +
                          PROMPT('Font')
             PARM       KWD(FORMFEED) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          VALUES(*DEVD *CONT *CUT *AUTOCUT *CONT2) +
                          PROMPT('Form feed')
             PARM       KWD(DRAWER) TYPE(*DEC) LEN(3) RANGE(1 255) +
                          SPCVAL((*E1 0) (*FORMDF -1)) +
                          PROMPT('Source drawer')
             PARM       KWD(OUTBIN) TYPE(*DEC) LEN(5) RANGE(1 65535) +
                          SPCVAL((*DEVD 0)) PROMPT('Output bin')
             PARM       KWD(PRTQLTY) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          VALUES(*STD *DRAFT *NLQ *FASTDRAFT *DEVD) +
                          PROMPT('Print quality')
             PARM       KWD(FORMTYPE) TYPE(*CHAR) LEN(10) SPCVAL((*STD)) +
                          PROMPT('Form type')
             PARM       KWD(COPIES) TYPE(*DEC) LEN(3) RANGE(1 255) +
                          PROMPT('Copies')
             PARM       KWD(PAGRTT) TYPE(*DEC) LEN(3) RSTD(*YES) +
                          VALUES(0 90 180 270) SPCVAL((*AUTO -1) +
                          (*DEVD -2) (*COR -3)) +
                          PROMPT('Degree of page rotation')
             PARM       KWD(MULTIUP) TYPE(*DEC) LEN(1) RSTD(*YES) +
                          VALUES(1 2 4) PROMPT('Pages per side')
             PARM       KWD(REDUCE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          VALUES(*TEXT *NONE) PROMPT('Reduce output')
             PARM       KWD(PRTTXT) TYPE(*CHAR) LEN(30) +
                          SPCVAL((*JOB) (*BLANK ' ')) PROMPT('Print text')
             PARM       KWD(JUSTIFY) TYPE(*DEC) LEN(3) RSTD(*YES) +
                          VALUES(0 50 100) PROMPT('Hardware justification')
             PARM       KWD(DUPLEX) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          VALUES(*NO *YES *TUMBLE *FORMDF) +
                          PROMPT('Print on both sides')
             PARM       KWD(UOM) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          VALUES(*INCH *CM) PROMPT('Unit of measure')
             PARM       KWD(FRONTOVL) TYPE(E6) SNGVAL((*NONE)) +
                          PROMPT('Front side overlay')
             PARM       KWD(BACKOVL) TYPE(E6) SNGVAL((*FRONTOVL) (*NONE)) +
                          PROMPT('Back side overlay')
             PARM       KWD(SPOOL) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*YES *NO) PROMPT('Spool the data')
             PARM       KWD(OUTQ) TYPE(Q1) SNGVAL((*JOB) (*DEV)) +
                          PROMPT('Output queue')
             PARM       KWD(MAXRCDS) TYPE(*DEC) LEN(6) RANGE(1 999999) +
                          SPCVAL((*NOMAX 0)) +
                          PROMPT('Max spooled output records')
             PARM       KWD(FILESEP) TYPE(*DEC) LEN(1) RANGE(0 9) +
                          SPCVAL((*FILE -1)) PROMPT('File separators')
             PARM       KWD(SCHEDULE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          VALUES(*FILEEND *JOBEND *IMMED) +
                          PROMPT('Spooled output schedule')
             PARM       KWD(HOLD) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*NO *YES) PROMPT('Hold spooled file')
             PARM       KWD(SAVE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*NO *YES) PROMPT('Save spooled file')
             PARM       KWD(OUTPTY) TYPE(*DEC) LEN(1) RANGE(1 9) +
                          SPCVAL((*JOB 0)) PROMPT('Output priority on OUTQ')
             PARM       KWD(USRDTA) TYPE(*CHAR) LEN(10) +
                          SPCVAL((*SOURCE) (*BLANK ' ')) PROMPT('User data')
             PARM       KWD(SPLFOWN) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          VALUES(*CURUSRPRF *JOB *CURGRPPRF *JOBGRPPRF) +
                          PROMPT('Spooled file owner')
             PARM       KWD(USRDFNOPT) TYPE(*CHAR) LEN(10) MAX(4) +
                          SNGVAL((*NONE)) PROMPT('User defined option')
             PARM       KWD(USRDFNDTA) TYPE(*CHAR) LEN(255) +
                          SPCVAL((*NONE ' ')) PROMPT('User defined data')
             PARM       KWD(USRDFNOBJ) TYPE(E7) SNGVAL((*NONE)) +
                          PROMPT('User defined object')
             PARM       KWD(SPLFNAME) TYPE(*NAME) LEN(10) SPCVAL((*FILE)) +
                          PROMPT('Spooled file name')
             PARM       KWD(IGCDTA) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*NO *YES) PROMPT('DBCS data')
             PARM       KWD(WAITFILE) TYPE(*DEC) LEN(5) RANGE(1 32767) +
                          SPCVAL((*IMMED 0) (*CLS -1)) +
                          PROMPT('Maximum file wait time')
             PARM       KWD(SHARE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*NO *YES) PROMPT('Share open data path')
             PARM       KWD(LVLCHK) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*YES *NO) PROMPT('Record format level check')
             PARM       KWD(SECURE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*NO *YES) PROMPT('Secure from other overrides')
             PARM       KWD(OVRSCOPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ACTGRPDFN) VALUES(*ACTGRPDFN *CALLLVL *JOB) +
                          PROMPT('Override scope')
             PARM       KWD(OPNSCOPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ACTGRPDFN) VALUES(*ACTGRPDFN *JOB) +
                          PROMPT('Open scope')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*DEC) LEN(5 3) MIN(1) +
                          RANGE(1 255) PROMPT('Length')
                        ELEM       TYPE(*DEC) LEN(5 3) MIN(1) +
                          RANGE(1 378) PROMPT('Width')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ROWCOL) VALUES(*ROWCOL *UOM) +
                          PROMPT('Measurement method')
             E2:        ELEM       TYPE(*DEC) LEN(5 3) MIN(1) +
                          RANGE(0 57.790) PROMPT('Offset down')
                        ELEM       TYPE(*DEC) LEN(5 3) MIN(1) +
                          RANGE(0 57.790) PROMPT('Offset across')
             E3:        ELEM       TYPE(*CHAR) LEN(4) RSTD(*YES) MIN(1) +
                          VALUES(*YES *NO) PROMPT('Replace')
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
             E6:        ELEM       TYPE(Q1) MIN(1) PROMPT('Overlay')
                        ELEM       TYPE(*DEC) LEN(5 3) DFT(0) +
                          RANGE(0 57.790) PROMPT('Offset down')
                        ELEM       TYPE(*DEC) LEN(5 3) DFT(0) +
                          RANGE(0 57.790) PROMPT('Offset across')
             E7:        ELEM       TYPE(Q1) MIN(1) PROMPT('Object')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) MIN(1) +
                          VALUES(*DTAARA *DTAQ *FILE *PSFCFG *USRIDX +
                          *USRQ *USRSPC) PROMPT('Object type')
