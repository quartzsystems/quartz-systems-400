/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSBCKUP: RUNBCKUP's steps from the service, and their outcome back. */
             CMD        PROMPT('Backup Plan') MAXPOS(1) ALLOW(*ALL) +
                          HLPPNLGRP(QHWMCMD) HLPID(QSBCKUP)
             PARM       KWD(ACTION) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          MIN(1) VALUES(*PLAN *DONE *DFT *SETDFT *FIELDS) +
                          PROMPT('Action')
             PARM       KWD(BCKUPOPT) TYPE(*CHAR) LEN(8) DFT(*NONE) +
                          PROMPT('Backup options')
             PARM       KWD(DEV) TYPE(*CHAR) LEN(10) DFT(*BCKUPOPT) +
                          PROMPT('Device')
             PARM       KWD(DATA) TYPE(*CHAR) LEN(2000) DFT(*NONE) +
                          PROMPT('Data')
