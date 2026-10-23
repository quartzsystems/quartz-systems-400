/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRSRVJOB: service another job: debug its programs from this one. */
             CMD        PROMPT('Start Service Job') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBGCMD) HLPID(STRSRVJOB)
             PARM       KWD(JOB) TYPE(QJ) MIN(1) PROMPT('Job name')
             PARM       KWD(DUPJOBOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SELECT) VALUES(*SELECT *MSG) +
                          PROMPT('Duplicate job option')
             QJ:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) RANGE(000000 999999) +
                          FULL(*YES) PROMPT('Number')
