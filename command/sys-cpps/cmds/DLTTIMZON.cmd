/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DLTTIMZON: delete time zone descriptions from QSYS. */
             CMD        PROMPT('Delete Time Zone Description') +
                          MAXPOS(1) ALLOW(*ALL) HLPPNLGRP(QHWCCMD) +
                          HLPID(DLTTIMZON)
             PARM       KWD(TIMZON) TYPE(*GENERIC) LEN(10) MIN(1) +
                          PROMPT('Time zone description')
