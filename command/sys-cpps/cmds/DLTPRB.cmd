/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DLTPRB: delete problems from the problem log. */
             CMD        PROMPT('Delete Problem') MAXPOS(1) ALLOW(*ALL) +
                          HLPPNLGRP(QHSXCMD) HLPID(DLTPRB)
             PARM       KWD(PRBID) TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Problem identifier')
             PARM       KWD(STATUS) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ALL) MAX(7) SNGVAL((*ALL)) VALUES(*OPENED +
                          *READY *PREPARED *SENT *ANSWERED *VERIFIED +
                          *CLOSED) PROMPT('Status')
             PARM       KWD(DAYS) TYPE(*INT2) DFT(30) RANGE(0 999) +
                          PROMPT('Days old')
