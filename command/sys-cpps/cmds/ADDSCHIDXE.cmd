/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDSCHIDXE: add a panel group's help topics to a search index. */
             CMD        PROMPT('Add Search Index Entry') MAXPOS(2) +
                          ALLOW(*ALL)
             PARM       KWD(SCHIDX) TYPE(Q1) MIN(1) PROMPT('Search index')
             PARM       KWD(PNLGRP) TYPE(Q2) MIN(1) PROMPT('Panel group')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
