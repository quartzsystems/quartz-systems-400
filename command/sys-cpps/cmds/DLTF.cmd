/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DLTF: delete a file, or every file matching a generic name. */
             CMD        PROMPT('Delete File') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(DLTF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(SYSTEM) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*LCL) VALUES(*LCL *RMT *FILETYPE) +
                          PROMPT('System')
             PARM       KWD(RMVCST) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*RESTRICT) VALUES(*RESTRICT *REMOVE *KEEP) +
                          PROMPT('Remove constraint')
             Q1:        QUAL       TYPE(*GENERIC) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL) (*ALLUSR) +
                          (*ALL)) PROMPT('Library')
