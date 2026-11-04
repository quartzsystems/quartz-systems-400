/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRPFRCOL: start Collection Services with a profile. */
             CMD        PROMPT('Start Performance Collection') MAXPOS(0) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(STRPFRCOL)
             PARM       KWD(COLPRF) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*CFG) VALUES(*CFG *MINIMUM *STANDARD +
                          *STANDARDP *ENHCPCPLN *CUSTOM) +
                          CHOICE('*CFG, *MINIMUM, *STANDARD...') +
                          PROMPT('Collection profile')
             PARM       KWD(CYCCOL) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Cycle collection')
