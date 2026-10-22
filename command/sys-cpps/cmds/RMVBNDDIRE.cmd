/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVBNDDIRE: remove entries from a binding directory. */
             CMD        PROMPT('Remove Binding Directory Entry') +
                          MAXPOS(2) ALLOW(*ALL) HLPPNLGRP(QHPGCMD) +
                          HLPID(RMVBNDDIRE)
             PARM       KWD(BNDDIR) TYPE(Q1) MIN(1) +
                          PROMPT('Binding directory')
             PARM       KWD(OBJ) TYPE(E1) MIN(1) MAX(300) PROMPT('Object')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL)) +
                          PROMPT('Library')
             Q2:        QUAL       TYPE(*GENERIC) LEN(10) MIN(1) +
                          SPCVAL((*ALL))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL) (*ALL)) +
                          PROMPT('Library')
             E1:        ELEM       TYPE(Q2) MIN(1) PROMPT('Object')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SRVPGM) VALUES(*SRVPGM *MODULE *ALL) +
                          PROMPT('Object type')
