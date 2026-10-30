/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDENVVAR: add an environment variable to the job or the system. */
             CMD        PROMPT('Add Environment Variable') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0ZCMD) HLPID(ADDENVVAR)
             PARM       KWD(ENVVAR) TYPE(*CHAR) LEN(128) MIN(1) +
                          CASE(*MIXED) PROMPT('Environment variable')
             PARM       KWD(VALUE) TYPE(*CHAR) LEN(1024) DFT(*NULL) +
                          SPCVAL((*NULL)) CASE(*MIXED) +
                          PROMPT('Initial value')
             PARM       KWD(CCSID) TYPE(*INT4) DFT(*JOB) RANGE(1 +
                          65535) SPCVAL((*JOB 0) (*HEX 65535)) +
                          EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Coded character set ID')
             PARM       KWD(LEVEL) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*JOB) VALUES(*JOB *SYS) PMTCTL(*PMTRQS) +
                          PROMPT('Level')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Replace existing entry')
