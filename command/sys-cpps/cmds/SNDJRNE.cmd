/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SNDJRNE: write a user entry to a journal. */
             CMD        PROMPT('Send Journal Entry') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHJRCMD) HLPID(SNDJRNE)
             PARM       KWD(JRN) TYPE(Q1) MIN(1) PROMPT('Journal')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(2) MIN(1) +
                          PROMPT('Entry type')
             PARM       KWD(FILE) TYPE(E1) DFT(*NONE) SNGVAL((*NONE)) +
                          PROMPT('Journaled file')
             PARM       KWD(ENTDTA) TYPE(*CHAR) LEN(3000) DFT(' ') +
                          PROMPT('Entry data')
             PARM       KWD(FORCE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Force to auxiliary storage')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(Q1) MIN(1) PROMPT('File')
                        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Member')
