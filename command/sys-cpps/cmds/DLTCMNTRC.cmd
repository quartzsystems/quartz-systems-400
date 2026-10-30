/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DLTCMNTRC: delete an ended trace and its data. */
             CMD        PROMPT('Delete Communications Trace') +
                          ALLOW(*ALL) HLPPNLGRP(QHSSCMD) HLPID(DLTCMNTRC)
             PARM       KWD(CFGOBJ) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Configuration object')
             PARM       KWD(CFGTYPE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*LIN *NWI *NWS) MIN(1) PROMPT('Type')
