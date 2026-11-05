/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ENDDBMON: end a database monitor, writing what it holds. */
             CMD        PROMPT('End Database Monitor') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(ENDDBMON)
             PARM       KWD(JOB) TYPE(QJ) DFT(*) SNGVAL((*)) +
                          PROMPT('Job name')
             PARM       KWD(MONID) TYPE(*CHAR) LEN(10) SPCVAL((*ALL)) +
                          PROMPT('Monitor ID')
             PARM       KWD(COMMENT) TYPE(*CHAR) LEN(100) DFT(*BLANK) +
                          SPCVAL((*BLANK)) PROMPT('Monitor comment')
             QJ:        QUAL       TYPE(*GENERIC) LEN(10) SPCVAL((*ALL))
                        QUAL       TYPE(*GENERIC) LEN(10) SPCVAL((*ALL)) +
                          PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) SPCVAL((*ALL)) +
                          PROMPT('Number')
