/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKOPTVOL: work with the optical volumes the system knows.         */
             CMD        PROMPT('Work with Optical Volumes') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHOPCMD) +
                          HLPID(WRKOPTVOL)
             PARM       KWD(VOL) TYPE(*GENERIC) LEN(32) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Volume')
             PARM       KWD(CSI) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Server')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Device')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
