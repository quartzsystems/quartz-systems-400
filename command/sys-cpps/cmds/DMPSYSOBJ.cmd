/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DMPSYSOBJ: a formatted dump of a system object by name and type, */
/* or by its object identifier, to QPSRVDMP.                        */
             CMD        PROMPT('Dump System Object') MAXPOS(4) +
                          ALLOW(*ALL) HLPPNLGRP(QHSXCMD) HLPID(DMPSYSOBJ)
             PARM       KWD(OBJ) TYPE(*CHAR) LEN(40) MIN(1) +
                          PROMPT('Object')
             PARM       KWD(CONTEXT) TYPE(*NAME) LEN(10) DFT(QSYS) +
                          SPCVAL((*LIBL)) PROMPT('Context')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Object type')
             PARM       KWD(SUBTYPE) TYPE(*CHAR) LEN(2) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Object subtype')
