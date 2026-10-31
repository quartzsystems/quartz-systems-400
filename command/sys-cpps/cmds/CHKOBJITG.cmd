/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHKOBJITG: check objects for damage and signatures that no longer */
/* verify against the keys this system trusts.                       */
             CMD        PROMPT('Check Object Integrity') ALLOW(*ALL) +
                          HLPPNLGRP(QHSECMD) HLPID(CHKOBJITG)
             PARM       KWD(USRPRF) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('User profile')
             PARM       KWD(OBJ) TYPE(Q1) DFT(*ALL) SNGVAL((*ALL)) +
                          PROMPT('Object')
             PARM       KWD(OBJTYPE) TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          PROMPT('Object type')
             PARM       KWD(CHKSIG) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SIGNED) VALUES(*SIGNED *ALL *NONE) +
                          PROMPT('Check signatures')
 Q1:         QUAL       TYPE(*GENERIC) LEN(10) DFT(*ALL) SPCVAL((*ALL))
             QUAL       TYPE(*NAME) LEN(10) DFT(*ALL) SPCVAL((*ALL)) +
                          PROMPT('Library')
