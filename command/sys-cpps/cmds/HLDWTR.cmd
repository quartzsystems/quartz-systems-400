/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* HLDWTR: hold a spooling writer. */
             CMD        PROMPT('Hold Writer') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSPCMD) HLPID(HLDWTR)
             PARM       KWD(WTR) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Writer')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*CNTRLD) VALUES(*CNTRLD *IMMED *PAGEEND) +
                          PROMPT('When to hold writer')
