/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSCHGUPDA: where the update service is and when to check it. */
             CMD        PROMPT('Change Update Attributes') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSVCMD) HLPID(QSCHGUPDA)
             PARM       KWD(URL) TYPE(*CHAR) LEN(256) DFT(*SAME) +
                          SPCVAL((*SAME) (*DFT)) CASE(*MIXED) +
                          PROMPT('Update service URL')
             PARM       KWD(PROXY) TYPE(*CHAR) LEN(256) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) CASE(*MIXED) +
                          PROMPT('Proxy host and port')
             PARM       KWD(SCHEDULE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NONE *DAILY *WEEKLY) +
                          PROMPT('Check for updates')
             PARM       KWD(SCDTIME) TYPE(*TIME) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Time of the check')
