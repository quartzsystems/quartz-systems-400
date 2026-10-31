/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGPRB: change a logged problem. */
             CMD        PROMPT('Change Problem') MAXPOS(1) ALLOW(*ALL) +
                          HLPPNLGRP(QHSXCMD) HLPID(CHGPRB)
             PARM       KWD(PRBID) TYPE(*CHAR) LEN(10) MIN(1) +
                          PROMPT('Problem identifier')
             PARM       KWD(SEV) TYPE(*INT2) DFT(*SAME) RANGE(1 4) +
                          SPCVAL((*SAME 0)) PROMPT('Severity')
             PARM       KWD(ASSIGNTO) TYPE(*CHAR) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Assigned to')
             PARM       KWD(STATUS) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *OPENED *READY +
                          *PREPARED *SENT *ANSWERED *VERIFIED *CLOSED) +
                          PROMPT('Status')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(70) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Note text')
