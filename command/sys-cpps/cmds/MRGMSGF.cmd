/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* MRGMSGF: copy message descriptions from one message file into */
/* another, keeping the replaced ones in a third if asked.         */
             CMD        PROMPT('Merge Message File') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHMHCMD) HLPID(MRGMSGF)
             PARM       KWD(FROMMSGF) TYPE(Q1) MIN(1) +
                          PROMPT('From message file')
             PARM       KWD(TOMSGF) TYPE(Q1) MIN(1) +
                          PROMPT('To message file')
             PARM       KWD(RPLMSGF) TYPE(Q1) DFT(*NONE) +
                          SNGVAL((*NONE)) +
                          PROMPT('Replaced message file')
             PARM       KWD(SELECT) TYPE(*GENERIC) LEN(7) MAX(50) +
                          DFT(*ALL) SNGVAL((*ALL)) +
                          PROMPT('Message identifiers')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
