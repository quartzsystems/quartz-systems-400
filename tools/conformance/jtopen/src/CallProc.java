// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import java.sql.CallableStatement;
import java.sql.Connection;
import java.sql.ResultSet;
import java.sql.Statement;
import java.sql.Types;

/**
 * A stored procedure through the database server: an SQL procedure made for
 * the run in QSTEST with IN, OUT and INOUT parameters and two result sets,
 * called with a CallableStatement; its outputs, then each result set in turn
 * (getMoreResults). A call of a missing procedure. The procedure is dropped at
 * the end.
 */
public final class CallProc {
    public static void main(String[] a) throws Exception {
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        String url = "jdbc:as400://" + Hs.host() + (Hs.skipSignon() ? ":8471" : "") + ";naming=sql;prompt=false;errors=full";
        try (Connection c = Hs.jdbc(url)) {
            Hs.out("ok   connect");
            Hs.step("create", () -> Jdbc.exec(c, "CREATE OR REPLACE PROCEDURE QSTEST.HSPROC1 "
                + "(IN N INTEGER, OUT WORD VARCHAR(10), INOUT AMT DECIMAL(7, 2)) "
                + "DYNAMIC RESULT SETS 2 LANGUAGE SQL "
                + "BEGIN "
                + "DECLARE C1 CURSOR WITH RETURN FOR SELECT ID, NAME FROM (VALUES (1, 'ANN'), (2, 'BOB'), (3, 'CY')) AS T (ID, NAME) WHERE ID <= N ORDER BY ID; "
                + "DECLARE C2 CURSOR WITH RETURN FOR SELECT N * 10 AS TEN FROM SYSIBM.SYSDUMMY1; "
                + "SET WORD = 'DONE'; SET AMT = AMT * 2; OPEN C1; OPEN C2; END"));
            Hs.step("call", () -> {
                try (CallableStatement p = c.prepareCall("CALL QSTEST.HSPROC1(?, ?, ?)")) {
                    p.setInt(1, 2);
                    p.registerOutParameter(2, Types.VARCHAR);
                    p.setBigDecimal(3, new java.math.BigDecimal("12.25"));
                    p.registerOutParameter(3, Types.DECIMAL, 2);
                    boolean rs = p.execute();
                    Hs.out("     result set first=" + rs + " word=" + p.getString(2) + " amt=" + p.getBigDecimal(3));
                    int set = 0;
                    while (rs) {
                        set++;
                        try (ResultSet r = p.getResultSet()) {
                            Hs.out("     set " + set);
                            Jdbc.dump(r, 5);
                        }
                        rs = p.getMoreResults();
                    }
                    Hs.out("     sets=" + set);
                }
            });
            Hs.step("missing", () -> {
                try (CallableStatement p = c.prepareCall("CALL QSTEST.HSNOPROC(?)")) {
                    p.setInt(1, 1);
                    p.execute();
                }
            });
            Hs.step("drop", () -> Jdbc.exec(c, "DROP PROCEDURE QSTEST.HSPROC1"));
        }
    }
}
