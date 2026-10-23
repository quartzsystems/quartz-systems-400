/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPWSUSR: the display station and the jobs signed on at it. */
             CMD        PROMPT('Display Work Station User') +
                          ALLOW(*INTERACT *IPGM *IMOD *IREXX *EXEC) +
                          HLPPNLGRP(QHWCCMD) HLPID(DSPWSUSR)
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
