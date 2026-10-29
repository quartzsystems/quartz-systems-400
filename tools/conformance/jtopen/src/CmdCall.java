// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.AS400;
import com.ibm.as400.access.CommandCall;

/** Remote command: commands that succeed, one that fails, the messages returned. */
public final class CmdCall {
    public static void main(String[] a) {
        AS400 s = Hs.system();
        Hs.step("connect", () -> s.connectService(AS400.COMMAND));
        CommandCall c = new CommandCall(s);
        String[] cmds = {
            "CHKOBJ OBJ(QSYS/QSYSOPR) OBJTYPE(*MSGQ)",
            "SNDPGMMSG MSG('QS/400 host server trace') TOPGMQ(*SAME) MSGTYPE(*COMP)",
            "CHKOBJ OBJ(QSYS/QSNOSUCH1) OBJTYPE(*PGM)",
            "NOSUCHCMD1",
        };
        for (String cmd : cmds) {
            Hs.step("run " + cmd, () -> {
                boolean ok = c.run(cmd);
                Hs.out("     result=" + ok);
                Hs.messages("msg", c.getMessageList());
            });
        }
        Hs.step("thread safe default", () -> Hs.out("     threadsafe=" + c.isThreadSafe()));
        s.disconnectAllServices();
    }
}
