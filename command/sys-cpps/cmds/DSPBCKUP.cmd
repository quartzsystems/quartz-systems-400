/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPBCKUP: show a backup's options. */
             CMD        PROMPT('Display Backup Options') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(DSPBCKUP)
             PARM       KWD(BCKUPOPT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          MIN(1) VALUES(*DAILY *WEEKLY *MONTHLY) +
                          PROMPT('Backup options')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
