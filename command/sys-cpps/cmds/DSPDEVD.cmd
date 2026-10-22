/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPDEVD: show a device description. */
             CMD        PROMPT('Display Device Description') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHCFCMD) HLPID(DSPDEVD)
             PARM       KWD(DEVD) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Device description')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *BASIC *MODE) +
                          PROMPT('Option')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
