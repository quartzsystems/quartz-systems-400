/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKJRNRCV: the journal receivers of one library or more. */
             CMD        PROMPT('Work with Journal Receivers') +
                          MAXPOS(1) +
                          ALLOW(*INTERACT *IPGM *IREXX *EXEC *IMOD) +
                          HLPPNLGRP(QHJRCMD) HLPID(WRKJRNRCV)
             PARM       KWD(JRNRCV) TYPE(Q1) MIN(1) EXPR(*YES) +
                          PROMPT('Journal receiver')
             Q1:        QUAL       TYPE(*GENERIC) LEN(10) SPCVAL((*ALL)) +
                          MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL) +
                          (*ALLUSR) (*ALL)) PROMPT('Library')
