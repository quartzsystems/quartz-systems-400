/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGTCPHTE: change the names or text of an address. */
             CMD        PROMPT('Change TCP/IP Host Table Entry') +
                          MAXPOS(1) ALLOW(*ALL) HLPPNLGRP(QHTOCCMD) +
                          HLPID(CHGTCPHTE)
             PARM       KWD(INTNETADR) TYPE(*CHAR) LEN(45) MIN(1) +
                          PROMPT('Internet address')
             PARM       KWD(HOSTNAME) TYPE(E1) MAX(65) +
                          PROMPT('Host names')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(64) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) PROMPT('Text')
             E1:        ELEM       TYPE(*CHAR) LEN(255) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK)) PROMPT('Name')
