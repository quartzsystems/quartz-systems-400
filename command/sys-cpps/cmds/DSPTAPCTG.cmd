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
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT *OUTFILE) PROMPT('Output')
             PARM       KWD(OUTFILE) TYPE(Q1) FILE(*OUT) PMTCTL(PO) +
                          PROMPT('File to receive output')
             PARM       KWD(OUTMBR) TYPE(EO) PMTCTL(PO) +
                          PROMPT('Output member options')
             PO:        PMTCTL     CTL(OUTPUT) COND((*EQ *OUTFILE))
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             EO:        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Member to receive output')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD) +
                          PROMPT('Replace or add records')
             E1:        ELEM       TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*SHARE400) (*NOSHARE) +
                          (*INSERT) (*EJECT) (*CNV) (*IPL) (*NL) +
                          (*SYSGEN)) PROMPT('Category name')
                        ELEM       TYPE(*CHAR) LEN(8) DFT(*CURRENT) +
                          SPCVAL((*CURRENT)) PROMPT('Category system')
