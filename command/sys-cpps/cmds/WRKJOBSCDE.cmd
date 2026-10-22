/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKJOBSCDE: work with the entries of the job schedule. */
             CMD        PROMPT('Work with Job Schedule Entries') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(WRKJOBSCDE)
             PARM       KWD(JOB) TYPE(*GENERIC) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Job name')
             PARM       KWD(ENTRYNBR) TYPE(*INT4) DFT(*ALL) +
                          RANGE(1 999999) SPCVAL((*ALL -1)) +
                          PROMPT('Entry number')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) DFT(*) +
                          VALUES(* *PRINT) PROMPT('Output')
             PARM       KWD(PRTFMT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*BASIC) VALUES(*BASIC *FULL) +
                          PROMPT('Print format')
