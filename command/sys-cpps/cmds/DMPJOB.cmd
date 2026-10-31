/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DMPJOB: the job's call stack, variables and storage to QPSRVDMP. */
             CMD        PROMPT('Dump Job') MAXPOS(3) ALLOW(*ALL) +
                          HLPPNLGRP(QHSXCMD) HLPID(DMPJOB)
             PARM       KWD(PGMSTG) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *NONE) +
                          PROMPT('Program storage')
             PARM       KWD(JOBARA) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO *MIN) +
                          PROMPT('Job areas')
             PARM       KWD(JOBTHDSET) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *THDID) +
                          PROMPT('Job thread set')
