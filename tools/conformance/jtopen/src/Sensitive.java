// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.ResultSet;
import java.sql.Statement;

/**
 * What a sensitive cursor sees: a table in QTEMP with three rows, a
 * scrollable sensitive result set over it fetched one row at a time with
 * no blocking, and between fetches an insert, an update of a row not yet
 * fetched, a delete of a row not yet fetched and an update of a row
 * already fetched (seen when the cursor goes back). Then the same with an
 * insensitive cursor, and the forms the server refuses.
 */
public final class Sensitive {
    static void show(ResultSet r, String what) throws Exception {
        Hs.out("     " + what + " " + r.getInt(1) + " " + r.getString(2).trim());
    }

    static void run(Connection c, int type, String label) throws Exception {
        Jdbc.exec(c, "DELETE FROM QTEMP.QSSENS");
        Jdbc.exec(c, "INSERT INTO QTEMP.QSSENS VALUES (1, 'ONE'), (2, 'TWO'), (3, 'THREE')");
        try (Statement s = c.createStatement(type, ResultSet.CONCUR_READ_ONLY); Statement u = c.createStatement()) {
            s.setFetchSize(1);
            try (ResultSet r = s.executeQuery("SELECT ID, NAME FROM QTEMP.QSSENS")) {
                Hs.out("     " + label + " type=" + r.getType());
                r.next();
                show(r, "first");
                u.executeUpdate("INSERT INTO QTEMP.QSSENS VALUES (4, 'FOUR')");
                u.executeUpdate("UPDATE QTEMP.QSSENS SET NAME = 'TWO*' WHERE ID = 2");
                u.executeUpdate("DELETE FROM QTEMP.QSSENS WHERE ID = 3");
                u.executeUpdate("UPDATE QTEMP.QSSENS SET NAME = 'ONE*' WHERE ID = 1");
                while (r.next()) {
                    show(r, "next");
                }
                Hs.out("     end");
                if (r.first()) {
                    show(r, "back to first");
                }
            }
        }
    }

    public static void main(String[] a) throws Exception {
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        String url = "jdbc:as400://" + Hs.host() + (Hs.skipSignon() ? ":8471" : "")
            + ";naming=sql;prompt=false;errors=full;block criteria=0;prefetch=false";
        try (Connection c = DriverManager.getConnection(url, Hs.user(), Hs.pass())) {
            Hs.out("ok   connect");
            Hs.step("table", () -> Jdbc.exec(c, "CREATE TABLE QTEMP.QSSENS (ID INTEGER NOT NULL, NAME CHAR(8) NOT NULL)"));
            Hs.step("sensitive", () -> run(c, ResultSet.TYPE_SCROLL_SENSITIVE, "sensitive"));
            Hs.step("insensitive", () -> run(c, ResultSet.TYPE_SCROLL_INSENSITIVE, "insensitive"));
            Hs.step("sensitive order", () -> {
                try (Statement s = c.createStatement(ResultSet.TYPE_SCROLL_SENSITIVE, ResultSet.CONCUR_READ_ONLY);
                    ResultSet r = s.executeQuery("SELECT ID, NAME FROM QTEMP.QSSENS ORDER BY NAME")) {
                    Hs.out("     type=" + r.getType());
                }
            });
            Hs.step("sensitive join", () -> {
                try (Statement s = c.createStatement(ResultSet.TYPE_SCROLL_SENSITIVE, ResultSet.CONCUR_READ_ONLY);
                    ResultSet r = s.executeQuery("SELECT A.ID, B.NAME FROM QTEMP.QSSENS A JOIN QTEMP.QSSENS B ON A.ID = B.ID")) {
                    Hs.out("     type=" + r.getType());
                }
            });
            Hs.step("drop", () -> Jdbc.exec(c, "DROP TABLE QTEMP.QSSENS"));
        }
    }
}
