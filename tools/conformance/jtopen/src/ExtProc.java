// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import java.sql.CallableStatement;
import java.sql.Connection;
import java.sql.ResultSet;
import java.sql.Types;

/**
 * External procedures through the database server (QEMU step 137, not an
 * oracle scenario: the primary has no RPG compiler): SQLQ.EXTRPG, an RPGLE
 * program returning a result set, and SQLQ.EXTCL, a CL program setting its
 * INOUT parameter. Both are made by the step before it runs this.
 */
public final class ExtProc {
    public static void main(String[] a) throws Exception {
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        String url = "jdbc:as400://" + Hs.host() + (Hs.skipSignon() ? ":8471" : "") + ";naming=sql;prompt=false;errors=full";
        try (Connection c = Hs.jdbc(url)) {
            Hs.out("ok   connect");
            Hs.step("rpg", () -> {
                try (CallableStatement p = c.prepareCall("CALL SQLQ.EXTRPG(?)")) {
                    p.setInt(1, 2);
                    boolean rs = p.execute();
                    Hs.out("     result set first=" + rs);
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
            Hs.step("cl", () -> {
                try (CallableStatement p = c.prepareCall("CALL SQLQ.EXTCL(?)")) {
                    p.setString(1, "JDBC");
                    p.registerOutParameter(1, Types.CHAR);
                    p.execute();
                    Hs.out("     msg=[" + p.getString(1) + "]");
                }
            });
        }
    }
}
