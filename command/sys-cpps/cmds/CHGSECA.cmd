/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGSECA: where generated user and group ID numbers start. */
             CMD        PROMPT('Change Security Attributes') MAXPOS(0) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(CHGSECA)
             PARM       KWD(UID) TYPE(*DEC) LEN(10 0) DFT(*SAME) +
                          RANGE(101 4294967294) SPCVAL((*SAME 0)) +
                          EXPR(*YES) PROMPT('User ID number')
             PARM       KWD(GID) TYPE(*DEC) LEN(10 0) DFT(*SAME) +
                          RANGE(101 4294967294) SPCVAL((*SAME 0)) +
                          EXPR(*YES) PROMPT('Group ID number')
