/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVEXITPGM: take a program off an exit point and format. */
             CMD        PROMPT('Remove Exit Program') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHUSCMD) HLPID(RMVEXITPGM)
             PARM       KWD(EXITPNT) TYPE(*CHAR) LEN(20) MIN(1) +
                          PROMPT('Exit point')
             PARM       KWD(FORMAT) TYPE(*NAME) LEN(8) MIN(1) +
                          PROMPT('Exit point format')
             PARM       KWD(PGMNBR) TYPE(*INT4) MIN(1) +
                          RANGE(1 2147483647) PROMPT('Program number')
