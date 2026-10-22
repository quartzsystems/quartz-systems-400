/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RLSWTR: release a held spooling writer. */
             CMD        PROMPT('Release Writer') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSPCMD) HLPID(RLSWTR)
             PARM       KWD(WTR) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Writer')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*CURRENT) VALUES(*CURRENT *BEGIN *BYPASS) +
                          PROMPT('Resume printing at')
