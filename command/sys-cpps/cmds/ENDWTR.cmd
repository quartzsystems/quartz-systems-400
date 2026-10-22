/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ENDWTR: end a spooling writer. */
             CMD        PROMPT('End Writer') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSPCMD) HLPID(ENDWTR)
             PARM       KWD(WTR) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*ALL) (*SYSVAL)) PROMPT('Writer')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*CNTRLD) VALUES(*CNTRLD *IMMED *PAGEEND) +
                          PROMPT('When to end writer')
