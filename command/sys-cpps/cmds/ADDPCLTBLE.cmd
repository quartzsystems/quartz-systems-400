/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDPCLTBLE: add a protocol to the protocol table. */
             CMD        PROMPT('Add Protocol Table Entry') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHTOCCMD) HLPID(ADDPCLTBLE)
             PARM       KWD(PROTOCOL) TYPE(*CHAR) LEN(32) MIN(1) CASE(*MIXED) +
                          PROMPT('Protocol')
             PARM       KWD(PCLNBR) TYPE(*INT4) RANGE(0 255) MIN(1) +
                          PROMPT('Protocol number')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Text')
             PARM       KWD(ALIASES) TYPE(*CHAR) LEN(32) DFT(*NONE) +
                          SNGVAL((*NONE)) MAX(4) CASE(*MIXED) PROMPT('Aliases')
