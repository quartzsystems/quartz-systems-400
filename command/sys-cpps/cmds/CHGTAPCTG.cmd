/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGTAPCTG: move cartridges of a media library to another category. */
             CMD        PROMPT('Change Tape Cartridge') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHSVCMD) +
                          HLPID(CHGTAPCTG)
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Library device')
             PARM       KWD(CTG) TYPE(*CHAR) LEN(6) MIN(1) MAX(40) +
                          PROMPT('Cartridge ID')
             PARM       KWD(CGY) TYPE(E1) PROMPT('Category')
             E1:        ELEM       TYPE(*CHAR) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*SHARE400) (*NOSHARE) +
                          (*IPL) (*NL) (*CNV)) PROMPT('Category name')
                        ELEM       TYPE(*CHAR) LEN(8) DFT(*CURRENT) +
                          SPCVAL((*CURRENT)) PROMPT('Category system')
