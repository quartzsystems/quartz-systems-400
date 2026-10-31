/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRCLNUP: schedule the daily cleanup, or run it now. */
             CMD        PROMPT('Start Cleanup') MAXPOS(1) ALLOW(*ALL) +
                          HLPPNLGRP(QHWMCMD) HLPID(STRCLNUP)
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*SCHED) VALUES(*SCHED *IMMED) +
                          PROMPT('Option')
