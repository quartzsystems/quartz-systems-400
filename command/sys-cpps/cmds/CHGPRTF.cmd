/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGPRTF: change a printer file's attributes. */
             CMD        PROMPT('Change Printer File') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(CHGPRTF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*JOB) (*SYSVAL)) PROMPT('Device')
             PARM       KWD(DEVTYPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SCS *IPDS *USERASCII +
                          *AFPDS *LINE *AFPDSLINE) +
                          PROMPT('Printer device type')
             PARM       KWD(PAGESIZE) TYPE(E1) DFT(*SAME) SNGVAL((*SAME)) +
                          PROMPT('Page size')
             PARM       KWD(LPI) TYPE(*DEC) LEN(3 1) DFT(*SAME) RSTD(*YES) +
                          VALUES(3 4 6 7.5 8 9 12) SPCVAL((*SAME 0)) +
                          PROMPT('Lines per inch')
             PARM       KWD(CPI) TYPE(*DEC) LEN(3 1) DFT(*SAME) RSTD(*YES) +
                          VALUES(5 10 12 13.3 15 16.7 18 20) +
                          SPCVAL((*SAME 0)) PROMPT('Characters per inch')
             PARM       KWD(FRONTMGN) TYPE(E2) DFT(*SAME) SNGVAL((*SAME) +
                          (*DEVD)) PROMPT('Front margin')
             PARM       KWD(BACKMGN) TYPE(E2) DFT(*SAME) SNGVAL((*SAME) +
                          (*FRONTMGN) (*DEVD)) PROMPT('Back margin')
             PARM       KWD(OVRFLW) TYPE(*DEC) LEN(3) DFT(*SAME) +
                          RANGE(1 255) SPCVAL((*SAME 0)) +
                          PROMPT('Overflow line number')
             PARM       KWD(FOLD) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Fold records')
             PARM       KWD(RPLUNPRT) TYPE(E3) DFT(*SAME) SNGVAL((*SAME)) +
                          PROMPT('Replace unprintable characters')
             PARM       KWD(ALIGN) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Align page')
             PARM       KWD(CTLCHAR) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NONE *FCFC *MACHINE) +
                          PROMPT('Control character')
             PARM       KWD(CHLVAL) TYPE(E4) MAX(12) DFT(*SAME) +
                          SNGVAL((*SAME) (*NORMAL)) PROMPT('Channel values')
             PARM       KWD(FONT) TYPE(E5) DFT(*SAME) SNGVAL((*SAME) (*CPI) +
                          (*DEVD)) PROMPT('Font')
             PARM       KWD(FORMFEED) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *DEVD *CONT *CUT +
                          *AUTOCUT *CONT2) PROMPT('Form feed')
             PARM       KWD(DRAWER) TYPE(*DEC) LEN(3) DFT(*SAME) +
                          RANGE(1 255) SPCVAL((*SAME -2) (*E1 0) +
                          (*FORMDF -1)) PROMPT('Source drawer')
             PARM       KWD(OUTBIN) TYPE(*DEC) LEN(5) DFT(*SAME) +
                          RANGE(1 65535) SPCVAL((*SAME -1) (*DEVD 0)) +
                          PROMPT('Output bin')
             PARM       KWD(PRTQLTY) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *STD *DRAFT *NLQ +
                          *FASTDRAFT *DEVD) PROMPT('Print quality')
             PARM       KWD(FORMTYPE) TYPE(*CHAR) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*STD)) PROMPT('Form type')
             PARM       KWD(COPIES) TYPE(*DEC) LEN(3) DFT(*SAME) +
                          RANGE(1 255) SPCVAL((*SAME 0)) PROMPT('Copies')
             PARM       KWD(PAGRTT) TYPE(*DEC) LEN(3) DFT(*SAME) +
                          RSTD(*YES) VALUES(0 90 180 270) +
                          SPCVAL((*SAME -4) (*AUTO -1) (*DEVD -2) +
                          (*COR -3)) PROMPT('Degree of page rotation')
             PARM       KWD(MULTIUP) TYPE(*DEC) LEN(1) DFT(*SAME) +
                          RSTD(*YES) VALUES(1 2 4) SPCVAL((*SAME 0)) +
                          PROMPT('Pages per side')
             PARM       KWD(REDUCE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *TEXT *NONE) +
                          PROMPT('Reduce output')
             PARM       KWD(PRTTXT) TYPE(*CHAR) LEN(30) DFT(*SAME) +
                          SPCVAL((*SAME) (*JOB) (*BLANK ' ')) +
                          PROMPT('Print text')
             PARM       KWD(JUSTIFY) TYPE(*DEC) LEN(3) DFT(*SAME) +
                          RSTD(*YES) VALUES(0 50 100) SPCVAL((*SAME -1)) +
                          PROMPT('Hardware justification')
             PARM       KWD(DUPLEX) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES *TUMBLE +
                          *FORMDF) PROMPT('Print on both sides')
             PARM       KWD(UOM) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *INCH *CM) +
                          PROMPT('Unit of measure')
             PARM       KWD(FRONTOVL) TYPE(E6) DFT(*SAME) SNGVAL((*SAME) +
                          (*NONE)) PROMPT('Front side overlay')
             PARM       KWD(BACKOVL) TYPE(E6) DFT(*SAME) SNGVAL((*SAME) +
                          (*FRONTOVL) (*NONE)) PROMPT('Back side overlay')
             PARM       KWD(SPOOL) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Spool the data')
             PARM       KWD(OUTQ) TYPE(Q2) DFT(*SAME) SNGVAL((*SAME) (*JOB) +
                          (*DEV)) PROMPT('Output queue')
             PARM       KWD(MAXRCDS) TYPE(*DEC) LEN(6) DFT(*SAME) +
                          RANGE(1 999999) SPCVAL((*SAME -1) (*NOMAX 0)) +
                          PROMPT('Max spooled output records')
             PARM       KWD(FILESEP) TYPE(*DEC) LEN(1) DFT(*SAME) +
                          RANGE(0 9) SPCVAL((*SAME -2) (*FILE -1)) +
                          PROMPT('File separators')
             PARM       KWD(SCHEDULE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *FILEEND *JOBEND +
                          *IMMED) PROMPT('Spooled output schedule')
             PARM       KWD(HOLD) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Hold spooled file')
             PARM       KWD(SAVE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Save spooled file')
             PARM       KWD(OUTPTY) TYPE(*DEC) LEN(1) DFT(*SAME) +
                          RANGE(1 9) SPCVAL((*SAME -1) (*JOB 0)) +
                          PROMPT('Output priority on OUTQ')
             PARM       KWD(USRDTA) TYPE(*CHAR) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*SOURCE) (*BLANK ' ')) +
                          PROMPT('User data')
             PARM       KWD(SPLFOWN) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *CURUSRPRF *JOB +
                          *CURGRPPRF *JOBGRPPRF) PROMPT('Spooled file owner')
             PARM       KWD(USRDFNOPT) TYPE(*CHAR) LEN(10) MAX(4) +
                          DFT(*SAME) SNGVAL((*SAME) (*NONE)) +
                          PROMPT('User defined option')
             PARM       KWD(USRDFNDTA) TYPE(*CHAR) LEN(255) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE ' ')) +
                          PROMPT('User defined data')
             PARM       KWD(USRDFNOBJ) TYPE(E7) DFT(*SAME) SNGVAL((*SAME) +
                          (*NONE)) PROMPT('User defined object')
             PARM       KWD(SPLFNAME) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*FILE)) PROMPT('Spooled file name')
             PARM       KWD(IGCDTA) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('DBCS data')
             PARM       KWD(WAITFILE) TYPE(*DEC) LEN(5) DFT(*SAME) +
                          RANGE(1 32767) SPCVAL((*SAME -2) (*IMMED 0) +
                          (*CLS -1)) PROMPT('Maximum file wait time')
             PARM       KWD(SHARE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Share open data path')
             PARM       KWD(LVLCHK) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Record format level check')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) +
                          PROMPT('Description')
             Q1:        QUAL       TYPE(*GENERIC) LEN(10) MIN(1) +
                          SPCVAL((*ALL))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL) (*ALLUSR) +
                          (*ALL)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*DEC) LEN(5 3) DFT(66) +
                          RANGE(1 255) PROMPT('Length')
                        ELEM       TYPE(*DEC) LEN(5 3) DFT(132) +
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
