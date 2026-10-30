/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSADDAPPID: register an application ID for certificates (Quartz    */
/* extension).                                                        */
             CMD        PROMPT('Add Certificate Application') +
                          MAXPOS(2) ALLOW(*ALL)
             PARM       KWD(APPID) TYPE(*CHAR) LEN(100) MIN(1) +
                          CASE(*MIXED) PROMPT('Application ID')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SERVER) VALUES(*SERVER *CLIENT) +
                          PROMPT('Application type')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) CASE(*MIXED) PROMPT('Text')
