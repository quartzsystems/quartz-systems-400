/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DMPOBJ: a formatted dump of an object to QPSRVDMP. */
             CMD        PROMPT('Dump Object') MAXPOS(2) ALLOW(*ALL) +
                          HLPPNLGRP(QHSXCMD) HLPID(DMPOBJ)
             PARM       KWD(OBJ) TYPE(QO) MIN(1) PROMPT('Object')
             PARM       KWD(OBJTYPE) TYPE(*CHAR) LEN(10) MIN(1) +
                          PROMPT('Object type')
 QO:         QUAL       TYPE(*NAME) LEN(10)
             QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
