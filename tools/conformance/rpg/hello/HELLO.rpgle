**FREE
dcl-f QSYSPRT printer(132);
dcl-ds line len(132);
end-ds;
line = 'Hello, world';
write QSYSPRT line;
*inlr = *on;
