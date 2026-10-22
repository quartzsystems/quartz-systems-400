/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPDTAARA: show a data area's attributes and value. */
             CMD        PROMPT('Display Data Area') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHLICMD) HLPID(DSPDTAARA)
             PARM       KWD(DTAARA) TYPE(Q1) MIN(1) PROMPT('Data area')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             PARM       KWD(OUTFMT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*CHAR) VALUES(*CHAR *HEX) +
                          PROMPT('Output format')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*LDA) (*GDA) (*PDA))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
