/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRHOSTSVR: start the host server daemons (and the prestart jobs    */
/* come with their subsystems).                                         */
             CMD        PROMPT('Start Host Server') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHZBCMD) HLPID(STRHOSTSVR)
             PARM       KWD(SERVER) TYPE(*CHAR) LEN(10) MIN(1) +
                          RSTD(*YES) +
                          VALUES(*ALL *CENTRAL *DATABASE *DTAQ +
                          *FILE *NETPRT *RMTCMD *SIGNON *SVRMAP) +
                          MAX(9) PROMPT('Server type')
             PARM       KWD(RQDPCL) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*ANY) VALUES(*ANY *TCP *NONE) +
                          PROMPT('Required protocol')
