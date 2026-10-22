/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGCMDDFT: change the default value of parameters of a command. */
             CMD        PROMPT('Change Command Default') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(CHGCMDDFT)
             PARM       KWD(CMD) TYPE(Q1) MIN(1) PROMPT('Command')
             PARM       KWD(NEWDFT) TYPE(*CHAR) LEN(5000) MIN(1) +
                          PROMPT('New default parameter string')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
