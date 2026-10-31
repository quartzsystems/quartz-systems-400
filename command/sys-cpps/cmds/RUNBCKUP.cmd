/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RUNBCKUP: run a backup with its options. */
             CMD        PROMPT('Run Backup') MAXPOS(1) ALLOW(*ALL) +
                          HLPPNLGRP(QHWMCMD) HLPID(RUNBCKUP)
             PARM       KWD(BCKUPOPT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          MIN(1) VALUES(*DAILY *WEEKLY *MONTHLY) +
                          KEYPARM(*YES) PROMPT('Backup options')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MAX(4) +
                          DFT(*BCKUPOPT) SNGVAL((*BCKUPOPT)) +
                          PROMPT('Device')
