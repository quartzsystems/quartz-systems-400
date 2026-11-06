/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGFCNUSG: a function's default, its *ALLOBJ use, a profile's usage. */
             CMD        PROMPT('Change Function Usage') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(CHGFCNUSG)
             PARM       KWD(FCNID) TYPE(*SNAME) LEN(30) MIN(1) +
                          EXPR(*YES) PROMPT('Function ID')
             PARM       KWD(USER) TYPE(*SNAME) LEN(10) EXPR(*YES) +
                          PROMPT('User')
             PARM       KWD(USAGE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          VALUES(*ALLOWED *DENIED *NONE) +
                          PROMPT('Usage')
             PARM       KWD(DEFAULT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *ALLOWED *DENIED) +
                          PROMPT('Default authority')
             PARM       KWD(ALLOBJAUT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *USED *NOTUSED) +
                          PROMPT('*ALLOBJ special authority')
             DEP        CTL(&USER) PARM(&USAGE) NBRTRUE(*EQ 1) +
                          MSGID(CPD22B5)
             DEP        CTL(&USAGE) PARM(&USER) NBRTRUE(*EQ 1) +
                          MSGID(CPD22B5)
