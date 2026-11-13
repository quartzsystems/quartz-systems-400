/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ENDNFSSVR: end the NFS server daemons, one type or all of them.   */
             CMD        PROMPT('End NFS Server') MAXPOS(2) ALLOW(*ALL) +
                          HLPPNLGRP(QHP0LCMD) HLPID(ENDNFSSVR)
             PARM       KWD(SERVER) TYPE(*INT4) RSTD(*YES) MIN(1) +
                          SPCVAL((*ALL -1) (*RPC -2) (*BIO -3) +
                          (*GSS -8) (*RGY -9) (*SVR -4) (*MNT -5) +
                          (*NSM -6) (*NLM -7)) +
                          CHOICE('*ALL, *RPC, *BIO, *GSS...') +
                          PROMPT('Server daemon')
             PARM       KWD(ENDJOBTIMO) TYPE(*INT4) DFT(30) +
                          RANGE(1 3600) SPCVAL((*NOMAX -1)) +
                          CHOICE('1-3600 seconds') +
                          PROMPT('Timeout for end of daemon')
