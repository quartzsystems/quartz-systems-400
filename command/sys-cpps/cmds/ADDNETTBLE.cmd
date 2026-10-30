/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDNETTBLE: add a network to the network table. */
             CMD        PROMPT('Add Network Table Entry') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHTOCCMD) HLPID(ADDNETTBLE)
             PARM       KWD(NETWORK) TYPE(*CHAR) LEN(255) MIN(1) CASE(*MIXED) +
                          PROMPT('Network')
             PARM       KWD(INTNETADR) TYPE(*CHAR) LEN(15) MIN(1) +
                          PROMPT('Internet address')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Text')
             PARM       KWD(ALIASES) TYPE(*CHAR) LEN(255) DFT(*NONE) +
                          SNGVAL((*NONE)) MAX(4) CASE(*MIXED) PROMPT('Aliases')
