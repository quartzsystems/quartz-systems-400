// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import java.sql.CallableStatement;
import java.sql.Connection;
import java.sql.Types;

/**
 * OUT and INOUT parameters of each common type through the database server: an
 * SQL procedure made in QSTEST sets one parameter of each type, one OUT is left
 * null, an INOUT VARCHAR grows; every output printed with getString and
 * wasNull. The procedure is dropped at the end.
 */
public final class CallOut {
    private static final String[][] PARMS = {
        {"C", "CHAR(6)", "'AB'", "1"},
        {"V", "VARCHAR(20)", "'HELLO WORLD'", "12"},
        {"S", "SMALLINT", "-12", "5"},
        {"I", "INTEGER", "123456", "4"},
        {"B", "BIGINT", "9876543210", "-5"},
        {"D", "DECIMAL(11, 3)", "-1234.567", "3"},
        {"N", "NUMERIC(9, 2)", "42.50", "2"},
        {"F", "DOUBLE", "1.5E3", "8"},
        {"DT", "DATE", "'2026-11-06'", "91"},
        {"TM", "TIME", "'13.45.30'", "92"},
        {"TS", "TIMESTAMP", "'2026-11-06-13.45.30.123456'", "93"},
        {"VB", "VARCHAR(8) FOR BIT DATA", "X'C1C2C3'", "-3"},
        {"NL", "VARCHAR(10)", null, "12"},
    };

    public static void main(String[] a) throws Exception {
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        String url = "jdbc:as400://" + Hs.host() + (Hs.skipSignon() ? ":8471" : "") + ";naming=sql;prompt=false;errors=full";
        try (Connection c = Hs.jdbc(url)) {
            Hs.out("ok   connect");
            StringBuilder parms = new StringBuilder();
            StringBuilder body = new StringBuilder();
            for (String[] p : PARMS) {
                parms.append("OUT ").append(p[0]).append(' ').append(p[1]).append(", ");
                if (p[2] != null) {
                    body.append("SET ").append(p[0]).append(" = ").append(p[2]).append("; ");
                }
            }
            parms.append("INOUT IO VARCHAR(30)");
            body.append("SET IO = IO CONCAT '-MORE'; ");
            Hs.step("create", () -> Jdbc.exec(c, "CREATE OR REPLACE PROCEDURE QSTEST.HSPROC2 (" + parms
                + ") LANGUAGE SQL BEGIN " + body + "END"));
            Hs.step("call", () -> {
                StringBuilder marks = new StringBuilder();
                for (int i = 0; i <= PARMS.length; i++) {
                    marks.append(i == 0 ? "?" : ", ?");
                }
                try (CallableStatement p = c.prepareCall("CALL QSTEST.HSPROC2(" + marks + ")")) {
                    for (int i = 0; i < PARMS.length; i++) {
                        p.registerOutParameter(i + 1, Integer.parseInt(PARMS[i][3]));
                    }
                    int io = PARMS.length + 1;
                    p.setString(io, "START");
                    p.registerOutParameter(io, Types.VARCHAR);
                    p.execute();
                    for (int i = 0; i < PARMS.length; i++) {
                        String v = p.getString(i + 1);
                        Hs.out("     " + PARMS[i][0] + " " + PARMS[i][1] + " = " + (p.wasNull() ? "NULL" : "[" + v + "]"));
                    }
                    Hs.out("     IO VARCHAR(30) = [" + p.getString(io) + "]");
                }
            });
            Hs.step("drop", () -> Jdbc.exec(c, "DROP PROCEDURE QSTEST.HSPROC2"));
        }
    }
}
