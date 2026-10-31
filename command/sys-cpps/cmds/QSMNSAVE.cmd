/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSMNSAVE: GO SAVE options 20 to 23. */
             CMD        PROMPT('Save Menu Option') MAXPOS(1) +
                          ALLOW(*INTERACT *IPGM *IREXX *EXEC) +
                          HLPPNLGRP(QHWMCMD) HLPID(QSMNSAVE)
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(2) MIN(1) RSTD(*YES) +
                          VALUES('20' '21' '22' '23') +
                          PROMPT('GO SAVE option')
