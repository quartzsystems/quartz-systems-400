/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKJOB: work with one job, the current one by default. */
             CMD        PROMPT('Work with Job') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(WRKJOB)
             PARM       KWD(JOB) TYPE(QJ) DFT(*) PROMPT('Job name')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) DFT(*) +
                          VALUES(* *PRINT) PROMPT('Output')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SELECT) VALUES(*SELECT *ALL *STSA *DFNA +
                          *RUNA *SPLF *JOBLOG *PGMSTK *JOBLCK *LIBL +
                          *OPNF *FILOVR *CMTCTL *CMNSTS *ACTGRP *MUTEX +
                          *THREAD *MLBA) PROMPT('Option')
             PARM       KWD(DUPJOBOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SELECT) VALUES(*SELECT *MSG) +
                          PROMPT('Duplicate job option')
             QJ:        QUAL       TYPE(*NAME) LEN(10) SPCVAL((*))
                        QUAL       TYPE(*NAME) LEN(10) PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) RANGE(000000 999999) +
                          FULL(*YES) PROMPT('Number')
