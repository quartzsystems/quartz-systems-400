/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPTAPCTG: display the tape cartridges of a media library.         */
             CMD        PROMPT('Display Tape Cartridge') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHSVCMD) +
                          HLPID(DSPTAPCTG)
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Library device')
             PARM       KWD(CTG) TYPE(*GENERIC) LEN(6) DFT(*ALL) +
                          SPCVAL((*ALL)) MAX(40) PROMPT('Cartridge ID')
             PARM       KWD(CGY) TYPE(E1) PROMPT('Category')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             E1:        ELEM       TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*SHARE400) (*NOSHARE) +
                          (*INSERT) (*EJECT) (*CNV) (*IPL) (*NL) +
                          (*SYSGEN)) PROMPT('Category name')
                        ELEM       TYPE(*CHAR) LEN(8) DFT(*CURRENT) +
                          SPCVAL((*CURRENT)) PROMPT('Category system')
