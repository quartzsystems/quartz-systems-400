/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSMNRST: GO RESTORE options 21 to 23. */
             CMD        PROMPT('Restore Menu Option') MAXPOS(1) +
                          ALLOW(*INTERACT *IPGM *IREXX *EXEC) +
                          HLPPNLGRP(QHWMCMD) HLPID(QSMNRST)
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(2) MIN(1) RSTD(*YES) +
                          VALUES('21' '22' '23') +
                          PROMPT('GO RESTORE option')
