/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVMSGD: remove a message description from a message file. */
             CMD        PROMPT('Remove Message Description') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHMHCMD) HLPID(RMVMSGD)
             PARM       KWD(MSGID) TYPE(*NAME) LEN(7) MIN(1) +
                          PROMPT('Message identifier')
             PARM       KWD(MSGF) TYPE(Q1) MIN(1) PROMPT('Message file')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
