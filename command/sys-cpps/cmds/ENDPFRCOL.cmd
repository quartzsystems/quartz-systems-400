/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ENDPFRCOL: end Collection Services. */
             CMD        PROMPT('End Performance Collection') MAXPOS(0) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(ENDPFRCOL)
             PARM       KWD(FRCCOLEND) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PROMPT('Force end')
