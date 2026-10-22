/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RTVMBRD: put the description of a database file member into CL */
/* variables.                                                         */
             CMD        PROMPT('Retrieve Member Description') MAXPOS(2) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(RTVMBRD)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(MBR) TYPE(E1) DFT(*FIRST) PROMPT('Member')
             PARM       KWD(ASPDEV) TYPE(*NAME) LEN(10) DFT(*) +
                          SPCVAL((*) (*CURASPGRP)) PROMPT('ASP device')
             PARM       KWD(RTNLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for RTNLIB (10)')
             PARM       KWD(RTNSYSTEM) TYPE(*CHAR) LEN(4) RTNVAL(*YES) +
                          PROMPT('CL var for RTNSYSTEM (4)')
             PARM       KWD(RTNMBR) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for RTNMBR (10)')
             PARM       KWD(FILEATR) TYPE(*CHAR) LEN(5) RTNVAL(*YES) +
                          PROMPT('CL var for FILEATR (5)')
             PARM       KWD(FILETYPE) TYPE(*CHAR) LEN(5) RTNVAL(*YES) +
                          PROMPT('CL var for FILETYPE (5)')
             PARM       KWD(SRCTYPE) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for SRCTYPE (10)')
             PARM       KWD(CRTDATE) TYPE(*CHAR) LEN(13) RTNVAL(*YES) +
                          PROMPT('CL var for CRTDATE (13)')
             PARM       KWD(EXPDATE) TYPE(*CHAR) LEN(7) RTNVAL(*YES) +
                          PROMPT('CL var for EXPDATE (7)')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) RTNVAL(*YES) +
                          PROMPT('CL var for TEXT (50)')
             PARM       KWD(NBRCURRCD) TYPE(*DEC) LEN(10 0) RTNVAL(*YES) +
                          PROMPT('CL var for NBRCURRCD (10 0)')
             PARM       KWD(NBRDLTRCD) TYPE(*DEC) LEN(10 0) RTNVAL(*YES) +
                          PROMPT('CL var for NBRDLTRCD (10 0)')
             PARM       KWD(DTASPCSIZ) TYPE(*DEC) LEN(15 0) RTNVAL(*YES) +
                          PROMPT('CL var for DTASPCSIZ (15 0)')
             PARM       KWD(ACCPTHSIZ) TYPE(*DEC) LEN(15 0) RTNVAL(*YES) +
                          PROMPT('CL var for ACCPTHSIZ (15 0)')
             PARM       KWD(NBRDTAMBRS) TYPE(*DEC) LEN(5 0) RTNVAL(*YES) +
                          PROMPT('CL var for NBRDTAMBRS (5 0)')
             PARM       KWD(CHGDATE) TYPE(*CHAR) LEN(13) RTNVAL(*YES) +
                          PROMPT('CL var for CHGDATE (13)')
             PARM       KWD(SAVDATE) TYPE(*CHAR) LEN(13) RTNVAL(*YES) +
                          PROMPT('CL var for SAVDATE (13)')
             PARM       KWD(RSTDATE) TYPE(*CHAR) LEN(13) RTNVAL(*YES) +
                          PROMPT('CL var for RSTDATE (13)')
             PARM       KWD(NBRDAYUSED) TYPE(*DEC) LEN(5 0) RTNVAL(*YES) +
                          PROMPT('CL var for NBRDAYUSED (5 0)')
             PARM       KWD(USEDATE) TYPE(*CHAR) LEN(7) RTNVAL(*YES) +
                          PROMPT('CL var for USEDATE (7)')
             PARM       KWD(SRCCHGDATE) TYPE(*CHAR) LEN(13) RTNVAL(*YES) +
                          PROMPT('CL var for SRCCHGDATE (13)')
             PARM       KWD(JOINMBR) TYPE(*CHAR) LEN(4) RTNVAL(*YES) +
                          PROMPT('CL var for JOINMBR (4)')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST) (*FIRSTMBR) (*LAST) +
                          (*LASTMBR)) PROMPT('Member')
                        ELEM       TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NEXT *PRV) +
                          PROMPT('Relationship')
