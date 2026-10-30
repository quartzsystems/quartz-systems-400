/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDTCPHTE: add an address and its names to the host table. */
             CMD        PROMPT('Add TCP/IP Host Table Entry') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHTOCCMD) +
                          HLPID(ADDTCPHTE)
             PARM       KWD(INTNETADR) TYPE(*CHAR) LEN(45) MIN(1) +
                          PROMPT('Internet address')
             PARM       KWD(HOSTNAME) TYPE(E1) MIN(1) MAX(65) +
                          PROMPT('Host names')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(64) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Text')
             E1:        ELEM       TYPE(*CHAR) LEN(255) MIN(1) +
                          PROMPT('Name')
