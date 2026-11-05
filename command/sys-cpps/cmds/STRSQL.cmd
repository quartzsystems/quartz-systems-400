/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRSQL: start the interactive SQL session. Parameters, order and     */
/* defaults as pub400's prompt (tools/conformance/golden/sql/prompts/). */
             CMD        PROMPT('Start SQL Interactive Session') MAXPOS(0) +
                          ALLOW(*INTERACT *IPGM *IREXX *EXEC *IMOD) +
                          HLPPNLGRP(QHDBCMD) HLPID(STRSQL)
             PARM       KWD(COMMIT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *CHG *UR *CS *ALL *RS +
                          *NC *RR) PROMPT('Commitment control')
             PARM       KWD(NAMING) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*SYS) VALUES(*SYS *SQL) +
                          PROMPT('Naming convention')
             PARM       KWD(PROCESS) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*RUN) VALUES(*RUN *VLD *SYN) +
                          PROMPT('Statement processing')
             PARM       KWD(LIBOPT) TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL) (*ALL) +
                          (*ALLUSR)) PROMPT('Library option')
             PARM       KWD(LISTTYPE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *SQL) PROMPT('List type')
             PARM       KWD(REFRESH) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*ALWAYS) VALUES(*ALWAYS *FORWARD) +
                          PROMPT('Data refresh')
             PARM       KWD(ALWCPYDTA) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *OPTIMIZE *NO) +
                          PROMPT('Allow copy data')
             PARM       KWD(DATFMT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*JOB) VALUES(*JOB *USA *ISO *EUR *JIS *MDY +
                          *DMY *YMD *JUL) PROMPT('Date format')
             PARM       KWD(DATSEP) TYPE(*CHAR) LEN(1) DFT(*JOB) +
                          SPCVAL((*JOB ' ') (*BLANK ' ')) +
                          PROMPT('Date separator character')
             PARM       KWD(TIMFMT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*HMS) VALUES(*HMS *USA *ISO *EUR *JIS) +
                          PROMPT('Time format')
             PARM       KWD(TIMSEP) TYPE(*CHAR) LEN(1) DFT(*JOB) +
                          SPCVAL((*JOB ' ') (*BLANK ' ')) +
                          PROMPT('Time separator character')
             PARM       KWD(DECPNT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*JOB) VALUES(*JOB *SYSVAL *PERIOD *COMMA) +
                          PROMPT('Decimal point')
             PARM       KWD(PGMLNG) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *C *CBL *PLI *RPG +
                          *FTN) PMTCTL(SY) PROMPT('Program language')
             PARM       KWD(SQLSTRDLM) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*APOSTSQL) VALUES(*APOSTSQL *QUOTESQL) +
                          PMTCTL(SY) PROMPT('SQL string delimiter')
             PARM       KWD(SRTSEQ) TYPE(Q1) DFT(*JOB) SNGVAL((*JOB) +
                          (*JOBRUN) (*LANGIDUNQ) (*LANGIDSHR) (*HEX)) +
                          PROMPT('Sort sequence')
             PARM       KWD(LANGID) TYPE(*NAME) LEN(7) DFT(*JOB) +
                          SPCVAL((*JOB) (*JOBRUN)) +
                          PROMPT('Language identifier')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             SY:        PMTCTL     CTL(PROCESS) COND((*EQ *SYN))
