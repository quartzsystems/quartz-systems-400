// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.AS400;
import com.ibm.as400.access.AS400Bin4;
import com.ibm.as400.access.AS400Message;
import com.ibm.as400.access.AS400Text;
import com.ibm.as400.access.ProgramCall;
import com.ibm.as400.access.ProgramParameter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * Writes the command definition (QCDRCMDD, CMDD0100: the command's XML) of each command
 * named into DIR/NAME.xml. Usage: CmdDef DIR LIB/CMD... Runs through Hs (QS_HS_*).
 */
public final class CmdDef {
    private CmdDef() {}

    public static void main(String[] args) throws Exception {
        AS400 s = Hs.system();
        Path dir = Path.of(args[0]);
        Files.createDirectories(dir);
        AS400Bin4 bin4 = new AS400Bin4();
        AS400Text t20 = new AS400Text(20, s);
        AS400Text t8 = new AS400Text(8, s);
        for (int i = 1; i < args.length; i++) {
            String[] q = args[i].split("/");
            String name = String.format("%-10s%-10s", q[1], q[0]);
            int len = 200000;
            ProgramParameter[] p = {
                new ProgramParameter(t20.toBytes(name)),
                new ProgramParameter(bin4.toBytes(len)),
                new ProgramParameter(t8.toBytes("DEST0100")),
                new ProgramParameter(len),
                new ProgramParameter(t8.toBytes("CMDD0100")),
                new ProgramParameter(bin4.toBytes(0), 8),
            };
            ProgramCall pc = new ProgramCall(s, "/QSYS.LIB/QCDRCMDD.PGM", p);
            if (!pc.run()) {
                System.out.println("fail " + args[i]);
                for (AS400Message m : pc.getMessageList()) {
                    System.out.println("     " + m.getID() + " " + m.getText());
                }
                continue;
            }
            byte[] out = p[3].getOutputData();
            int ret = bin4.toInt(out, 0);
            int n = Math.min(ret, out.length - 8);
            String xml = new String(out, 8, n, StandardCharsets.UTF_8).trim();
            Files.writeString(dir.resolve(q[1] + ".xml"), xml);
            System.out.println("ok " + args[i] + " " + xml.length());
        }
    }
}
