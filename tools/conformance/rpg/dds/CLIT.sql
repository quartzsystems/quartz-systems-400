-- Batch 23's table for the SQL CLI: a key, nullable columns, a date.
CREATE TABLE {lib}.CLIT (ID INT NOT NULL PRIMARY KEY,
 NAME VARCHAR(20) CCSID 37, AMT DECIMAL(9, 2), D DATE);
INSERT INTO {lib}.CLIT VALUES
 (1, 'Ann', 10.50, '2024-01-02'),
 (2, 'Bob', NULL, '2024-02-03'),
 (3, 'Cid', -7.25, NULL),
 (4, 'Dee', 99.99, '2024-04-05');
