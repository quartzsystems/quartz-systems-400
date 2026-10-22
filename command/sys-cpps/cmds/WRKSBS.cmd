/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKSBS: work with the active subsystems. */
             CMD        PROMPT('Work with Subsystems') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(WRKSBS)
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) DFT(*) +
                          VALUES(* *PRINT) PROMPT('Output')
