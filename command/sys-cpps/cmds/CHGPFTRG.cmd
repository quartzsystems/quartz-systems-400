/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGPFTRG: enable or disable triggers of a physical file. */
             CMD        PROMPT('Change Physical File Trigger') +
                          MAXPOS(3) ALLOW(*ALL) HLPPNLGRP(QHDBCMD) +
                          HLPID(CHGPFTRG)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('Physical file')
             PARM       KWD(TRG) TYPE(*CHAR) LEN(258) MIN(1) +
                          SPCVAL((*ALL)) PROMPT('Trigger')
             PARM       KWD(TRGLIB) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*CURLIB)) PROMPT('Trigger library')
             PARM       KWD(STATE) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *ENABLED *DISABLED) +
                          PROMPT('Trigger state')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             DEP        CTL((&TRG *NE *ALL)) PARM((&TRGLIB *NE *ALL)) +
                          MSGID(CPD32E9)
