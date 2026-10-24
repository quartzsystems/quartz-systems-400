**FREE
CTL-OPT DftActGrp(*No);
Ctl-Opt Option(*NoDebugIO);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
line = 'mixed case control';
write QSYSPRT line;
*inlr = *on;
