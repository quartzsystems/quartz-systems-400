/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDPFTRG: add a trigger program to a physical file. */
             CMD        PROMPT('Add Physical File Trigger') +
                          MAXPOS(3) ALLOW(*ALL) HLPPNLGRP(QHDBCMD) +
                          HLPID(ADDPFTRG)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('Physical file')
             PARM       KWD(TRGTIME) TYPE(*CHAR) LEN(7) RSTD(*YES) MIN(1) +
                          VALUES(*BEFORE *AFTER) PROMPT('Trigger time')
             PARM       KWD(TRGEVENT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          MIN(1) VALUES(*INSERT *DELETE *UPDATE *READ) +
                          PROMPT('Trigger event')
             PARM       KWD(PGM) TYPE(Q1) MIN(1) PROMPT('Program')
             PARM       KWD(RPLTRG) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Replace trigger')
             PARM       KWD(TRG) TYPE(*CHAR) LEN(258) DFT(*GEN) +
                          SPCVAL((*GEN)) PROMPT('Trigger')
             PARM       KWD(TRGLIB) TYPE(*NAME) LEN(10) DFT(*FILE) +
                          SPCVAL((*FILE) (*CURLIB)) PROMPT('Trigger library')
             PARM       KWD(ALWREPCHG) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Allow repeated change')
             PARM       KWD(TRGUPDCND) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ALWAYS) VALUES(*ALWAYS *CHANGE) +
                          PROMPT('Trigger update condition')
             PARM       KWD(THDSAFE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*UNKNOWN) VALUES(*UNKNOWN *NO *YES) +
                          PROMPT('Threadsafe')
             PARM       KWD(MLTTHDACN) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SYSVAL) VALUES(*SYSVAL *MSG *NORUN *RUN) +
                          PROMPT('Multithreaded job action')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             DEP        CTL((&TRGTIME *EQ *BEFORE)) +
                          PARM((&TRGEVENT *NE *READ)) MSGID(CPD730A)
             DEP        CTL((&TRGEVENT *NE *UPDATE)) +
                          PARM((&TRGUPDCND *EQ *ALWAYS)) MSGID(CPD7308)
