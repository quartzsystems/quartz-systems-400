# UIM panel groups

The `uim` crate (`ui/uim`) implements this document: the panel-group
tag language as `CRTPNLGRP` and `CRTMNU TYPE(*UIM)` read it from a
source member, the model a renderer walks, the checks, the listing and
the `*PNLGRP` payload. `ui/uim/tests/spec.rs` compares the code's
layouts with the tables in §6. The crate renders nothing and never
touches the store; imbedded members and the exports of other groups
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
`PRVPAGE`, `NXTPAGE`, `CALL var`, `PANEL name`, `MENU name`, `MSG var`.
`CALL` and `MSG` name a dialog variable holding the program or message
identifier; `PANEL` names a panel of the group or an import; `MENU`
names a `*MENU` object and is not checked.

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
`SPACE`; `LAYOUT` 1; `SCROLL` `NO`; `DISPLAY` `YES`; `CMDLINE SIZE`
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
| UIM1007 | 20 | menu option not 1 to 99 or `*` |
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

## 8. Open questions

- The condition grammar accepts a bare name as a variable because the
  reference's operand form is not pinned down; a probe of a real
  system settles whether `&name.` is the only form.
- `:DATAIXT.` is accepted as the text of the extender before it; its
  real attribute set is unverified.
- `CMDLINE SIZE` defaults to `LONG`; `:MENU.` text is taken as the
  area's heading. Both wait for probes.
- Help module names are checked for characters only (up to 32), panel
  and variable names for the ten-character rule.
