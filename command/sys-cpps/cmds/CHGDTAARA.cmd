/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGDTAARA: change all or part of a data area's value. */
             CMD        PROMPT('Change Data Area') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHLICMD) HLPID(CHGDTAARA)
             PARM       KWD(DTAARA) TYPE(E1) MIN(1) +
                          PROMPT('Data area specification')
             PARM       KWD(VALUE) TYPE(*X) LEN(2000) MIN(1) EXPR(*YES) +
                          PROMPT('New value')
             E1:        ELEM       TYPE(Q1) MIN(1) +
                          SNGVAL((*LDA) (*GDA) (*PDA)) PROMPT('Data area')
                        ELEM       TYPE(E2) DFT(*ALL) SNGVAL((*ALL)) +
                          PROMPT('Substring')
             E2:        ELEM       TYPE(*INT2) MIN(1) RANGE(1 2000) +
                          PROMPT('Start position')
                        ELEM       TYPE(*INT2) MIN(1) RANGE(1 2000) +
                          PROMPT('Length')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
