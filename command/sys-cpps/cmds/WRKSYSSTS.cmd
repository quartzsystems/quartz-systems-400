/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKSYSSTS: work with the system status and the storage pools. */
             CMD        PROMPT('Work with System Status') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(WRKSYSSTS)
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) DFT(*) +
                          VALUES(* *PRINT) PROMPT('Output')
             PARM       KWD(RESET) TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*NO) +
                          VALUES(*NO *YES) PROMPT('Reset statistics')
             PARM       KWD(ASTLVL) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*USRPRF) VALUES(*USRPRF *BASIC *INTERMED +
                          *ADVANCED) PROMPT('Assistance level')
