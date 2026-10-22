/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DLTBNDDIR: delete one binding directory or a generic set of them. */
             CMD        PROMPT('Delete Binding Directory') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(DLTBNDDIR)
             PARM       KWD(BNDDIR) TYPE(Q1) MIN(1) +
                          PROMPT('Binding directory')
             Q1:        QUAL       TYPE(*GENERIC) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL) (*ALL) +
                          (*ALLUSR)) PROMPT('Library')
