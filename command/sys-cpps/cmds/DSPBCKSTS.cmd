/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPBCKSTS: the backups that ran. */
             CMD        PROMPT('Display Backup Status') ALLOW(*ALL) +
                          HLPPNLGRP(QHWMCMD) HLPID(DSPBCKSTS)
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
