/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGCMD: change the attributes of a command that the definition */
/* source does not fix.                                             */
             CMD        PROMPT('Change Command') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(CHGCMD)
             PARM       KWD(CMD) TYPE(Q1) MIN(1) PROMPT('Command')
             PARM       KWD(PGM) TYPE(Q2) DFT(*SAME) SNGVAL((*SAME) +
                          (*REXX)) PROMPT('Program to process command')
             PARM       KWD(REXSRCFILE) TYPE(Q3) DFT(*SAME) +
                          SNGVAL((*SAME)) PROMPT('REXX source file')
             PARM       KWD(REXSRCMBR) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*CMD)) PROMPT('REXX source member')
             PARM       KWD(REXCMDENV) TYPE(Q3) DFT(*SAME) +
                          SNGVAL((*SAME) (*COMMAND) (*CPICOMM) +
                          (*EXECSQL)) PROMPT('REXX command environment')
             PARM       KWD(VLDCKR) TYPE(Q3) DFT(*SAME) SNGVAL((*SAME) +
                          (*NONE)) PROMPT('Validity checking program')
             PARM       KWD(MODE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *ALL *PROD *DEBUG +
                          *SERVICE) MAX(3) PROMPT('Mode in which valid')
             PARM       KWD(ALLOW) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *ALL *BATCH *INTERACT +
                          *BPGM *IPGM *BREXX *IREXX *EXEC *BMOD *IMOD) +
                          MAX(9) PROMPT('Where allowed to run')
             PARM       KWD(ALWLMTUSR) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Allow limited users')
             PARM       KWD(MAXPOS) TYPE(*INT2) DFT(*SAME) RANGE(0 99) +
                          SPCVAL((*SAME -2) (*NONE -1)) +
                          PROMPT('Max positional parameters')
             PARM       KWD(PMTFILE) TYPE(E1) DFT(*SAME) SNGVAL((*SAME) +
                          (*NONE)) PROMPT('Prompt file')
             PARM       KWD(MSGF) TYPE(Q3) DFT(*SAME) SNGVAL((*SAME)) +
                          PROMPT('Message file')
             PARM       KWD(HLPPNLGRP) TYPE(Q3) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) PROMPT('Help panel group')
             PARM       KWD(HLPID) TYPE(*CHAR) LEN(32) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE) (*CMD)) +
                          PROMPT('Help identifier')
             PARM       KWD(HLPSCHIDX) TYPE(Q3) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) +
                          PROMPT('Help search index')
             PARM       KWD(CURLIB) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*NOCHG) (*CRTDFT)) +
                          PROMPT('Current library')
             PARM       KWD(PRDLIB) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*NOCHG) (*NONE)) +
                          PROMPT('Product library')
             PARM       KWD(PMTOVRPGM) TYPE(Q3) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) +
                          PROMPT('Prompt override program')
             PARM       KWD(ENBGUI) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Enable GUI')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) PROMPT('Description')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q3:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(Q3) PROMPT('File')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*STATIC) VALUES(*STATIC *DYNAMIC) +
                          PROMPT('Message retrieval')
