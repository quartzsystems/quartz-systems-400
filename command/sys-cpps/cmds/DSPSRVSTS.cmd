/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPSRVSTS: whether a job is serviced, traced or debugged. */
             CMD        PROMPT('Display Service Status') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSXCMD) HLPID(DSPSRVSTS)
             PARM       KWD(JOB) TYPE(QJ) DFT(*) PROMPT('Job name')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) DFT(*) +
                          VALUES(* *PRINT) PROMPT('Output')
             QJ:        QUAL       TYPE(*NAME) LEN(10) SPCVAL((*))
                        QUAL       TYPE(*NAME) LEN(10) PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) RANGE(000000 999999) +
                          FULL(*YES) PROMPT('Number')
