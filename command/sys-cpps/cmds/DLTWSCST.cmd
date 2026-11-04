/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DLTWSCST: delete a workstation customizing object. */
             CMD        PROMPT('Delete WSCST') MAXPOS(1) ALLOW(*ALL) +
                          HLPPNLGRP(QHUCCMD) HLPID(DLTWSCST)
             PARM       KWD(WSCST) TYPE(Q1) MIN(1) +
                          PROMPT('Workstation customizing object')
             Q1:        QUAL       TYPE(*GENERIC) LEN(10) SPCVAL((*ALL))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB))
