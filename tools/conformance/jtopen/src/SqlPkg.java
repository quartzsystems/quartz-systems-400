// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.PreparedStatement;
import java.sql.ResultSet;

/**
 * Extended dynamic SQL: a connection with a package in QSTEST (the server
 * creates it, prepares go into it), a second connection with the package
 * cached (the client downloads it and reuses its statements), what the
 * catalog shows of it, DSPSQLPKG through the remote command server (V7R1M0
 * has no such command), and the package deleted at the end.
 */
public final class SqlPkg {
    static Connection open(boolean cache) throws Exception {
        String url = "jdbc:as400://" + Hs.host() + (Hs.skipSignon() ? ":8471" : "")
            + ";naming=sql;prompt=false;errors=full;extended dynamic=true;package=QSPKG;package library=QSTEST"
            + ";package cache=" + cache + ";package criteria=select;package error=exception";
        return DriverManager.getConnection(url, Hs.user(), Hs.pass());
    }

    static void query(Connection c, String sql, Object... parms) throws Exception {
        try (PreparedStatement p = c.prepareStatement(sql)) {
            for (int i = 0; i < parms.length; i++) {
                p.setObject(i + 1, parms[i]);
            }
            try (ResultSet r = p.executeQuery()) {
                Jdbc.dump(r, 3);
            }
        }
    }

    static void run(com.ibm.as400.access.CommandCall cc, String command) throws Exception {
        boolean ok = cc.run(command);
        Hs.out("     " + (ok ? "ran " : "failed ") + command);
        Hs.messages("msg", cc.getMessageList());
    }

    public static void main(String[] a) throws Exception {
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        try (Connection c = open(false)) {
            Hs.out("ok   connect");
            Hs.step("prepare1", () -> query(c, "SELECT TABLE_NAME, TABLE_TYPE FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = ? ORDER BY 1 FETCH FIRST 2 ROWS ONLY", "QSYS2"));
            Hs.step("prepare2", () -> query(c, "SELECT CAST(? AS INTEGER) + 1 AS N FROM SYSIBM.SYSDUMMY1", 41));
            Hs.step("again", () -> query(c, "SELECT CAST(? AS INTEGER) + 1 AS N FROM SYSIBM.SYSDUMMY1", 1));
            Hs.step("once", () -> query(c, "SELECT CAST(? AS CHAR(3)) AS C FROM SYSIBM.SYSDUMMY1", "ABC"));
            Hs.step("nomarker", () -> query(c, "SELECT IBMREQD FROM SYSIBM.SYSDUMMY1"));
            Hs.step("catalog", () -> query(c, "SELECT * FROM QSYS2.SYSPACKAGE WHERE PACKAGE_SCHEMA = 'QSTEST'"));
            Hs.step("objects", () -> query(c, "SELECT OBJNAME, OBJTYPE, OBJTEXT FROM TABLE(QSYS2.OBJECT_STATISTICS('QSTEST', '*SQLPKG')) X"));
        }
        try (Connection c = open(true)) {
            Hs.out("ok   connect cached");
            Hs.step("cached", () -> query(c, "SELECT CAST(? AS INTEGER) + 1 AS N FROM SYSIBM.SYSDUMMY1", 7));
            Hs.step("new", () -> query(c, "SELECT CAST(? AS VARCHAR(10)) AS W FROM SYSIBM.SYSDUMMY1", "HI"));
        }
        try (Connection c = DriverManager.getConnection("jdbc:as400://" + Hs.host() + (Hs.skipSignon() ? ":8471" : "") + ";naming=sql;prompt=false;errors=full", Hs.user(), Hs.pass())) {
            Hs.step("stmts", () -> query(c, "SELECT * FROM QSYS2.SYSPACKAGESTAT WHERE PACKAGE_SCHEMA = 'QSTEST' FETCH FIRST 5 ROWS ONLY"));
            com.ibm.as400.access.AS400 sys = Hs.system();
            com.ibm.as400.access.CommandCall cc = new com.ibm.as400.access.CommandCall(sys);
            String[][] listings = {
                {"DSPSQLPKG SQLPKG(QSTEST/QSPKG9899) OUTPUT(*PRINT)", "QSYSPRT"},
            };
            for (String[] l : listings) {
                Hs.step("listing " + l[0].substring(0, 9), () -> {
                    run(cc, "CRTPF FILE(QSTEST/QSPKGLST) RCDLEN(133)");
                    try {
                        run(cc, l[0]);
                        run(cc, "CPYSPLF FILE(" + l[1] + ") TOFILE(QSTEST/QSPKGLST) SPLNBR(*LAST) CTLCHAR(*FCFC)");
                        run(cc, "DLTSPLF FILE(" + l[1] + ") SPLNBR(*LAST)");
                        try (java.sql.Statement st = c.createStatement(); ResultSet r = st.executeQuery("SELECT * FROM QSTEST.QSPKGLST")) {
                            while (r.next()) {
                                Hs.out("     |" + r.getString(1).replace(Hs.user(), "{user}"));
                            }
                        }
                    } finally {
                        run(cc, "DLTF FILE(QSTEST/QSPKGLST)");
                    }
                });
            }
            sys.disconnectAllServices();
            Hs.step("delete", () -> Jdbc.exec(c, "CALL QSYS2.QCMDEXC('DLTSQLPKG SQLPKG(QSTEST/QSPKG9899)', 34)"));
            Hs.step("gone", () -> query(c, "SELECT COUNT(*) FROM QSYS2.SYSPACKAGE WHERE PACKAGE_SCHEMA = 'QSTEST'"));
        }
    }
}
