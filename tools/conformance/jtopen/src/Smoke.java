// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.AS400;
import com.ibm.as400.access.AS400JDBCDriver;
import com.ibm.as400.access.CommandCall;
import java.sql.Connection;
import java.sql.ResultSet;
import java.util.Properties;

/**
 * The QEMU step's smoke test: a command and a JDBC query, through the sign-on server
 * (QS_HS_PORT_<port> moves a server to a forwarded port).
 */
public final class Smoke {
    public static void main(String[] a) {
        AS400 s = Hs.signonSystem();
        Hs.step("command", () -> {
            CommandCall c = new CommandCall(s);
            boolean ok = c.run("CHKOBJ OBJ(QSYS/QSYSOPR) OBJTYPE(*MSGQ)");
            Hs.out("     result=" + ok);
            if (!ok) {
                Hs.messages("msg", c.getMessageList());
                throw new IllegalStateException("the command failed");
            }
        });
        Hs.step("jdbc", () -> {
            try (Connection c = new AS400JDBCDriver().connect(s, new Properties(), null);
                    ResultSet r = c.createStatement().executeQuery("SELECT COUNT(*) FROM SYSIBM.SYSDUMMY1")) {
                r.next();
                Hs.out("     count=" + r.getInt(1));
            }
        });
        s.disconnectAllServices();
    }
}
