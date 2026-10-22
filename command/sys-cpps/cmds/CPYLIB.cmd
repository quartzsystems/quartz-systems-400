/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CPYLIB: copy a library and its objects to another library. */
             CMD        PROMPT('Copy Library') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHLICMD) HLPID(CPYLIB)
             PARM       KWD(FROMLIB) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('From library')
             PARM       KWD(TOLIB) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('To library')
             PARM       KWD(CRTLIB) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Create library')
             PARM       KWD(TOASP) TYPE(*INT2) DFT(1) RANGE(1 32) +
                          PMTCTL(P1) PROMPT('To ASP number')
             PARM       KWD(TOASPDEV) TYPE(*NAME) LEN(10) DFT(*ASP) +
                          SPCVAL((*ASP) (*ASPGRPPRI)) PMTCTL(P1) +
                          PROMPT('To ASP device')
             P1:        PMTCTL     CTL(CRTLIB) COND((*EQ *YES))
