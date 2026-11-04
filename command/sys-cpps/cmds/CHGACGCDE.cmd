/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGACGCDE: change the accounting code of a job; with QACGLVL(*JOB)  */
/* the resources used so far are journaled under the old code.          */
             CMD        PROMPT('Change Accounting Code') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(CHGACGCDE)
             PARM       KWD(JOB) TYPE(QJ) DFT(*) PROMPT('Job name')
             PARM       KWD(ACGCDE) TYPE(*CHAR) LEN(15) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) CHOICE(*NONE) +
                          PROMPT('Accounting code')
             QJ:        QUAL       TYPE(*NAME) LEN(10) SPCVAL((*))
                        QUAL       TYPE(*NAME) LEN(10) PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) RANGE(000000 999999) +
                          FULL(*YES) CHOICE('000000-999999') +
                          PROMPT('Number')
