/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGRDBDIRE: change an entry of the relational database directory. */
             CMD        PROMPT('Change RDB Directory Entry') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHRWCMD) HLPID(CHGRDBDIRE)
             PARM       KWD(RDB) TYPE(E1) MIN(1) PROMPT('Entry')
             PARM       KWD(RMTLOCNAME) TYPE(E2) +
                          SNGVAL((*ARDPGM) (*LOOPBACK) (*MIRROR)) +
                          PMTCTL(*PMTRQS) PROMPT('Remote location')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) PMTCTL(*PMTRQS) +
                          PROMPT('Text')
             PARM       KWD(ARDPGM) TYPE(E3) DFT(*SAME) +
                          SNGVAL((*SAME) (*DRDA)) PMTCTL(*PMTRQS) +
                          PROMPT('Application requester driver')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*LOC)) PMTCTL(*PMTRQS) +
                          PROMPT('Device')
             PARM       KWD(LCLLOCNAME) TYPE(*CNAME) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME) (*LOC) (*NETATR)) +
                          PMTCTL(*PMTRQS) PROMPT('Local location')
             PARM       KWD(RMTNETID) TYPE(*CNAME) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME) (*LOC) (*NETATR) (*NONE)) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Remote network identifier')
             PARM       KWD(MODE) TYPE(*CNAME) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME) (*NETATR)) PMTCTL(*PMTRQS) +
                          PROMPT('Mode')
             PARM       KWD(TNSPGM) TYPE(*CHAR) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME) (*DRDA)) PMTCTL(*PMTRQS) +
                          PROMPT('Transaction program')
             PARM       KWD(PORT) TYPE(*CHAR) LEN(14) DFT(*SAME) +
                          SPCVAL((*SAME) (*DRDA)) PMTCTL(*PMTRQS) +
                          PROMPT('Port number or service program')
             PARM       KWD(RMTAUTMTH) TYPE(E4) PMTCTL(*PMTRQS) +
                          PROMPT('Remote authentication method')
             PARM       KWD(SECCNN) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NONE *SSL *TLS) +
                          PMTCTL(*PMTRQS) PROMPT('Secure connection')
             PARM       KWD(ENCALG) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *DES *AES) +
                          PMTCTL(*PMTRQS) PROMPT('Encryption algorithm')
             E1:        ELEM       TYPE(*CHAR) LEN(18) MIN(1) +
                          PROMPT('Relational database')
                        ELEM       TYPE(*CHAR) LEN(18) DFT(*NONE) +
                          SPCVAL((*NONE)) +
                          PROMPT('Relational database alias')
             E2:        ELEM       TYPE(*CHAR) LEN(254) DFT(*SAME) +
                          SPCVAL((*SAME) (*LOCAL)) PROMPT('Name or address')
                        ELEM       TYPE(*CHAR) LEN(10) RSTD(*YES) DFT(*SAME) +
                          VALUES(*SAME *SNA *IP) PROMPT('Type')
             E3:        ELEM       TYPE(Q1) PROMPT('Program')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E4:        ELEM       TYPE(*CHAR) LEN(10) RSTD(*YES) DFT(*SAME) +
                          VALUES(*SAME *USRID *USRIDPWD *USRENCPWD +
                          *ENCUSRPWD *KERBEROS *ENCRYPTED) +
                          PROMPT('Preferred method')
                        ELEM       TYPE(*CHAR) LEN(11) RSTD(*YES) DFT(*SAME) +
                          VALUES(*SAME *ALWLOWER *NOALWLOWER) +
                          PROMPT('Allow lower authentication')
