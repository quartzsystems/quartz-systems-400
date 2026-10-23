"""The database files of batches 6 and 7: DDS sources in rpg/dds/ and
the SQL scripts that fill them. Run from tools/conformance/rpg/gen."""

import os

from fixed import A

HERE = os.path.dirname(os.path.abspath(__file__))
DDS = os.path.join(HERE, "..", "dds")


def write(name, lines):
    with open(os.path.join(DDS, name), "w", newline="\n") as f:
        f.write("\n".join(lines) + "\n")


FILES = {
    # Customers, unique key.
    "CUSTF.pf": [
        A(kw="UNIQUE"),
        A(kw="CCSID(37)"),
        A("CUSTR", "R"),
        A("CUSNO", length=5, typ="P", dec=0),
        A("NAME", length=20, typ="A"),
        A("CITY", length=12, typ="A"),
        A("BAL", length=9, typ="P", dec=2),
        A("CUSNO", "K"),
    ],
    # Customers by city, then number.
    "CUSTCITY.lf": [
        A("CUSTR", "R", kw="PFILE(CUSTF)"),
        A("CITY", "K"),
        A("CUSNO", "K"),
    ],
    # Orders, keyed by customer and order number.
    "ORDF.pf": [
        A(kw="CCSID(37)"),
        A("ORDR", "R"),
        A("ORDCUS", length=5, typ="P", dec=0),
        A("ORDNO", length=5, typ="P", dec=0),
        A("ITEM", length=10, typ="A"),
        A("QTY", length=5, typ="P", dec=0),
        A("PRICE", length=7, typ="P", dec=2),
        A("ORDCUS", "K"),
        A("ORDNO", "K"),
    ],
    # History in arrival sequence.
    "HISTF.pf": [
        A(kw="CCSID(37)"),
        A("HISTR", "R"),
        A("HCUS", length=5, typ="P", dec=0),
        A("HTEXT", length=15, typ="A"),
        A("HAMT", length=7, typ="P", dec=2),
    ],
    # One field of each kind.
    "TYPESF.pf": [
        A(kw="CCSID(37)"),
        A("TYPR", "R"),
        A("TKEY", length=3, typ="S", dec=0),
        A("TZON", length=7, typ="S", dec=2),
        A("TBIN", length=9, typ="B", dec=0),
        A("TBIN2", length=4, typ="B", dec=2),
        A("TDATE", typ="L", kw="DATFMT(*ISO)"),
        A("TTIME", typ="T", kw="TIMFMT(*ISO)"),
        A("TSTMP", typ="Z"),
        A("TVAR", length=20, typ="A", kw="VARLEN"),
        A("TKEY", "K"),
    ],
    # Sales in region and rep order, for the cycle.
    "SALESF.pf": [
        A(kw="CCSID(37)"),
        A("SALESR", "R"),
        A("REGION", length=2, typ="A"),
        A("REP", length=4, typ="A"),
        A("ITEM", length=10, typ="A"),
        A("AMT", length=7, typ="P", dec=2),
        A("QTY", length=3, typ="P", dec=0),
    ],
    # Transactions with a record type in position 1.
    "TRANF.pf": [
        A(kw="CCSID(37)"),
        A("TRANR", "R"),
        A("TCODE", length=1, typ="A"),
        A("TDATA", length=19, typ="A"),
    ],
    # Masters and their activity, both in number order.
    "MSTF.pf": [
        A(kw="CCSID(37)"),
        A("MSTR", "R"),
        A("MCUS", length=3, typ="A"),
        A("MNAME", length=10, typ="A"),
    ],
    "ACTF.pf": [
        A(kw="CCSID(37)"),
        A("ACTR", "R"),
        A("ACUS", length=3, typ="A"),
        A("AAMT", length=5, typ="S", dec=0),
    ],
}

SQL = {
    "CUSTF.sql": """
INSERT INTO {lib}.CUSTF VALUES
 (101, 'Alice Adams', 'Boston', 1250.50),
 (102, 'Bob Brown', 'Chicago', -75.25),
 (103, 'Carol Clark', 'Boston', 0),
 (104, 'Dan Davis', 'Denver', 300),
 (105, 'Eve Evans', 'Chicago', 99.99),
 (106, 'Frank Fox', 'Denver', 5000),
 (107, 'Grace Green', 'Boston', 12.34),
 (108, 'Hank Hill', 'Austin', 450);
""",
    "ORDF.sql": """
INSERT INTO {lib}.ORDF VALUES
 (101, 1, 'Widget', 5, 2.50),
 (101, 2, 'Gadget', 1, 19.99),
 (101, 3, 'Gizmo', 10, 0.75),
 (102, 1, 'Widget', 2, 2.50),
 (104, 1, 'Sprocket', 7, 3.10),
 (104, 2, 'Widget', 1, 2.50),
 (107, 1, 'Gadget', 3, 19.99),
 (107, 2, 'Bolt', 100, 0.05);
""",
    "HISTF.sql": """
INSERT INTO {lib}.HISTF VALUES
 (101, 'Opened', 100.00),
 (102, 'Payment', -25.50),
 (101, 'Refund', -10.00),
 (103, 'Opened', 0),
 (104, 'Purchase', 42.42);
""",
    "TYPESF.sql": """
INSERT INTO {lib}.TYPESF VALUES
 (1, 12.34, 123456789, 12.34, '2026-10-23', '13.45.00',
  '2026-10-23-13.45.00.123456', 'short'),
 (2, -0.05, -42, -0.50, '1999-12-31', '00.00.01',
  '1999-12-31-23.59.59.000000', 'a longer value here'),
 (3, 0, 0, 0, '2000-02-29', '23.59.59',
  '2000-02-29-12.00.00.000001', '');
""",
    "SALESF.sql": """
INSERT INTO {lib}.SALESF VALUES
 ('E1', 'R001', 'Widget', 25.00, 10),
 ('E1', 'R001', 'Gadget', 199.90, 10),
 ('E1', 'R001', 'Gizmo', 7.50, 10),
 ('E1', 'R002', 'Widget', 12.50, 5),
 ('E1', 'R002', 'Bolt', 1.25, 25),
 ('N1', 'R003', 'Sprocket', 31.00, 10),
 ('W1', 'R004', 'Widget', 50.00, 20),
 ('W1', 'R004', 'Gadget', 39.98, 2),
 ('W1', 'R005', 'Gizmo', 0.75, 1),
 ('W1', 'R005', 'Bolt', 5.00, 100),
 ('W1', 'R005', 'Widget', 2.50, 1);
""",
    "TRANF.sql": """
INSERT INTO {lib}.TRANF VALUES
 ('H', '101Alice Adams'),
 ('D', '0002500Widget'),
 ('D', '0001999Gadget'),
 ('H', '104Dan Davis'),
 ('D', '0000310Sprocket'),
 ('X', 'ignored record'),
 ('H', '107Grace Green'),
 ('D', '0001000Bolt');
""",
    "MSTF.sql": """
INSERT INTO {lib}.MSTF VALUES
 ('001', 'Alpha'),
 ('003', 'Gamma'),
 ('004', 'Delta'),
 ('007', 'Eta');
""",
    "ACTF.sql": """
INSERT INTO {lib}.ACTF VALUES
 ('001', 100),
 ('001', 250),
 ('002', 999),
 ('004', 40),
 ('007', 7),
 ('007', 70),
 ('008', 8);
""",
}

if __name__ == "__main__":
    os.makedirs(DDS, exist_ok=True)
    for name, lines in FILES.items():
        write(name, lines)
    for name, text in SQL.items():
        write(name, text.strip().splitlines())
