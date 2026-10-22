/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGAUTLE: change the authority of users already on an authorization */
/* list.                                                                 */
             CMD        PROMPT('Change Auth List Entry') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(CHGAUTLE)
             PARM       KWD(AUTL) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Authorization list')
             PARM       KWD(USER) TYPE(*NAME) LEN(10) MIN(1) MAX(50) +
                          SNGVAL((*PUBLIC)) PROMPT('Users')
             PARM       KWD(AUT) TYPE(*CHAR) LEN(10) RSTD(*YES) MAX(11) +
                          DFT(*USE) VALUES(*USE *ALL *CHANGE *EXCLUDE +
                          *AUTLMGT *OBJALTER *OBJEXIST *OBJMGT *OBJOPR +
                          *OBJREF *ADD *DLT *EXECUTE *READ *UPD) +
                          PROMPT('Authority')
