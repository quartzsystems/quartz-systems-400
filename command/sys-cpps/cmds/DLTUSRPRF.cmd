/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DLTUSRPRF: delete a user profile, saying what becomes of the objects */
/* it owns and of those it is the primary group of.                      */
             CMD        PROMPT('Delete User Profile') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(DLTUSRPRF)
             PARM       KWD(USRPRF) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('User profile')
             PARM       KWD(OWNOBJOPT) TYPE(E1) DFT(*NODLT) +
                          SNGVAL((*NODLT) (*DLT)) PROMPT('Owned objects')
             PARM       KWD(PGPOPT) TYPE(E2) DFT(*NOCHG) +
                          SNGVAL((*NOCHG)) PROMPT('Primary group of objects')
             PARM       KWD(EIMASSOC) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*DLT) VALUES(*DLT *KEEP) +
                          PROMPT('EIM association')
             E1:        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*CHGOWN) VALUES(*CHGOWN) +
                          PROMPT('Action')
                        ELEM       TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('New owner')
             E2:        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*CHGPGP) VALUES(*CHGPGP) +
                          PROMPT('Action')
                        ELEM       TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('New primary group')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*OLDPGP) VALUES(*OLDPGP *PRIVATE *ALL +
                          *CHANGE *USE *EXCLUDE) +
                          PROMPT('New primary group authority')
