/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SNDSRVRQS: test the connection to the service provider. */
             CMD        PROMPT('Send Service Request') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSXCMD) HLPID(SNDSRVRQS)
             PARM       KWD(ACTION) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*TEST) VALUES(*TEST) PROMPT('Action')
             PARM       KWD(RMTCPNAME) TYPE(*NAME) LEN(8) DFT(*IBMSRV) +
                          SPCVAL((*IBMSRV)) PROMPT('Remote control point')
             PARM       KWD(RMTNETID) TYPE(*NAME) LEN(8) DFT(*NETATR) +
                          SPCVAL((*NETATR)) PROMPT('Remote network identifier')
             PARM       KWD(CNNTYPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*AUTOMATIC) VALUES(*AUTOMATIC *DIRECT +
                          *IPCNN) PROMPT('Connection type')
