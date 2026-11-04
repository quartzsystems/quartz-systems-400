/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHKOBJITG: check objects for damage and signatures that no longer */
/* verify against the keys this system trusts.                       */
             CMD        PROMPT('Check Object Integrity') ALLOW(*ALL) +
                          HLPPNLGRP(QHSECMD) HLPID(CHKOBJITG)
             PARM       KWD(USRPRF) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('User profile')
             PARM       KWD(OBJ) TYPE(Q1) PROMPT('Object')
             PARM       KWD(OBJTYPE) TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          PROMPT('Object type')
             PARM       KWD(CHKSIG) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SIGNED) VALUES(*SIGNED *ALL *NONE) +
                          PROMPT('Check signatures')
             PARM       KWD(OUTFILE) TYPE(Q2) FILE(*OUT) +
                          PROMPT('File to receive output')
             PARM       KWD(OUTMBR) TYPE(E1) +
                          PROMPT('Output member options')
 Q1:         QUAL       TYPE(*GENERIC) LEN(10) DFT(*ALL) SPCVAL((*ALL))
             QUAL       TYPE(*NAME) LEN(10) DFT(*ALL) SPCVAL((*ALL)) +
                          PROMPT('Library')
 Q2:         QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
 E1:         ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Member to receive output')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD) +
                          PROMPT('Replace or add records')
