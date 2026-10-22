/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKUSRJOB: work with the jobs of a user. */
             CMD        PROMPT('Work with User Jobs') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(WRKUSRJOB)
             PARM       KWD(USER) TYPE(*NAME) LEN(10) DFT(*) +
                          SPCVAL((*) (*ALL)) PROMPT('User')
             PARM       KWD(STATUS) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *ACTIVE *JOBQ *OUTQ) +
                          PROMPT('Status')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) DFT(*) +
                          VALUES(* *PRINT) PROMPT('Output')
             PARM       KWD(JOBTYPE) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *INTERACT *BATCH) +
                          PROMPT('Job type')
             PARM       KWD(ASTLVL) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*USRPRF) VALUES(*USRPRF *BASIC *INTERMED) +
                          PROMPT('Assistance level')
