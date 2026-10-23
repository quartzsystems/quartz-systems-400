# UIM panel groups

The `uim` crate (`ui/uim`) implements this document: the panel-group
tag language as `CRTPNLGRP` and `CRTMNU TYPE(*UIM)` read it from a
source member, the model a renderer walks, the checks, the listing and
the `*PNLGRP` payload. `ui/uim/tests/spec.rs` compares the code's
layouts with the tables in §6. The renderer (§7) lays a panel out as
texts and fields; nothing in the crate makes a data stream or touches
the store; imbedded members and the exports of other groups
come through callbacks (`Imbed`, `Imports`). Every text here is ours;
the tag, attribute and action names are the compatibility interface.

## 1. Source

A source member is lines of text. The reader (`source`) numbers them
100, 200, ... within their member, replaces tabs with blanks and drops
trailing blanks. A line starting with `.IM name` imbeds another member
in place: its lines follow the `.IM` line, marked with the member's
name, numbered from 100 again, up to eight levels deep and never
recursively. `.*` at the start of a line is a comment and stays in the
listing. Any other `.word` at the start of a line is `UIM0106`.

The lexer (`lex`) finds a tag wherever a colon precedes a letter:
`:NAME attr=value attr='quoted value' flag.` The period closes the tag;
inside quotes a period and a blank are text, and `''` is one quote. A
tag may run over lines until its period. Tag and attribute names fold
to upper case, unquoted values too; quoted values keep their case. An
attribute written without `=` is a flag (`COMPACT`, `REQUIRED`), the
same as `=YES`.

Everything else is text, one token per line segment. `&name.` in text
is a built-in symbol or a dialog variable: `&amp.` `&colon.` `&period.`
`&dot.` `&lbracket.` `&rbracket.` `&lbrace.` `&rbrace.` `&comma.`
`&apos.` `&semi.` `&slr.` `&bsl.` `&splat.` are the symbols; any other
name is a variable the checks must find (`UIM2010`). An `&` that does
not start `&name.` is a literal ampersand.

## 2. Tags

The tree builder (`tree`) nests elements by the table in `tags`. Each
tag closes one of four ways: a *container* on its `:Ename.`; a *leaf*
when a tag its parent takes arrives or its parent ends; an *empty* tag
has no content; an *inline* highlight on its `:Ename.`, or with
`UIM0208` when the text around it ends. A tag under a parent that does
not take it is `UIM0204`, text under an element that takes none
`UIM0205`, an unknown tag `UIM0201`, an unknown attribute `UIM0301`, a
missing required attribute `UIM0302`.

Status: **model** means the tag and its attributes are in the model
with their values parsed; **carried** means the tag is parsed and kept
as text or as a name without interpretation; a tag not in the table
is a diagnostic, never a panic.

| Tag | Closes | Attributes (required in bold) | Status |
|---|---|---|---|
| `:PNLGRP.` | `:EPNLGRP.` | SCHIDX DFTMSGF SUBMSGF TXTCHRID HLPSHELF | model |
| `:COPYR.` | leaf | | model |
| `:IMPORT.` | empty | **NAME PNLGRP** | model |
| `:HLPSHELF.` `:KEYLSHLF.` | empty | **NAME** | model |
| `:DUMMY.` | empty | | carried (ignored) |
| `:CLASS.` | `:ECLASS.` | **NAME BASETYPE** WIDTH SHIFT CASE CHRID ECPARAM | model |
| `:TL.` | `:ETL.` | MSG | model |
| `:TI.` | leaf | **VALUE** | model |
| `:CHECK.` | empty | **TYPE** MSG VAR MSGID | carried (TYPE as text) |
| `:VAR.` | empty | **NAME CLASS** VALUE SIZE | model |
| `:VARRCD.` | empty | **NAME VARS** NOGET NOPUT EXTVARS | model |
| `:LISTDEF.` | empty | **NAME VARS** MSGID | model |
| `:COND.` | empty | **NAME EXPR** MSG | model (§4) |
| `:TT.` | `:ETT.` | **NAME CONDS** | model |
| `:TTROW.` | empty | **VALUES** (`1`, `0`, `X` per condition) | model |
| `:KEYL.` | `:EKEYL.` | **NAME** HELP | model |
| `:KEYI.` | leaf | **KEY ACTION** HELP VARUPD COND | model |
| `:PANEL.` | `:EPANEL.` | **NAME KEYL** HELP ENTER DEPTH WIDTH PANELID TOPSEP BOTSEP TOPINST CSRVAR CSRLST HELPTITLE PRVPNL NXTPNL ENBGUI KEYLSHLF | model |
| `:TOPINST.` `:BOTINST.` | leaf | | model |
| `:CMDLINE.` | leaf | SIZE NAME | model |
| `:DATA.` | `:EDATA.` | **DEPTH** LAYOUT BOTSEP COMPACT SCROLL TOPINST HELP | model |
| `:DATAGRP.` | `:EDATAGRP.` | GRPSEP HELP COMPACT NOGUI | model |
| `:DATACOL.` | empty | **WIDTH** | model |
| `:DATAI.` | leaf | **VAR USAGE** HELP NAME BASETYPE AUTOSKIP DISPLAY NOCHECK REQUIRED MAXWIDTH MINWIDTH SHIFT CSRLOC NOPUTOVR | model |
| `:DATAIX.` | empty | **VAR USAGE** NAME HELP MAXWIDTH MINWIDTH DISPLAY AUTOSKIP NOCHECK REQUIRED SHIFT CSRLOC NOPUTOVR | model (joins the `:DATAI.` before it) |
| `:DATAIXT.` | leaf | | model (text of the extender before it) |
| `:DATAC.` | leaf | NAME HELP DISPLAY WIDTH | model |
| `:DATASLT.` | `:EDATASLT.` | **VAR** HELP USAGE NAME DISPLAY REQUIRED CSRLOC SLTTYPE NOCHECK | model |
| `:DATASLTC.` | leaf | **VALUE** HELP COND DISPLAY | model |
| `:LIST.` | `:ELIST.` | **DEPTH LISTDEF** VIEWS ACTOR EXTACT MAXHEAD PARMS SCROLL BOTINST ENTER LAYOUT SELECT ACTLEN HELP | model |
| `:LISTACT.` | leaf | OPTION ENTER HELP CONFIRM USREXIT ACTFOR RETURN VARUPD ENTERMSG | model |
| `:LISTCOL.` | leaf | **VAR USAGE** MAXWIDTH HELP NAME DISPLAY DSPCOL NOGUI AUTOSKIP MINWIDTH | model |
| `:LISTGRP.` | `:ELISTGRP.` | COL HELP NOGUI | model |
| `:LISTVIEW.` | empty | **COLS** LAYOUT | model |
| `:MENU.` | `:EMENU.` | **DEPTH** SCROLL BOTSEP TOPSEP OPTIONS HELP | model |
| `:MENUGRP.` | `:EMENUGRP.` | HELP | model |
| `:MENUI.` | leaf | **OPTION ACTION** HELP COND VARUPD | model |
| `:INFO.` | `:EINFO.` | **DEPTH** SCROLL BOTSEP HELP | model (document) |
| `:TEXT.` | `:ETEXT.` | DEPTH BOTSEP | model (document) |
| `:PRTPNL.` | `:EPRTPNL.` | **NAME** HELP WIDTH DEPTH TOPSEP BOTSEP | model |
| `:PRTHEAD.` `:PRTTRAIL.` | `:EPRTHEAD.` `:EPRTTRAIL.` | | model (document) |
| `:HELP.` | `:EHELP.` | **NAME** TITLE WIDTH DEPTH | model |
| `:H1.` to `:H4.` | leaf | | model |
| `:P.` `:PC.` | leaf | | model |
| `:XMP.` `:LINES.` `:FIG.` | `:EXMP.` `:ELINES.` `:EFIG.` | | model (lines kept) |
| `:UL.` `:OL.` `:SL.` | `:EUL.` `:EOL.` `:ESL.` | COMPACT | model |
| `:LI.` | leaf (nests lists) | | model |
| `:DL.` | `:EDL.` | COMPACT TSIZE BREAK | model (TSIZE, BREAK carried) |
| `:DTHD.` `:DDHD.` `:DT.` `:DD.` | leaf | | model |
| `:PARML.` | `:EPARML.` | | model |
| `:PT.` `:PD.` | leaf | | model |
| `:NT.` `:NOTE.` | `:ENT.` `:ENOTE.` | | model |
| `:IMHELP.` | empty | **NAME** | model (reference checked) |
| `:ISCH.` | leaf | **ROOTS** | model |
| `:ISCHSYN.` | `:EISCHSYN.` | **ROOT** | model |
| `:HP0.` to `:HP9.` | inline | | model |
| `:PK.` `:PV.` `:RT.` | inline | DEF on `:PK.` | model (DEF carried) |
| `:LINK.` | inline | **PERFORM** | model (PERFORM as text) |

Where text goes: a container's text before its first child is its
title (`:PANEL.`, `:DATA.`, `:DATAGRP.`, `:LISTGRP.`, `:MENU.`,
`:MENUGRP.`, `:HELP.`, `:PRTPNL.`, `:DATASLT.`); a leaf's text is its
prompt, heading or key text; `:INFO.`, `:TEXT.`, `:NT.`, `:LI.`,
`:DD.`, `:PD.`, `:PRTHEAD.` and `:PRTTRAIL.` hold help-style blocks.
Flowed text (paragraphs, prompts, headings) is trimmed per line and
joined with one blank; `:XMP.`, `:LINES.` and `:FIG.` keep one entry
per source line, blank lines included, with leading blanks.

Actions (`ACTION`, `ENTER`, `USREXIT`): `RETURN n`, `ENTER`, `HELP`,
`HELP name`, `EXTHELP`, `KEYSHELP`, `ABOUT`, `EXIT`, `CANCEL`, `PRINT`,
`CMDLINE`, `PRVPNL`, `NXTPNL`, `PAGEUP`, `PAGEDOWN`, `PULLDOWN`,
`ACTIONS`, `RETRIEVE`, `MOREKEYS`, `FLDHELP`, `TOP`, `BOTTOM`, `HOME`,
`PRVPAGE`, `NXTPAGE`, `CALL var`, `PANEL name`, `MENU name`, `MSG var`,
`CMD string`. `CALL` and `MSG` name a dialog variable holding the
program or message identifier; `PANEL` names a panel of the group or an
import; `MENU` names a `*MENU` object and is not checked; `CMD` carries
the rest of the value as a command string, case kept and not checked,
which the menu driver runs (a leading `?` prompts it), as the generated
command-group menus of `docs/MENUS.md` use it.

Menu options (`:MENUI OPTION=`) run 1 to 999: IBM's command-group
menus (`CMDCHG` has more than 250 items) go past 99.

Keys (`KEY`): `ENTER`, `HELP`, `PRINT`, `HOME`, `PAGEUP` (`ROLLDOWN`),
`PAGEDOWN` (`ROLLUP`), `F1` to `F24`.

Base types (`BASETYPE`) and the display width each implies: `CHAR n`
and `IGC n` (n), `BIN 15` (6), `BIN 31` (11), `PACKED d p` and
`ZONED d p` (d + 1, + 1 with decimals), `DATE` (10), `TIME` (8),
`OBJNAME`, `NAME`, `SNAME` (10), `OBJNAMEQ` (21), `ACTION` (2);
`FLOAT 4|8`, `DEC`, `PTR` and `CMDLINE` need `WIDTH`.

## 3. Model

`uim::PanelGroup` holds, in source order: the group attributes,
`imports`, `classes`, `variables`, `var_records`, `list_defs`,
`conditions` (as expression trees), `truth_tables`, `key_lists`,
`panels`, `print_panels`, `helps` and `synonyms`. Names are upper
case. Every element keeps its source line.

A `Panel` has its title, `keyl`, `enter`, `depth` (24) and `width`
(80), the separators, cursor and identifier variables, and `areas` in
order: `Data`, `List`, `Menu`, `Info`, `Text`, `CmdLine`, `TopInst`,
`BotInst`. A `DataArea` holds `DataNode`s: fields with their extenders,
constants, selection fields with their choices, groups (nested nodes)
and column breaks. A `ListArea` holds its actions, its columns (plain
or under a `ListGroup` heading) and its views. A `MenuArea` holds items
and groups of items. Information and text areas, help modules and print
headings hold a `Document`: `Block`s (`Heading`, `Para`, `ParaCont`,
`Example`, `Lines`, `Figure`, `List`, `DefList`, `ParmList`, `Note`,
`ImHelp`) over `Span`s (`Text`, `Var`, `Hp`, `Pk`, `Pv`, `Rt`, `Link`).

Defaults filled where the reference gives one: `DEPTH` 24 and `WIDTH`
80 on a panel, 66 and 132 on a print panel; `TOPSEP`/`BOTSEP`/`GRPSEP`
`SPACE` (`RULE`, `NONE`, and on a panel `SYSNAM`, the `System:` line
of R2); `LAYOUT` 1; `SCROLL` `NO`; `DISPLAY` `YES`; `CMDLINE SIZE`
`LONG`; `CASE` `MIXED`; `SHIFT` `NONE`; a `:LISTCOL.` without `NAME`
is named after its variable.

Compiling: `uim::compile(text, &imbeds, &imports, &opts)` returns a
`Compilation` with the expanded `source`, the `group` (whenever a
`:PNLGRP.` was found, errors or not), the `diagnostics` and the
`listing`; `ok()` is true with no message of severity 20 or more, and
`payload()` then gives the bytes of §6. `PanelGroup::exports()` lists
the help modules and panels another group may import.

## 4. Expressions

`:COND EXPR=` is parsed by `uim::expr`:

```text
expr    := and ( OR and )*
and     := not ( AND not )*
not     := NOT not | cmp
cmp     := operand ( (EQ|NE|GT|LT|GE|LE) operand )?
operand := '(' expr ')' | %LIST '(' name ')' | %NODE '(' name ')'
         | &name. | "string" | 'string' | number | name
```

`&name.` (the period optional) and a bare name are dialog variables;
`'string'` is written `''string''` inside the attribute's quotes;
`%LIST(name)` is the number of entries in a list, `%NODE(name)` whether
the list's current node is set. Variables and lists an expression names
are checked. `Expr` keeps the tree; evaluation is the renderer's.

## 5. Checks and diagnostics

Severities: 10 warning (the group is created), 20 error (not created),
30 severe (an element was dropped), 40 terminal (nothing to compile).

| Identifier | Sev | When |
|---|---|---|
| UIM0101 | 30 | `.IM` member not found |
| UIM0102 | 30 | `.IM` too deep or recursive |
| UIM0103 | 20 | tag not closed by a period before the end of the source |
| UIM0104 | 20 | quoted attribute value not closed |
| UIM0105 | 10 | tab replaced by a blank |
| UIM0106 | 20 | control word other than `.*` and `.IM` |
| UIM0107 | 20 | stray character inside a tag |
| UIM0108 | 30 | `.IM` without a name |
| UIM0201 | 20 | unknown tag |
| UIM0202 | 20 | end tag with no open element |
| UIM0203 | 20 | element not closed before its parent's end or the end of the source |
| UIM0204 | 20 | tag not allowed under its parent |
| UIM0205 | 20 | text where none is allowed |
| UIM0206 | 40 | no `:PNLGRP.` |
| UIM0207 | 20 | a second `:PNLGRP.` |
| UIM0208 | 20 | highlight left open when its text ends |
| UIM0301 | 10 | unknown attribute |
| UIM0302 | 20 | required attribute missing |
| UIM0303 | 20 | attribute value not valid |
| UIM0304 | 10 | attribute repeated |
| UIM0305 | 20 | flag written where a value is needed |
| UIM1001 | 20 | duplicate name |
| UIM1002 | 20 | name not valid |
| UIM1003 | 20 | `BASETYPE` not valid |
| UIM1004 | 20 | expression does not parse |
| UIM1005 | 20 | truth table row width |
| UIM1006 | 20 | option used twice in a list or menu |
| UIM1007 | 20 | menu option not 1 to 999 or `*` |
| UIM1008 | 20 | list without columns or views |
| UIM1009 | 20 | view names an unknown column |
| UIM1010 | 20 | input field whose class has no width |
| UIM1011 | 20 | area depths exceed the panel |
| UIM1012 | 20 | key listed twice |
| UIM1013 | 20 | `*` option without a command line |
| UIM1014 | 20 | `:LISTACT.` with neither `OPTION` nor `ENTER` |
| UIM1015 | 10 | `VIEWS` disagrees with the `:LISTVIEW.` count |
| UIM1016 | 20 | action not valid |
| UIM1017 | 20 | key not valid |
| UIM1018 | 20 | `:DATAIX.`/`:DATAIXT.` with no field before it |
| UIM1019 | 20 | list option not 1 to 99 or one letter |
| UIM2001-2009 | 20 | class, variable, list, condition, key list, help module, panel, variable record, truth table not defined |
| UIM2010 | 20 | `&name.` neither a symbol nor a variable |
| UIM2011 | 20 | import of a name the other group does not export |
| UIM3001 | 10 | panel with no areas |
| UIM3002 | 20 | second command line |
| UIM3003 | 20 | panel size not 24x80, 27x132 or 24x132 |
| UIM3004 | 10 | help module with no text |

Depth rule: the fixed `DEPTH`s of a panel's areas, plus one line per
`DEPTH='*'` area, must fit in the panel's depth less three lines (title
and two key lines), less the command line (one for `SHORT`, two for
`LONG`) and one per `:TOPINST.`/`:BOTINST.`.

Help references: `HELP=` on any tag, `:IMHELP.`, `ACTION='HELP name'`
and `KEYL HELP=` must name a help module of the group or an import. A
qualified name (`grp/name`) is taken on trust. With `:IMPORT NAME=*`,
the `Imports` resolver supplies the other group's exports; when it
cannot (`None`), every imported name is taken on trust.

The listing shows the header, the source with sequence numbers and the
imbed member column, messages under their lines, the counts (panels,
print panels, help modules, classes, variables, conditions, key lists,
imports), every message again with its sequence number, the totals by
severity and whether the group is created.

## 6. Payload

The `*PNLGRP` object's space holds a `PnlgrpHdr`, then `PnlgrpTail`
records to the end, each followed by `Len` bytes. Text in the header
is CP037, blank padded; integers are little-endian. Record tags: 1 the
source lines (`ipc::Wire`: `Vec<SourceLine>`, each `seq` u32, `text`,
`imbed` option), 2 the model (`ipc::Wire` of `PanelGroup`, structs
field by field, enums as a one-byte tag; `ui/uim/src/payload/codec.rs`
fixes the tags). A reader skips tags it does not know.

### `PnlgrpHdr` (64 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | Magic | `QSUIPNLG` in ASCII |
| 8 | 2 | Version | 1 |
| 10 | 2 | Flags | 0 |
| 12 | 10 | Name | the panel group |
| 22 | 10 | Library | |
| 32 | 2 | PanelCount | |
| 34 | 2 | PrtPanelCount | |
| 36 | 2 | HelpCount | |
| 38 | 2 | VarCount | |
| 40 | 2 | ClassCount | |
| 42 | 2 | CondCount | |
| 44 | 2 | KeylCount | |
| 46 | 2 | ImportCount | |
| 48 | 4 | SourceLines | lines in record 1 |
| 52 | 12 | Reserved | zero |

### `PnlgrpTail` (8 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | Tag | 1 source, 2 model |
| 2 | 2 | Reserved | zero |
| 4 | 4 | Len | bytes that follow |

## 7. Renderer rules

`uim::render` lays a compiled panel and the dialog's values out as a
`Screen`: texts at 1-based positions with an attribute, input fields
with their width and target, the cursor and the paging indicator.
Nothing is written to column 1 or the last column; the data stream
crate puts attribute bytes there. The rules were settled against the
pub400 goldens (`tools/conformance/golden/verified/pub400`, 2026-10-23);
each names the goldens that establish it. `W` is the panel width (80
or 132) and the last usable column is `W-1`.

| # | Rule | Goldens |
|---|---|---|
| R1 | The title is on row 1 from column `1 + (W - len)/2`, high intensity; the `PANELID` variable's value is at column 2 of row 1, blue. | all; main-menu, major-menu, cmdwrk-menu, information-assistant |
| R2 | Row 2 is `TOPSEP`: `SPACE` blank, `SYSNAME` puts `System:` at column `W-18` and the name at `W-8` (normal), `RULE` dashes, `NONE` no row (the areas start on row 2). | wrksbs, wrkrpyle, wrkjob, dspjobd, dspmsgd-5, main-menu; wrkobj, wrkjobd, wrkcls blank |
| R3 | The bottom block is laid out upward from the message line (the last row): the function keys on one row or two above it; with a command line, its field rows (one for `SHORT`, two for `LONG`) and its prompt row sit directly on the keys; without one, `BOTSEP` (a blank row) sits on the keys and the `BOTINST` or the menu selection rows above it. The areas share the rows between the title block and the bottom block. | wrkobj 20-21/22-23, main-menu 19-21/22-23, cmdwrk-menu 20-22/23, dspjobd 21/23, dspobjaut 20/22-23, dspmsgd-list 20-21/23 |
| R4 | Instruction rows (`TOPINST`, `BOTINST`, the menu heading) are at column 2, blue. | wrkobj 3, dspjobd 21, main-menu 3 |
| R5 | Option legends: the `:LISTACT.` texts with an `OPTION`, in order from column 4, blue, three blanks apart, wrapping to column 4 of the next row when one would pass `W-1`; two rows align as R7. | wrkobj, wrkjobd, wrkcls, wrkbnddir, wrksbs |
| R6 | Function keys: the key list's items with text, `COND` true, from column 2 as R5; at most two rows, a single row on the row above the message line. A `MOREKEYS` item splits the list: the items before it and itself, or with `more_keys` the items after it and itself. | wrkjobd, wrkobj, wrkrpyle, wrksbs, main-menu, main-menu-help |
| R7 | Column alignment of two flowed rows (R5, R6): after the greedy wrap, for each item index both rows hold, the row whose item starts earlier moves that item and what follows it to the other's column when the row still ends within `W-1`; otherwise both stay. | wrkobj legends 4/28/37/48/70 over 4/28; wrkjobd, wrkcls, wrkbnddir keys 2/15/27/40/54 over 2/15/40; wrkusrjob 2/15/27/40/54 over 2/15/27/40; wrkobj keys greedy 2/12/24/37/51; wrkrpyle, wrksbs one lower item |
| R8 | A list area holds, top down: the legend rows, a blank row, the heading rows (`MAXHEAD`, else as many as the widest wrapped heading needs, plus one for a `:LISTGRP.`), the entries, and the indicator on its last row. | wrkobj 4-19, wrkjobd 4-19, wrkbnddir 4-19 |
| R9 | List columns start at column 2 and are two columns apart; a column is as wide as the larger of its field (`MAXWIDTH`, else the class) and its widest heading row; the heading wraps at the field width and its rows sit left-aligned at the column start, bottom-aligned; a `:LISTGRP.` heading is on the first heading row at its first column; the `ACTION` column (`Opt`) has its field centered (two wide at column 2, one wide at column 3) with `ACTLEN` or the class as the width; numbers are right-aligned in their field; the last column is cut at `W-1`. Headings are high intensity, values normal, input columns underlined. | wrkobj 2/7/19/29/41/53, wrkjobd 7/20/32 with the one-wide field at 3, wrkbnddir 7/19/31, wrkcls, wrkrpyle |
| R10 | The indicator is `More...` ending at `W-1` (column 73) or `Bottom` (column 74), high intensity, on the area's last row: always on a list (`More...` while entries follow the page or the list is incomplete with a full page); on a menu, data or information area only when it has more than one page. | wrkobj 19, dspjobd 20, dspsysval none, major-menu 18, menu-major-2 Bottom, main-menu none |
| R11 | The command line: the `:CMDLINE.` text at column 2 (normal), else `Selection or command` on a menu panel and `Parameters or command` elsewhere; `===>` at column 2 of the next row (normal); the field from column 7 to `W-1`, going on to `W-1` of the row after for `LONG`. The form `Parameters for options 5, 7 and 13 or command` is the author's text (`options_prompt` builds it: commas, then `and`). | wrkobj, wrkjobd, wrkcls, wrkbnddir, main-menu, cmdwrk-menu; wrksplf's `1, 2, 3` is a display file |
| R12 | A menu panel without a command line: `Selection` (or the `BOTINST` text) at column 2, normal, with the selection field at column 7 of the next row, as wide as the longest option number. | dspmsgd-list 20-21, information-assistant 20-21 |
| R13 | A menu area: its heading row (blue), a blank row, then the items: the option right-aligned in columns 5-7, a period at 8, the text at 10 (normal); a `:MENUGRP.` heading at column 4 on its own row after a blank one; a blank row where the option numbers skip; a trailing name after two blanks (or one of four characters or more after one blank and lower-case text) goes to column `W-10`; an item without text is a blank row that keeps its number; items with `COND` false are left out. Pages hold the item rows less one, and none starts with a blank row. | main-menu, major-menu, cmdwrk-menu, dspjob-menu |
| R14 | A data item: the label at column 2, dot leaders to the colon at `C`, the value at `C+4`; `C` is the longest label's end column plus 12 over the whole area (every page); the dots sit at the colon's parity from the first such column two past the label; numbers are right-aligned in their width; `:TL.` values show their text. | dspsysval 25, dspusrprf 45; dspmsgd-5 31 and dspcls 52 measure plus 13 (see §8) |
| R15 | Groups: a `:DATAGRP.` with a heading shows `Heading:` at its indent and its items two columns further in with values at `C+4`; a group without a heading indents its items' labels and values by two (`C+6`); `GRPSEP` goes before a group that does not start the area (`SPACE` a blank row, `RULE` dashes); an empty `:DATAC.` is a blank row. | dspjob-status 9-11 and 12-14 (`Entered system:`), 7-8 (`Set by`); dspjobd 13-14; dspmsgd-5 |
| R16 | A `COMPACT` group is an identification block without leaders: `Label:`, three blanks, the field, then five blanks before each extender's `Text:`, three blanks and its field. Extenders of a normal item follow its field after three blanks. | dspjobd 3, dspjob-status 3, wrkjob 3; dspneta 14 |
| R17 | `:DATACOL WIDTH=n.` starts a column that wide at the previous column's end; without one, `LAYOUT=2` or `3` splits the items into equal columns over `W-1`; each column has its own `C` and cuts at its end. | dspmsg 3-5, dspobjaut 3-5 |
| R18 | The message line is the last row, high intensity: the message, else the copyright while `show_copyright`. | main-menu, wrkrpyle, dspautusr; wrkobj without |
| R19 | Conditions: `:KEYI.`, `:MENUI.` and `:DATASLTC.` with a `COND` that evaluates false are left out. The tree evaluates with the dialog's values: `%LIST(l)` is the list's count, `%NODE(l)` whether it has entries, a comparison is numeric when both sides are numbers and by characters with trailing blanks ignored otherwise; a condition the group does not define is asked of the dialog. `:LISTACT.` has no `COND`. | |
| R20 | A help panel: `Help` at column 2 of row 1 (blue) with the module's title centered (high intensity); the text from row 3: headings at column 2 (high intensity), paragraphs at column 6 wrapped so no row passes column `W-7`, a blank row between blocks, list markers (`o`, `1.`) at column 6, definition and parameter terms at column 6 with their text at column 16, `Note:` at column 6; 18 text rows per page (`W` 80, 24 rows); the indicator on row 21; `F3=Exit help   F10=Move to top   F12=Cancel   F13=Information Assistant` and `F14=Print help` on rows 22-23. `&name.` shows the variable's declared `VALUE`. | main-menu-help |
| R21 | Attributes: titles, headings, the indicator and the message line high intensity; instructions, legends, keys and the panel identifier blue; input fields underlined; `:HP2.`, `:PK.` high intensity, `:HP1.`, `:PV.`, `:LINK.` underlined, `:HP3.`, `:RT.` reverse, `:HP4.` blue; everything else normal. | main-menu, wrkobj, main-menu-help attribute grids |

The area heights: a `DEPTH` number is the area's rows; `DEPTH='*'`
areas share what the fixed ones leave; `:TEXT.` without a depth takes
the rows its text needs. Data, menu and information areas page with
`Options.page`; a list shows the entries of its `ListState` from
`top`; `Options.view` picks the `:LISTVIEW.`. The cursor goes to the
`CSRLOC` field, else the first option field, else the command line or
selection field, else the first input field.

## 8. The dialog engine

The job shows panels through `work/qsjob/src/display/uim.rs`: a
`UimDialog` holds a group's variables (their `VALUE`s to start), its
lists (every entry, the page's first index, the options typed on
entries) and imported conditions, implements the renderer's `Dialog`,
and `show(surface, panel)` renders the panel (§7) onto the job's
screen (`Surface`: the 5250 device, or `slic`'s console as a panel),
waits for a key and turns it into a `Response`:

- the key's `:KEYI.` action (its `COND` evaluated), Enter being the
  panel's `ENTER`;
- `EXIT` and `CANCEL` return as such; `RETURN n` returns `n`, after a
  menu's option was looked up (`MENU name`, `CMD text` and `RETURN`
  actions of the `:MENUI.`; a number no item has leaves "Selection n
  is not valid."), options typed on list entries were collected
  (`Options`, each with the `:LISTACT.` action it matched, the caller
  substituting `&VAR` from the entry and running the command), and a
  command on the command line was returned (`Command`);
- `PAGEUP`/`PAGEDOWN` move every list by the entries the page showed
  (`Screen::list_shown`) and the data, menu and information areas by a
  page; `TOP`, `BOTTOM`, `MOREKEYS` likewise stay in the engine, with
  "Already at the top/bottom of the area." when nothing moves;
- `HELP`, `EXTHELP`, `KEYSHELP` and `HELP module` show the module
  (§7 R20) until Enter, F3 or F12;
- `RETRIEVE` and `PRINT` return for the caller; `PANEL name` returns
  the panel to show next; a key not in the list is "Function key not
  allowed."; the other actions answer "That function is not available
  in this release."

A `:CMDLINE.` without `NAME` shows the dialog's `*CMDLINE` variable, so
`F9=Retrieve` can fill it. Field values come back from the screen
model by position: input variables, list cells, list options (by
entry index), the command line and the menu selection.

## 9. Open questions

- The condition grammar accepts a bare name as a variable because the
  reference's operand form is not pinned down; a probe of a real
  system settles whether `&name.` is the only form.
- `:DATAIXT.` is accepted as the text of the extender before it; its
  real attribute set is unverified.
- `CMDLINE SIZE` defaults to `LONG`; `:MENU.` text is taken as the
  area's heading. Both wait for probes.
- Help module names are checked for characters only (up to 32), panel
  and variable names for the ten-character rule.
- R14's colon column is the longest label's end plus 12 on dspsysval and
  dspusrprf, plus 13 on dspmsgd-5 and dspcls; a prompt longer than the
  visible ones (a later page, a `DISPLAY=NO` item) would explain the
  two, or the rule has a parity term. A probe with a known source
  settles it.
- Two-row column headings: WRKBNDDIR shows `Binding` over `Directory`
  left-aligned (R9); WRKJOBD shows `Job` centered over `Description`
  and WRKRPYLE `Number` one column in. Which of those panels are UIM
  and which display files is not known from the captures; R9 follows
  WRKBNDDIR.
- The keys of a set are flowed over two rows and cut there; the real
  behaviour of a set that needs a third row is unverified.
- `:DATASLT.` shows its choices as `n=text` after the field; the real
  presentation is unverified. `:LIST BOTINST=` is not rendered.
- The copyright line's rule (which panels show it) is the caller's
  (`Options.show_copyright`); the goldens show it on menus and on some
  display panels reached from them.
