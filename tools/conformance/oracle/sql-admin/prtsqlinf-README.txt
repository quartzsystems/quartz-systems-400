PRTSQLINF on pub400 (V7R5M0), 2026-11-05.

prtsqlinf-pgm.sqlrpgle   the source, member QS15SQL of {user}1/QRPGLESRC
                         (CRTSQLRPGI OBJ({user}1/QS15SQL) COMMIT(*NONE) OBJTYPE(*PGM))
prtsqlinf-pgm.txt        PRTSQLINF OBJ({user}1/QS15SQL) OBJTYPE(*PGM): the spooled file QS15SQL
                         (user data PRTSQLINF, 2 pages) copied with CPYSPLF CTLCHAR(*FCFC);
                         first column the FCFC character, the page width 132.
Messages (IDs only):
  PRTSQLINF of an RPGLE program without SQL     SQL5062 (diagnostic), SQL9011 (escape)
  PRTSQLINF of a program that does not exist    SQL0204 type *PGM, SQL9011
  PRTSQLINF of an object in a missing library   SQL0204 type *LIB in QSYS, SQL9011
  OBJTYPE(*SRVPGM) of a *PGM                    SQL0204 type *SRVPGM, SQL9011
The spooled file is named after the object, in the user's QPRTJOB.
The header: product 5770SS1 at column 1, "Program LIB/NAME" at 58, the name padded to 36,
date yy-mm-dd and time at 102, "Page" at 121 with the number as three digits.
