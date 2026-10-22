/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ENDJOB: end a job, controlled or at once. */
             CMD        PROMPT('End Job') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(ENDJOB)
             PARM       KWD(JOB) TYPE(QJ) MIN(1) PROMPT('Job name')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*CNTRLD) VALUES(*CNTRLD *IMMED) +
                          PROMPT('How to end')
             PARM       KWD(DELAY) TYPE(*INT4) DFT(30) RANGE(1 999999) +
                          PROMPT('Controlled end delay time')
             PARM       KWD(SPLFILE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Delete spooled files')
             PARM       KWD(LOGLMT) TYPE(*INT4) DFT(*SAME) +
                          RANGE(0 999999) SPCVAL((*SAME -1) (*NOMAX -2)) +
                          PROMPT('Maximum log entries')
             PARM       KWD(ADLINTJOBS) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *GRPJOB *ALL) +
                          PROMPT('Additional interactive jobs')
             PARM       KWD(DUPJOBOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SELECT) VALUES(*SELECT *MSG) +
                          PROMPT('Duplicate job option')
             QJ:        QUAL       TYPE(*NAME) LEN(10) MIN(1) SPCVAL((*))
                        QUAL       TYPE(*NAME) LEN(10) PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) RANGE(000000 999999) +
                          FULL(*YES) PROMPT('Number')
