/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DLYJOB: delay the job for a number of seconds or until a time. */
             CMD        PROMPT('Delay Job') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(DLYJOB)
             PARM       KWD(DLY) TYPE(*INT4) RANGE(0 999999) +
                          PROMPT('Delay time, seconds')
             PARM       KWD(RSMTIME) TYPE(*TIME) PROMPT('Resume time')
             DEP        CTL(*ALWAYS) PARM(&DLY &RSMTIME) +
                          NBRTRUE((*EQ 1)) MSGID(CPD0908)
