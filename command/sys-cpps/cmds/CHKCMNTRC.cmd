/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHKCMNTRC: the status of the traces. */
             CMD        PROMPT('Check Communications Trace') +
                          ALLOW(*ALL) HLPPNLGRP(QHSSCMD) HLPID(CHKCMNTRC)
             PARM       KWD(CFGOBJ) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Configuration object')
             PARM       KWD(CFGTYPE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *LIN *NWI *NWS) +
                          PROMPT('Type')
