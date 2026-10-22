/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* HLDJOB: hold a job, and its spooled files if asked. */
             CMD        PROMPT('Hold Job') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(HLDJOB)
             PARM       KWD(JOB) TYPE(QJ) DFT(*) PROMPT('Job name')
             PARM       KWD(SPLFILE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Hold spooled files')
             PARM       KWD(DUPJOBOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SELECT) VALUES(*SELECT *MSG) +
                          PROMPT('Duplicate job option')
             QJ:        QUAL       TYPE(*NAME) LEN(10) SPCVAL((*))
                        QUAL       TYPE(*NAME) LEN(10) PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) RANGE(000000 999999) +
                          FULL(*YES) PROMPT('Number')
