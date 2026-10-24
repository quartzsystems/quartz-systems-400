-- Batch 11's department table, joined with EMPT.
CREATE TABLE {lib}.DEPTT (DEPT CHAR(3) CCSID 37 NOT NULL PRIMARY KEY,
 DNAME VARCHAR(15) CCSID 37);
INSERT INTO {lib}.DEPTT VALUES ('ACC', 'Accounting'),
 ('DEV', 'Development'), ('OPS', 'Operations');
