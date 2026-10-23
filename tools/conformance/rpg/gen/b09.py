"""Corpus batch 9: exceptions. Writes ../b09exc/*.rpgle.

usage: python -I b09.py
"""
import os

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "b09exc")
HEAD = [
    "**FREE",
    "ctl-opt dftactgrp(*no);",
    "dcl-f QSYSPRT printer(132);",
    "dcl-ds line len(132) end-ds;",
]
PSDS = [
    "dcl-ds pgmsts psds qualified;",
    "  proc *proc;",
    "  status *status;",
    "  prvsts zoned(5:0) pos(16);",
    "  stmt char(8) pos(21);",
    "  routine *routine;",
    "  parms *parms;",
    "  msgtype char(3) pos(40);",
    "  msgnum char(4) pos(43);",
    "  pgmlib char(10) pos(81);",
    "  excid char(7) pos(40);",
    "end-ds;",
]
M = {}


def member(name, decls, body, tail=None):
    M[name] = HEAD + decls + body + (tail if tail is not None else ["*inlr = *on;"])


def p(expr):
    return [f"line = {expr};", "write QSYSPRT line;"]


# An unhandled divide by zero: the inquiry's default reply ends it.
member("EXC001", ["dcl-s z packed(3:0) inz(0);", "dcl-s r packed(5:0);"],
       p("'before'") + ["r = 10 / z;"] + p("'after'"))

# An unhandled size error in a linear-main program.
M["EXC002"] = [
    "**FREE",
    "ctl-opt dftactgrp(*no) main(go);",
    "dcl-f QSYSPRT printer(132);",
    "dcl-ds line len(132) end-ds;",
    "dcl-proc go;",
    "  dcl-s n packed(3:0) inz(999);",
    "  line = 'start';",
    "  write QSYSPRT line;",
    "  n += 1;",
    "  line = 'not reached';",
    "  write QSYSPRT line;",
    "end-proc;",
]

# *PSSR with *CANCL: the program ends after the subroutine.
member("EXC003", PSDS + ["dcl-s z packed(3:0) inz(0);", "dcl-s r packed(5:0);"],
       p("'before'") + ["r = 10 / z;"] + p("'after'") + ["*inlr = *on;", "begsr *pssr;"]
       + ["  " + x for x in p("'pssr ' + %char(pgmsts.status) + ' ' + pgmsts.routine + ' ' + pgmsts.excid")]
       + ["endsr '*CANCL';"], tail=[])

# *PSSR invoked by EXSR, and the routine name.
member("EXC004", PSDS, ["exsr *pssr;"] + p("'back'") + ["*inlr = *on;", "begsr *pssr;"]
       + ["  " + x for x in p("'in pssr ' + %char(pgmsts.status) + ' [' + pgmsts.routine + ']'")]
       + ["endsr;"], tail=[])

# PSDS: names, statement, parameters, exception fields after a handled error.
member("EXC005", PSDS + ["dcl-s z packed(3:0) inz(0);", "dcl-s r packed(5:0);"], [
    "monitor;",
    "  r = 1 / z;",
    "on-error;",
] + ["  " + x for x in p("'st=' + %char(pgmsts.status) + ' ex=' + pgmsts.excid + ' rt=' + pgmsts.routine")]
  + ["  " + x for x in p("'pr=' + pgmsts.proc + ' pa=' + %char(pgmsts.parms) + ' sm=' + pgmsts.stmt")]
  + ["endmon;"] + p("'status now ' + %char(%status) + ' ' + %char(pgmsts.status)"))

# A subprocedure's unhandled error seen by the caller's monitor.
member("EXC006", ["dcl-s r packed(5:0);"], [
    "monitor;",
    "  r = fail(0);",
    "  " + p("'no'")[0],
    "  write QSYSPRT line;",
    "on-error 202;",
] + ["  " + x for x in p("'caught 202 ' + %char(%status)")] + [
    "on-error;",
] + ["  " + x for x in p("'other ' + %char(%status)")] + ["endmon;"], tail=[
    "*inlr = *on;",
    "dcl-proc fail;",
    "  dcl-pi *n packed(5:0);",
    "    d packed(5:0) value;",
    "  end-pi;",
    "  return 100 / d;",
    "end-proc;",
])

# Nested monitors: the inner one that matches takes it.
member("EXC007", ["dcl-s z packed(3:0) inz(0);", "dcl-s r packed(5:0);", "dcl-s c char(5) inz('abc');", "dcl-s n int(10) inz(9);"], [
    "monitor;",
    "  monitor;",
    "    r = 1 / z;",
    "  on-error 103;",
] + ["    " + x for x in p("'inner 103'")] + ["  endmon;"]
  + ["  " + x for x in p("'after inner'")] + [
    "on-error 102;",
] + ["  " + x for x in p("'outer 102'")] + ["endmon;", "monitor;", "  c = %subst(c:n:2);", "on-error;"]
  + ["  " + x for x in p("'subst ' + %char(%status)")] + ["endmon;"])

# Array index out of range.
member("EXC008", ["dcl-s a char(2) dim(3);", "dcl-s i int(10) inz(4);"], [
    "monitor;",
    "  a(i) = 'x';",
    "on-error 121;",
] + ["  " + x for x in p("'index 121'")] + ["endmon;", "monitor;", "  i = 0;", "  a(i) = 'y';", "on-error;"]
  + ["  " + x for x in p("'index ' + %char(%status)")] + ["endmon;"])

# *PSSR with no return point after an error, guarded against looping.
member("EXC009", PSDS + ["dcl-s z packed(3:0) inz(0);", "dcl-s r packed(5:0);", "dcl-s times int(10);"],
       p("'before'") + ["r = 10 / z;"] + p("'after error'") + ["*inlr = *on;", "return;", "begsr *pssr;",
                                                               "  times += 1;"]
       + ["  " + x for x in p("'pssr ' + %char(times)")]
       + ["  if times > 2;", "    *inlr = *on;", "    return;", "  endif;", "endsr;"], tail=[])

# ON-EXIT (7.5).
member("EXC010", [], ["callp work();"] + p("'back in main'"), tail=[
    "*inlr = *on;",
    "dcl-proc work;",
    "  line = 'working';",
    "  write QSYSPRT line;",
    "  return;",
    "  on-exit;",
    "    line = 'on-exit runs';",
    "    write QSYSPRT line;",
    "end-proc;",
])

# SND-MSG *ESCAPE and ON-EXCP (7.5).
member("EXC011", [], [
    "monitor;",
    "  snd-msg *escape %msg('CPF9898':'QCPFMSG':'boom');",
    "on-excp 'CPF9898';",
] + ["  " + x for x in p("'caught CPF9898 ' + %char(%status)")] + ["endmon;"])

# A CALL to a program that does not exist: status 211.
member("EXC012", ["dcl-pr nothere extpgm('NOTHEREXYZ');", "end-pr;"], [
    "monitor;",
    "  nothere();",
    "on-error 211;",
] + ["  " + x for x in p("'not found 211'")] + ["on-error;"]
  + ["  " + x for x in p("'other ' + %char(%status)")] + ["endmon;"])

# Errors in an expression's built-ins: %DEC of bad data, %INT overflow.
member("EXC013", ["dcl-s c char(5) inz('12x');", "dcl-s n packed(5:0);", "dcl-s i int(5);"], [
    "monitor;",
    "  n = %dec(c:5:0);",
    "on-error;",
] + ["  " + x for x in p("'dec ' + %char(%status)")] + ["endmon;", "monitor;", "  i = 99999;", "on-error;"]
  + ["  " + x for x in p("'int ' + %char(%status)")] + ["endmon;"])

for name, lines in M.items():
    with open(os.path.join(OUT, name + ".rpgle"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
print(len(M), "members")
