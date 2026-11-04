/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPHLPDOC: show help by name: DOC is the panel group, FLR its library */
/* (blank for the library list) and HLPLBL the help module.              */
             CMD        PROMPT('Display Help Document') MAXPOS(3) +
                          ALLOW(*INTERACT *IPGM *IREXX *IMOD) +
                          HLPPNLGRP(QHUICMD) HLPID(DSPHLPDOC)
             PARM       KWD(DOC) TYPE(*NAME) LEN(12) MIN(1) +
                          PROMPT('Document')
             PARM       KWD(FLR) TYPE(*CHAR) LEN(63) CHOICE(*NONE) +
                          PROMPT('Folder')
             PARM       KWD(HLPLBL) TYPE(*CHAR) LEN(32) DFT(*FIRST) +
                          SPCVAL((*FIRST)) INLPMTLEN(12) +
                          CHOICE('Character value, *FIRST') +
                          PROMPT('Help text label')
