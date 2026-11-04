/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVSCHIDXE: remove a panel group's help topics from a search index. */
             CMD        PROMPT('Remove Search Index Entry') MAXPOS(2) +
                          ALLOW(*ALL) +
                          HLPPNLGRP(QHUICMD) HLPID(RMVSCHIDXE)
             PARM       KWD(SCHIDX) TYPE(Q1) MIN(1) PROMPT('Search index')
             PARM       KWD(PNLGRP) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Panel group')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
