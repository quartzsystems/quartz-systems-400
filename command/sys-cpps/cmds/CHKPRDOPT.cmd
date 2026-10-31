/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHKPRDOPT: check that a product's objects are all there. */
             CMD        PROMPT('Check Product Option') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHSZCMD) HLPID(CHKPRDOPT)
             PARM       KWD(PRDID) TYPE(*CHAR) LEN(7) MIN(1) +
                          PROMPT('Product ID')
             PARM       KWD(RLS) TYPE(*CHAR) LEN(6) DFT(*ONLY) +
                          SPCVAL((*ONLY)) PROMPT('Release')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(5) DFT(*ALL) +
                          SPCVAL((*ALL) (*BASE)) PROMPT('Product option')
             PARM       KWD(DETAIL) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*BASIC) VALUES(*BASIC *FULL) +
                          PROMPT('Detail')
