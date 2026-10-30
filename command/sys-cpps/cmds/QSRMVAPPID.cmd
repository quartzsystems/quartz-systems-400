/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSRMVAPPID: remove an application ID a user registered (Quartz     */
/* extension).                                                        */
             CMD        PROMPT('Remove Certificate Application') +
                          MAXPOS(1) ALLOW(*ALL)
             PARM       KWD(APPID) TYPE(*CHAR) LEN(100) MIN(1) +
                          CASE(*MIXED) PROMPT('Application ID')
