/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RCLLNK: reclaim the IFS: attributes, owners, authorities, links. */
             CMD        PROMPT('Reclaim Object Links') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(RCLLNK)
             PARM       KWD(OBJ) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Object')
             PARM       KWD(SUBTREE) TYPE(*INT4) RSTD(*YES) DFT(*DIR) +
                          SPCVAL((*DIR 1) (*NONE 0) (*ALL 2)) +
                          PROMPT('Directory subtree')
             PARM       KWD(DMGOBJOPT) TYPE(E1) +
                          PROMPT('Damaged object option')
 E1:         ELEM       TYPE(*INT4) RSTD(*YES) DFT(*KEEP) +
                          SPCVAL((*KEEP 0) (*DELETE 1)) +
                          PROMPT('Object option')
             ELEM       TYPE(*INT4) RSTD(*YES) DFT(*DELETE) +
                          SPCVAL((*DELETE 1) (*KEEP 0)) +
                          PROMPT('Unusable directory option')
