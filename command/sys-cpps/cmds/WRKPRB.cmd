/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKPRB: list the problems the system logged (none are logged yet). */
             CMD        PROMPT('Work with Problems') MAXPOS(1) +
                          ALLOW(*INTERACT *IPGM *IREXX *EXEC *IMOD) +
                          HLPPNLGRP(QHSXCMD) HLPID(WRKPRB)
             PARM       KWD(PRBID) TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Problem identifier')
             PARM       KWD(STATUS) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *OPENED *READY *PREPARED +
                          *SENT *ANSWERED *VERIFIED *CLOSED) +
                          PROMPT('Status')
