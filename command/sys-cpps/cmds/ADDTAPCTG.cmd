/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDTAPCTG: move cartridges of a media library out of *INSERT into */
/* a usable category.                                                 */
             CMD        PROMPT('Add Tape Cartridge') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHSVCMD) +
                          HLPID(ADDTAPCTG)
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Library device')
             PARM       KWD(CTG) TYPE(*CHAR) LEN(6) MIN(1) MAX(40) +
                          SPCVAL((*INSERT)) PROMPT('Cartridge ID')
             PARM       KWD(CGY) TYPE(E1) PROMPT('Category')
             PARM       KWD(CHECK) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Check volume identifier')
             E1:        ELEM       TYPE(*CHAR) LEN(10) DFT(*SHARE400) +
                          SPCVAL((*SHARE400) (*NOSHARE) (*IPL) (*NL) +
                          (*CNV)) PROMPT('Category name')
                        ELEM       TYPE(*CHAR) LEN(8) DFT(*CURRENT) +
                          SPCVAL((*CURRENT)) PROMPT('Category system')
