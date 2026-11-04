/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRSCHIDX: search a help index for topics and show their help. */
             CMD        PROMPT('Start Search Index') MAXPOS(1) +
                          ALLOW(*INTERACT *IPGM *IREXX *IMOD) +
                          HLPPNLGRP(QHUICMD) HLPID(STRSCHIDX)
             PARM       KWD(SCHIDX) TYPE(Q1) DFT(*USER) SNGVAL((*USER)) +
                          PROMPT('Search index')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL)) PROMPT('Library')
