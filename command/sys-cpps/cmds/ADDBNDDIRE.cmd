/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDBNDDIRE: add module and service program entries to a binding */
/* directory.                                                         */
             CMD        PROMPT('Add Binding Directory Entry') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(ADDBNDDIRE)
             PARM       KWD(BNDDIR) TYPE(Q1) MIN(1) +
                          PROMPT('Binding directory')
             PARM       KWD(OBJ) TYPE(E1) MIN(1) MAX(300) PROMPT('Object')
             PARM       KWD(POSITION) TYPE(E2) DFT(*LAST) +
                          SNGVAL((*LAST) (*FIRST)) PROMPT('Position')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL)) +
                          PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL)) +
                          PROMPT('Library')
             Q3:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL)) +
                          PROMPT('Library')
             E1:        ELEM       TYPE(Q2) MIN(1) PROMPT('Object')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SRVPGM) VALUES(*SRVPGM *MODULE) +
                          PROMPT('Object type')
             E2:        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          VALUES(*AFTER *BEFORE *REPLACE) +
                          PROMPT('Relative position')
                        ELEM       TYPE(Q3) PROMPT('Reference object')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SRVPGM) VALUES(*SRVPGM *MODULE) +
                          PROMPT('Reference object type')
