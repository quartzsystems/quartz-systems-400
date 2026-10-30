/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ENDCMNTRC: stop a trace, keeping its data. */
             CMD        PROMPT('End Communications Trace') +
                          ALLOW(*ALL) HLPPNLGRP(QHSSCMD) HLPID(ENDCMNTRC)
             PARM       KWD(CFGOBJ) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Configuration object')
             PARM       KWD(CFGTYPE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*LIN *NWI *NWS) MIN(1) PROMPT('Type')
