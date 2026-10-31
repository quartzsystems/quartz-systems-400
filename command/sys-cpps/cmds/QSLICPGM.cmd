/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSLICPGM: one GO LICPGM panel (Quartz's; the menu's options). */
             CMD        PROMPT('Licensed Program Panel') MAXPOS(1) +
                          ALLOW(*INTERACT *IPGM *IREXX *EXEC) +
                          HLPPNLGRP(QHSZCMD) HLPID(QSLICPGM)
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(2) MIN(1) RSTD(*YES) +
                          VALUES('1' '10' '11' '12' '13' '20' '21' '22') +
                          PROMPT('GO LICPGM option')
