/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPJRNRCVA: display a journal receiver's attributes. */
             CMD        PROMPT('Display Journal Receiver Attr') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHJRCMD) HLPID(DSPJRNRCVA)
             PARM       KWD(JRNRCV) TYPE(Q1) MIN(1) +
                          PROMPT('Journal receiver')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
