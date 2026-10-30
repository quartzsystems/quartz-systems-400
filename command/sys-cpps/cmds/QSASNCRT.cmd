/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSASNCRT: assign a certificate of *SYSTEM to an application ID     */
/* (Quartz extension).                                                */
             CMD        PROMPT('Assign Certificate') MAXPOS(2) +
                          ALLOW(*ALL)
             PARM       KWD(APPID) TYPE(*CHAR) LEN(100) MIN(1) +
                          CASE(*MIXED) PROMPT('Application ID')
             PARM       KWD(LABEL) TYPE(*CHAR) LEN(64) MIN(1) +
                          SPCVAL((*NONE)) CASE(*MIXED) +
                          PROMPT('Certificate label')
