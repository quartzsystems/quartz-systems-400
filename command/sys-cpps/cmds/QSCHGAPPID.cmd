/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSCHGAPPID: change an application ID's client authentication, CA   */
/* trust list or text (Quartz extension).                             */
             CMD        PROMPT('Change Certificate Application') +
                          MAXPOS(1) ALLOW(*ALL)
             PARM       KWD(APPID) TYPE(*CHAR) LEN(100) MIN(1) +
                          CASE(*MIXED) PROMPT('Application ID')
             PARM       KWD(CLTAUTH) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NONE *OPTIONAL +
                          *REQUIRED) PROMPT('Client authentication')
             PARM       KWD(CATRUST) TYPE(*CHAR) LEN(64) DFT(*SAME) +
                          SNGVAL((*SAME) (*ALL)) MAX(50) CASE(*MIXED) +
                          PROMPT('Trusted CA labels')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK)) CASE(*MIXED) PROMPT('Text')
