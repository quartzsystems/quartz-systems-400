/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVRPYLE: remove one entry, or all of them, from the reply list. */
             CMD        PROMPT('Remove Reply List Entry') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHMHCMD) HLPID(RMVRPYLE)
             PARM       KWD(SEQNBR) TYPE(*DEC) LEN(4 0) MIN(1) +
                          RANGE(1 9999) SPCVAL((*ALL 0)) +
                          PROMPT('Sequence number')
