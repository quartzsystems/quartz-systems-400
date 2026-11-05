-- oracle: pub400
-- JSON_TABLE's table-level ON ERROR: a context item that is not JSON, a strict row path that finds nothing, by default, with EMPTY ON ERROR and with ERROR ON ERROR.
SELECT COUNT(*) FROM JSON_TABLE('not json', '$' COLUMNS(A INTEGER PATH '$.a')) AS T;
SELECT COUNT(*) FROM JSON_TABLE('not json', '$' COLUMNS(A INTEGER PATH '$.a') EMPTY ON ERROR) AS T;
SELECT COUNT(*) FROM JSON_TABLE('not json', '$' COLUMNS(A INTEGER PATH '$.a') ERROR ON ERROR) AS T;
SELECT COUNT(*) FROM JSON_TABLE('{"a":1}', 'strict $.r[*]' COLUMNS(A INTEGER PATH '$.a')) AS T;
SELECT COUNT(*) FROM JSON_TABLE('{"a":1}', 'strict $.r[*]' COLUMNS(A INTEGER PATH '$.a') ERROR ON ERROR) AS T;
SELECT T.A FROM JSON_TABLE('{"r":[{"a":1},{"a":2}]}', '$.r[*]' COLUMNS(A INTEGER PATH '$.a') ERROR ON ERROR) AS T ORDER BY T.A;
SELECT T.A FROM JSON_TABLE('{"r":[{"a":"x"}]}', '$.r[*]' COLUMNS(A INTEGER PATH '$.a') ERROR ON ERROR) AS T;
SELECT COUNT(*) FROM JSON_TABLE('{"a":1}', '$' COLUMNS(A INTEGER PATH '$.a') NULL ON ERROR) AS T;
