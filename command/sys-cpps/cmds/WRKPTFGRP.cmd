/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKPTFGRP: the PTF groups on the system, their levels and status. */
             CMD        PROMPT('Work with PTF Groups') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSVCMD) HLPID(WRKPTFGRP)
             PARM       KWD(PTFGRP) TYPE(*GENERIC) LEN(60) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('PTF group')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
