/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ENDHOSTSVR: end the host server daemons.                              */
             CMD        PROMPT('End Host Server') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHZBCMD) HLPID(ENDHOSTSVR)
             PARM       KWD(SERVER) TYPE(*CHAR) LEN(10) MIN(1) +
                          RSTD(*YES) +
                          VALUES(*ALL *CENTRAL *DATABASE *DTAQ +
                          *FILE *NETPRT *RMTCMD *SIGNON *SVRMAP) +
                          MAX(9) PROMPT('Server type')
             PARM       KWD(ENDACTCNN) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *DATABASE *FILE) +
                          MAX(2) PROMPT('End active connections')
