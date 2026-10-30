/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* MKDIR: make a directory (CRTDIR's alias). */
/* CRTOBJSCAN takes its words here; IBM's definition passes one byte.   */
             CMD        PROMPT('Create Directory') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(MKDIR)
             PARM       KWD(DIR) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Directory')
             PARM       KWD(DTAAUT) TYPE(*NAME) LEN(10) DFT(*INDIR) +
                          SPCVAL((*INDIR) (*RWX) (*RW) (*RX) (*WX) +
                          (*R) (*W) (*X) (*EXCLUDE) (*NONE)) +
                          PROMPT('Public authority for data')
             PARM       KWD(OBJAUT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*INDIR) VALUES(*OBJEXIST *OBJMGT +
                          *OBJALTER *OBJREF) SNGVAL((*INDIR) (*NONE) +
                          (*ALL)) MAX(4) PROMPT('Public authority for object')
             PARM       KWD(CRTOBJAUD) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SYSVAL) VALUES(*SYSVAL *NONE *USRPRF +
                          *CHANGE *ALL) PROMPT('Auditing value for objects')
             PARM       KWD(CRTOBJSCAN) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*PARENT) VALUES(*PARENT *YES *NO *CHGONLY) +
                          PROMPT('Scanning option for objects')
             PARM       KWD(RSTDRNMUNL) TYPE(*INT2) RSTD(*YES) DFT(*NO) +
                          SPCVAL((*NO 0) (*YES 1)) +
                          PROMPT('Restricted rename and unlink')
