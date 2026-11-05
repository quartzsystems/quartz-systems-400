/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGQRYA: change a job's query attributes. */
             CMD        PROMPT('Change Query Attributes') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(CHGQRYA)
             PARM       KWD(JOB) TYPE(QJ) DFT(*) SNGVAL((*)) +
                          PROMPT('Job name')
             PARM       KWD(QRYTIMLMT) TYPE(*INT4) DFT(*SAME) +
                          RANGE(0 2147352578) SPCVAL((*SAME -2) +
                          (*NOMAX -1) (*SYSVAL -3)) +
                          PROMPT('Query processing time limit')
             PARM       KWD(DEGREE) TYPE(*CHAR) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE) (*IO) (*OPTIMIZE) (*MAX) +
                          (*SYSVAL) (*ANY) (*NBRTASKS)) +
                          PROMPT('Parallel processing degree')
             PARM       KWD(ASYNCJ) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *DIST *LOCAL *ANY *NONE) +
                          PROMPT('Asynchronous job usage')
             PARM       KWD(APYRMT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Apply CHGQRYA to remote')
             PARM       KWD(QRYOPTLIB) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Query options file library')
             PARM       KWD(QRYSTGLMT) TYPE(*INT4) DFT(*SAME) +
                          RANGE(0 2147352578) SPCVAL((*SAME -2) +
                          (*NOMAX -1)) PROMPT('Query storage limit')
             QJ:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) RANGE(000000 999999) +
                          FULL(*YES) PROMPT('Number')
