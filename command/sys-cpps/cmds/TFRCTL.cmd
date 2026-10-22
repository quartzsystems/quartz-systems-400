/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* TFRCTL: end the current program and call another in its place. */
             CMD        PROMPT('Transfer Control') MAXPOS(2) +
                          ALLOW(*BPGM *IPGM) +
                          HLPPNLGRP(QHCLCMD) HLPID(TFRCTL)
             PARM       KWD(PGM) TYPE(Q1) MIN(1) PGM(*YES) +
                          PROMPT('Program')
             PARM       KWD(PARM) TYPE(*VARNAME) LEN(11) MAX(255) +
                          PROMPT('Parameters')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
