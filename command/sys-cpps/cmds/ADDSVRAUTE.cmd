/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDSVRAUTE: a user and password a profile uses on a remote server. */
             CMD        PROMPT('Add Server Auth Entry') MAXPOS(4) +
                          ALLOW(*ALL) HLPPNLGRP(QHSECCMD) HLPID(ADDSVRAUTE)
             PARM       KWD(USRPRF) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*CURRENT)) PROMPT('User profile')
             PARM       KWD(SERVER) TYPE(*CHAR) LEN(200) MIN(1) +
                          CASE(*MIXED) INLPMTLEN(32) PROMPT('Server')
             PARM       KWD(USRID) TYPE(*CHAR) LEN(1000) DFT(*USRPRF) +
                          SPCVAL((*USRPRF)) CASE(*MIXED) INLPMTLEN(17) +
                          PROMPT('User ID')
             PARM       KWD(PASSWORD) TYPE(*CHAR) LEN(128) DFT(*NONE) +
                          SPCVAL((*NONE)) CASE(*MIXED) INLPMTLEN(17) +
                          DSPINPUT(*NO) PROMPT('User password')
