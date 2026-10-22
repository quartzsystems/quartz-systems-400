/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKBNDDIR: work with a list of binding directories. */
             CMD        PROMPT('Work with Binding Directories') +
                          MAXPOS(1) ALLOW(*ALL) HLPPNLGRP(QHPGCMD) +
                          HLPID(WRKBNDDIR)
             PARM       KWD(BNDDIR) TYPE(Q1) DFT(*ALL) +
                          PROMPT('Binding directory')
             Q1:        QUAL       TYPE(*GENERIC) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL) (*ALLUSR) +
                          (*ALL)) PROMPT('Library')
