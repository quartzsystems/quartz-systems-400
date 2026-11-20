/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVQSICSL: remove a management console's pin (docs/QSID.md section   */
/* 12): by its name, its fingerprint or its place in WRKQSICSL's list.  */
/* A security officer's: *ALLOBJ and *SECADM. QS/400's.                 */
             CMD        PROMPT('Remove QSI Console') MAXPOS(1) +
                          HLPPNLGRP(QHTOCCMD) HLPID(RMVQSICSL) +
                          ALLOW(*ALL)
             PARM       KWD(CONSOLE) TYPE(*CHAR) LEN(64) MIN(1) +
                          CASE(*MIXED) PROMPT('Console')
