/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSWRKAPPID: work with the certificate application IDs (Quartz      */
/* extension).                                                        */
             CMD        PROMPT('Work with Cert Applications') +
                          MAXPOS(1) ALLOW(*ALL)
             PARM       KWD(APPID) TYPE(*CHAR) LEN(100) DFT(*ALL) +
                          SPCVAL((*ALL)) CASE(*MIXED) PROMPT('Application ID')
