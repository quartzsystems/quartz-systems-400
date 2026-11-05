/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVPFTRG: remove triggers from a physical file. */
             CMD        PROMPT('Remove Physical File Trigger') +
                          MAXPOS(3) ALLOW(*ALL) HLPPNLGRP(QHDBCMD) +
                          HLPID(RMVPFTRG)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('Physical file')
             PARM       KWD(TRGTIME) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *BEFORE *AFTER) +
                          PROMPT('Trigger time')
             PARM       KWD(TRGEVENT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *INSERT *DELETE *UPDATE +
                          *READ) PROMPT('Trigger event')
             PARM       KWD(TRG) TYPE(*CHAR) LEN(258) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Trigger')
             PARM       KWD(TRGLIB) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*CURLIB)) PROMPT('Trigger library')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
