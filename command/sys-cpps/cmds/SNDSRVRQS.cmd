/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SNDSRVRQS: send a request to the update service; *TEST checks the */
/* connection.                                                       */
             CMD        PROMPT('Send Service Request') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSVCMD) HLPID(SNDSRVRQS)
             PARM       KWD(ACTION) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*TEST) VALUES(*TEST *REPORT) +
                          PROMPT('Action')
             PARM       KWD(RMTCPNAME) TYPE(*NAME) LEN(8) +
                          DFT(*IBMSRV) SPCVAL((*IBMSRV)) +
                          PROMPT('Remote control point')
             PARM       KWD(RMTNETID) TYPE(*NAME) LEN(8) +
                          DFT(*NETATR) SPCVAL((*NETATR)) +
                          PROMPT('Remote network identifier')
