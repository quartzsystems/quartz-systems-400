/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKPFCST: work with the constraints of physical files. */
             CMD        PROMPT('Work with Physical File Constraints') +
                          MAXPOS(3) ALLOW(*INTERACT *IPGM *IMOD +
                          *IREXX *EXEC) HLPPNLGRP(QHDBCMD) +
                          HLPID(WRKPFCST)
             PARM       KWD(FILE) TYPE(Q1) DFT(*ALL) PROMPT('File')
             PARM       KWD(CST) TYPE(*CHAR) LEN(258) DFT(*ALL) +
                          SPCVAL((*ALL) (*CHKPND)) PROMPT('Constraint')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *REFCST *UNQCST *PRIKEY +
                          *CHKCST) PROMPT('Constraint type')
             Q1:        QUAL       TYPE(*GENERIC) LEN(10) SPCVAL((*ALL))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL) (*ALL) +
                          (*ALLUSR)) PROMPT('Library')
