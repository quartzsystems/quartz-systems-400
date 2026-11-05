-- oracle: pub400
-- The REGEXP_ functions.
VALUES (REGEXP_COUNT('banana', 'an'));
VALUES (REGEXP_INSTR('banana', 'an', 1, 2));
VALUES (REGEXP_INSTR('banana', 'an', 1, 1, 1));
VALUES (REGEXP_SUBSTR('banana', 'a(n)', 1, 2, 'c', 1));
VALUES (REGEXP_SUBSTR('hello world', '\w+$'));
VALUES (REGEXP_REPLACE('banana', 'an', 'AN'));
VALUES (REGEXP_REPLACE('banana', 'an', 'X', 1, 2));
VALUES (REGEXP_REPLACE('a1b22c', '\d+', '#'));
VALUES (REGEXP_REPLACE('ABC', 'b', 'x', 1, 0, 'i'));
VALUES (REGEXP_LIKE('abc', 'B', 'i'));
