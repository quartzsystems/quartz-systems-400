/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DLTJRNRCV: delete journal receivers. */
             CMD        PROMPT('Delete Journal Receiver') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHJRCMD) HLPID(DLTJRNRCV)
             PARM       KWD(JRNRCV) TYPE(Q1) MIN(1) +
                          PROMPT('Journal receiver')
             PARM       KWD(DLTOPT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *IGNINQMSG *IGNTGTRCV +
                          *IGNEXITPGM) MAX(3) PROMPT('Delete options')
             Q1:        QUAL       TYPE(*GENERIC) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL) (*ALLUSR) +
                          (*ALL)) PROMPT('Library')
