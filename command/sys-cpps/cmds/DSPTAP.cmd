/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPTAP: display the labels, or the saves, on a tape volume.       */
             CMD        PROMPT('Display Tape') MAXPOS(2) ALLOW(*ALL) +
                          HLPPNLGRP(QHSVCMD) HLPID(DSPTAP)
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Device')
             PARM       KWD(VOL) TYPE(*CHAR) LEN(6) DFT(*MOUNTED) +
                          SPCVAL((*MOUNTED)) PROMPT('Volume identifier')
             PARM       KWD(LABEL) TYPE(*CHAR) LEN(17) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('File label')
             PARM       KWD(SEQNBR) TYPE(E1) PROMPT('Sequence number')
             PARM       KWD(DATA) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*LABELS) VALUES(*LABELS *SAVRST *VOLLBL) +
                          PROMPT('Data type')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT *OUTFILE) PROMPT('Output')
             PARM       KWD(ENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*REWIND) VALUES(*REWIND *LEAVE *UNLOAD) +
                          PROMPT('End of tape option')
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
             E1:        ELEM       TYPE(*INT4) DFT(*FIRST) +
                          RANGE(1 16777215) SPCVAL((*FIRST 1)) +
                          PROMPT('Starting file sequence number')
                        ELEM       TYPE(*INT4) DFT(*LAST) +
                          RANGE(1 16777215) SPCVAL((*LAST 16777215) +
                          (*ONLY 0)) PROMPT('Ending file sequence number')
