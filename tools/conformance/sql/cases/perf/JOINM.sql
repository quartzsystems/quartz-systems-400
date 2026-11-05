-- oracle: primary
-- Joins of every kind over an equality: the same rows whatever the method.
CREATE TABLE {lib}.QSJMA (K INTEGER, A CHAR(3));
CREATE TABLE {lib}.QSJMB (K INTEGER, B CHAR(3));
INSERT INTO {lib}.QSJMA VALUES (1, 'a1'), (2, 'a2'), (2, 'a2b'), (NULL, 'an'), (5, 'a5');
INSERT INTO {lib}.QSJMB VALUES (2, 'b2'), (1, 'b1'), (NULL, 'bn'), (3, 'b3'), (2, 'b2b');
SELECT A.A, B.B FROM {lib}.QSJMA A JOIN {lib}.QSJMB B ON A.K = B.K ORDER BY 1, 2;
SELECT A.A, B.B FROM {lib}.QSJMA A LEFT JOIN {lib}.QSJMB B ON A.K = B.K ORDER BY 1, 2;
SELECT A.A, B.B FROM {lib}.QSJMA A RIGHT JOIN {lib}.QSJMB B ON A.K = B.K ORDER BY 2, 1;
SELECT A.A, B.B FROM {lib}.QSJMA A FULL JOIN {lib}.QSJMB B ON A.K = B.K ORDER BY 1, 2;
SELECT A.A FROM {lib}.QSJMA A EXCEPTION JOIN {lib}.QSJMB B ON A.K = B.K ORDER BY 1;
SELECT A.A, B.B FROM {lib}.QSJMA A, {lib}.QSJMB B WHERE A.K = B.K AND B.B <> 'b2' ORDER BY 1, 2;
DROP TABLE {lib}.QSJMA;
DROP TABLE {lib}.QSJMB;
