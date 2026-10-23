# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.

"""Build the pub400 variant of the QSTEST kit and the Xfer script that
loads, runs and fetches it. Library CWELLMANB, IFS /home/CWELLMAN/qstest."""
import pathlib, re, shutil

repo = pathlib.Path(__file__).resolve().parents[2]
wd = pathlib.Path(__file__).parent
kit = wd / "kit"
if kit.exists():
    shutil.rmtree(kit)
for d in ("dds", "data", "oracle"):
    (kit / d).mkdir(parents=True)

LIB = "CWELLMANB"
HOME = "/home/CWELLMAN/qstest"

def port(text):
    text = text.replace("/home/QSTEST", HOME)
    text = text.replace("QSTEST.LIB", LIB + ".LIB")
    text = re.sub(r"\bQSTEST/", LIB + "/", text)
    text = text.replace("LIB(QSTEST)", "LIB(" + LIB + ")")
    return text

for f in (repo / "dds").iterdir():
    # unqualified: CWELLMANB is first on the library list, and the longer
    # name would push JFILE past column 80
    dds = re.sub(r"\bQSTEST/", "", f.read_text(encoding="utf-8"))
    (kit / "dds" / f.name).write_text(dds, encoding="utf-8", newline="\n")
for f in (repo / "data").iterdir():
    shutil.copy(f, kit / "data" / f.name)
(kit / "oracle" / "RLATEST.RPGLE").write_text(
    port((repo / "oracle" / "RLATEST.RPGLE").read_text(encoding="utf-8")), encoding="utf-8", newline="\n")
(kit / "oracle" / "QRESULTS.SQL").write_text(
    port((repo / "oracle" / "QRESULTS.SQL").read_text(encoding="utf-8")), encoding="utf-8", newline="\n")

cl = (repo / "oracle" / "QSTEST.CLLE").read_text(encoding="utf-8").replace("\r\n", "\n")
cl = port(cl)
# no CRTLIB on this account: the kit library is cleared and put on the list
cl = cl.replace(
    "  DLTLIB LIB(" + LIB + ")\n  MONMSG MSGID(CPF0000)\n  CRTLIB LIB(" + LIB + ") TEXT('Database conformance kit')\n",
    "  CLRLIB LIB(" + LIB + ")\n  MONMSG MSGID(CPF0000)\n  ADDLIBLE LIB(" + LIB + ") POSITION(*FIRST)\n  MONMSG MSGID(CPF0000)\n")
assert "CLRLIB" in cl, "library lines not found"
# 7.5 refuses to convert into the hex field FH (CCSID 65535); TYPES comes
# from the 7.1 capture, so its import may fail here
m = re.search(r"(  CPYFRMIMPF FROMSTMF\('[^']*/TYPES\.csv'\)[^\n]*\n)", cl)
assert m, "TYPES import not found"
cl = cl.replace(m.group(1), m.group(1) + "  MONMSG MSGID(CPF2817)\n")
# the RPG compiler needs an EBCDIC target for a UTF-8 stream
cl = cl.replace("RLATEST.RPGLE')\n", "RLATEST.RPGLE') TGTCCSID(*JOB)\n")
assert "TGTCCSID(*JOB)" in cl
# 7.5 writes the column names itself
for name in ("rla.csv", "ffd.csv", "lvlid.csv", "dspjrn.csv"):
    cl = re.sub(r"(TOSTMF\('" + re.escape(HOME) + r"/out/" + re.escape(name) + r"'\)[^\n]*RCDDLM\(\*LF\))",
                r"\1 ADDCOLNAM(*SYS)", cl)
# the 7.1 header step is not needed
start = cl.find("  /* Column names of the outfiles")
end = cl.find("ENDPGM")
if start != -1:
    cl = cl[:start] + cl[end:]
(kit / "oracle" / "QSTEST.CLLE").write_text(cl, encoding="utf-8", newline="\n")

# the transfer script
lines = [
    "cmd\tQSH CMD('mkdir -p " + HOME + "/dds " + HOME + "/data " + HOME + "/oracle " + HOME + "/out/data " + HOME + "/out/sql')",
]
for sub in ("dds", "data", "oracle"):
    for f in sorted((kit / sub).iterdir()):
        lines.append("iput\t" + str(f) + "\t" + HOME + "/" + sub + "/" + f.name)
lines += [
    "cmd\tDLTPGM PGM(CWELLMAN1/QSTESTPUB)",
    "cmd\tCRTBNDCL PGM(CWELLMAN1/QSTESTPUB) SRCSTMF('" + HOME + "/oracle/QSTEST.CLLE')\tstop",
    "cmd\tCALL PGM(CWELLMAN1/QSTESTPUB)",
    "log\t60",
]
outs = ["rla.csv", "dspjrn.csv", "ffd.csv", "lvlid.csv"]
outs += ["data/" + n + ".csv" for n in ("CUSTOMER", "ORDERS", "ORDLINE", "PRODUCT", "INVENTORY", "EVENTS", "TYPES", "NOTES", "LOGHIST", "RLALOG", "CUSBYNAME", "ORDBYCUS", "PRDBYDESC", "ORDJOIN", "ACTIVE", "BIGORD")]
outs += ["sql/Q%02d.csv" % i for i in range(1, 13)]
got = wd / "got"
for o in outs:
    (got / o).parent.mkdir(parents=True, exist_ok=True)
    lines.append("iget\t" + HOME + "/out/" + o + "\t" + str(got / o))
(wd / "run.xfer").write_text("\n".join(lines) + "\n", encoding="utf-8")
print("kit", sum(1 for _ in kit.rglob("*") if _.is_file()), "files;", len(lines), "script lines")
