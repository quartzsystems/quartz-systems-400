/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGSVRAUTE: change the user or password of a server entry.  */
             CMD        PROMPT('Change Server Auth Entry') MAXPOS(4) +
                          ALLOW(*ALL) HLPPNLGRP(QHSECCMD) HLPID(CHGSVRAUTE)
             PARM       KWD(USRPRF) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*CURRENT)) PROMPT('User profile')
             PARM       KWD(SERVER) TYPE(*CHAR) LEN(200) MIN(1) +
                          CASE(*MIXED) INLPMTLEN(32) PROMPT('Server')
             PARM       KWD(USRID) TYPE(*CHAR) LEN(1000) DFT(*SAME) +
                          SPCVAL((*SAME) (*USRPRF)) CASE(*MIXED) INLPMTLEN(17) +
                          PROMPT('User ID')
             PARM       KWD(PASSWORD) TYPE(*CHAR) LEN(128) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) CASE(*MIXED) INLPMTLEN(17) +
                          DSPINPUT(*NO) PROMPT('User password')
