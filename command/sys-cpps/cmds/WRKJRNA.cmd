/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKJRNA: work with a journal's attributes, receivers and objects. */
             CMD        PROMPT('Work with Journal Attributes') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHJRCMD) HLPID(WRKJRNA)
             PARM       KWD(JRN) TYPE(Q1) MIN(1) PROMPT('Journal')
             PARM       KWD(DETAIL) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*CURATR) VALUES(*CURATR *RCVDIR *JRNFILE +
                          *JRNAP *JRNDTAARA *JRNDTAQ *JRNLIB *JRNIFS +
                          *JRNOBJ *RMTJRN) MAX(10) SNGVAL((*ALL)) +
                          PROMPT('Detail')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
