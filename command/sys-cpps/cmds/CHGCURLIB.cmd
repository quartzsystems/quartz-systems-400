/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGCURLIB: change the current library of the job. */
             CMD        PROMPT('Change Current Library') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(CHGCURLIB)
             PARM       KWD(CURLIB) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*CRTDFT)) PROMPT('Current library')
