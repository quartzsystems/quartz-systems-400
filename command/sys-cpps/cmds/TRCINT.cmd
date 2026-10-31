/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* TRCINT: the storage service's internal trace points. */
             CMD        PROMPT('Trace Internal') MAXPOS(2) ALLOW(*ALL) +
                          HLPPNLGRP(QHSXCMD) HLPID(TRCINT)
             PARM       KWD(SET) TYPE(*CHAR) LEN(5) RSTD(*YES) DFT(*ON) +
                          VALUES(*ON *OFF *END *HOLD) +
                          PROMPT('Trace option setting')
             PARM       KWD(TRCTBL) TYPE(*NAME) LEN(10) DFT(QSTRCTBL) +
                          PROMPT('Trace table')
             PARM       KWD(SIZE) TYPE(*INT4) DFT(1024) RANGE(1 512000) +
                          PROMPT('Trace table size')
             PARM       KWD(TRCFULL) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*WRAP) VALUES(*WRAP *STOPTRC) +
                          PROMPT('Trace full')
             PARM       KWD(TRCTYPE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*ALL) MAX(9) SNGVAL((*ALL)) VALUES(*CALL +
                          *CMD *REQ *MSG *IO *JOB *PRB *DMP *LOG) +
                          PROMPT('Trace type')
             PARM       KWD(JOBNBR) TYPE(*CHAR) LEN(6) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Job number')
