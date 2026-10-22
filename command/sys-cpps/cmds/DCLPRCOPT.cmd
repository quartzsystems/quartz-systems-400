/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DCLPRCOPT: set the compile and bind options of a CL procedure from */
/* inside its source.                                                   */
             CMD        PROMPT('Declare Processing Options') +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(DCLPRCOPT)
             PARM       KWD(SUBRSTACK) TYPE(*INT2) DFT(99) RANGE(20 9999) +
                          PROMPT('Subroutine stack depth')
             PARM       KWD(LOG) TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*JOB) +
                          VALUES(*JOB *YES *NO) PROMPT('Log commands')
             PARM       KWD(ALWRTVSRC) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PROMPT('Allow RTVCLSRC')
             PARM       KWD(USRPRF) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*USER) VALUES(*USER *OWNER) +
                          PROMPT('User profile')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PROMPT('Authority')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SRCMBRTXT) +
                          SPCVAL((*SRCMBRTXT) (*BLANK ' ')) +
                          PROMPT('Description')
             PARM       KWD(DFTACTGRP) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Default activation group')
             PARM       KWD(ACTGRP) TYPE(*NAME) LEN(10) DFT(*STGMDL) +
                          SPCVAL((*STGMDL) (*NEW) (*CALLER)) +
                          PROMPT('Activation group')
             PARM       KWD(STGMDL) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SNGLVL) VALUES(*SNGLVL *TERASPACE +
                          *INHERIT) PROMPT('Storage model')
             PARM       KWD(BNDDIR) TYPE(Q1) MAX(300) DFT(*NONE) +
                          SNGVAL((*NONE)) PROMPT('Binding directory')
             PARM       KWD(BNDSRVPGM) TYPE(E1) MAX(300) DFT(*NONE) +
                          SNGVAL((*NONE)) PROMPT('Bind service program')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL)) +
                          PROMPT('Library')
             Q2:        QUAL       TYPE(*GENERIC) LEN(10) SPCVAL((*ALL))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL)) +
                          PROMPT('Library')
             E1:        ELEM       TYPE(Q2) PROMPT('Service program')
                        ELEM       TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*IMMED) VALUES(*IMMED *DEFER) +
                          PROMPT('Activation')
