# Phase 7: command definitions and CL

Phase 7 gives the system commands. A `*CMD` object holds a definition
compiled from command definition source; one command analyzer reads
every line typed at a console, at Command Entry, through `QCMDEXC`
or in a CL program; the prompter shows a definition as the F4
display; the CL compiler turns ILE CL source into the IR of Phase 4;
every system command of Phases 2 to 6 is a `*CMD` with IBM's
parameter set; and the dev console's old words live on behind `svc.`.

## What was built

| Crate or place | Holds |
|---|---|
| `cmd` (`command/cmd`) | the tokenizer, the definition model, the source compiler and its rules (`rules.toml`), the `*CMD` payload, the analyzer (bind, convert, `DEP`, constant expressions), the parameter block, the store commands (`CRTCMD`, `CHGCMD`, `CHGCMDDFT`, `CRTPRXCMD`, ...), the prompter |
| `sys-cpps` (`command/sys-cpps`) | the 343 shipped definitions (`cmds/*.cmd`), the inventory (`inventory.toml`: sites, stubs, bridge words, completion messages, limited-user commands), the installer, the bridge, the denylist |
| `cl` (`command/cl`) | the CL compiler: lexer, parser, semantic tables, lowering to IR, listing |
| `lic/slic/src/cpp/` | the service's processing programs: direct implementations, the bridge, the CL commands (`CRTCLMOD`, `CRTBNDCL`, `CRTCLPGM`, `RTVCLSRC`) |
| `lic/slic/src/stgsvc/shipped/QSTRUP.clle` | the startup program as CL, compiled at the service's first start |
| `work/qsjob/src/command/` | the job's side: `run`, the dispatch by site, the job-site commands (`CALL`, `RTVJOBA`, `RTVSYSVAL`, `RTVOBJD`, the debugger, `SBMJOB`, ...), the `*LDA` commands, the prompt session on the display, the natives a compiled CL program calls (`QC*`, `QCLEXEC`, `QCLSWS`, `QCLDMP`), the `QCMDEXC` hook |
| `work/qsjob/src/work/inter.rs` | Command Entry: the 512-character line, F4, F9 over the last 25 commands, F10 second-level text, F11 full width, F24; `CPD0043` for a limited user |
| `work/stg` | `cmd_lookup` and `run_cmd` requests; the job's cached `CmdSource`; the Phase 4 `QCMDEXC` queue gone |
| `runtime/sysapi` | `QCMDEXC`, `QCMDCHK`, `QCAPCMD` through the hook |
| `runtime/msgf` | `CPD00xx` analyzer messages, `CPD02xx` definition rules, `CPD07xx` compiler messages, the CL commands' messages |
| `conformance` | the CL corpus kit, the analyzer kit, the benches, QEMU steps 46 to 50 |

`CMD.md` describes the source, the object, the analyzer, the block and
the prompter; `CL.md` the compiler; `CMD_OPEN_QUESTIONS.md` what a
real box would settle.

## The inventory

| Site | Commands | How |
|---|---|---|
| CL statements | 40 | compiled by `cl`; refused at a command line by `ALLOW` |
| service, direct | 40 | `lic/slic/src/cpp/direct.rs` on the library functions |
| service, bridged | 181 | the keyword form handed to a console word of Phases 2 to 6 |
| job | 20 | `work/qsjob/src/command/jobsite.rs` |
| stub | 4 | `SAVLIB`, `RSTLIB`, `CHKTAP` (`CPF3700`), `STRSST` (`CPF0901`) |
| unsupported | 58 | defined, prompted and analyzed; the service answers `CPF0006` (below) |

`command/sys-cpps/src/inventory.rs` keeps the lists, and
`tests/inventory.rs` checks every command is in exactly one.

## The kits

| Kit | What | Run |
|---|---|---|
| prompt goldens | 50 first displays over the shipped definitions, 14 states over the test definitions | `cargo test -p sys-cpps --test prompt`, `cargo test -p cmd --test prompt` |
| analyzer kit | 797 command strings and their outcomes (`tools/conformance/cmd/`) | `cargo test -p conformance --test cmd_kit` |
| CL corpus | 47 programs compiled, run with doubles, their outcomes and listings (`tools/conformance/cl/`) | `cargo test -p conformance --test cl_kit` |
| harness tripwire | every command string the QEMU harness types analyzes offline | `cargo test -p sys-cpps --test harness` |
| denylist | no IBM phrasing in the definitions or the message files | `cargo test -p sys-cpps --test defs` |
| spec | the `*CMD` header and the block constants match `CMD.md` | `cargo test -p cmd --test spec` |

## QEMU steps

| Step | Checks |
|---|---|
| 46 command entry | QSECOFR creates a library through the analyzer; `?CRTPF` and F4 prompt; Enter runs the prompted command; `DSPJOBLOG` lists the requests; F9 retrieves; a bad keyword gives `CPD0012` |
| 47 CL program | `CRTCLPGM` from a source member; `CALL` with a parameter prints the program's messages (`RTVJOBA`, `DOWHILE`, `MONMSG`); `SBMJOB` submits it; `WRKACTJOB`; `RTVCLSRC` gives the source back |
| 48 limited user | `LMTCPB(*YES)` refuses `CRTLIB` with `CPD0043`, allows sign-off |
| 49 startup program | the subsystems `QSTRUP` names are active; `QSYS/QSTRUP` is a `CLLE` program |
| 50 analyzer console | `CPD0015`, `CPD0030`, `CPD0012` at the service console; `svc.` words still work |

The suite is 50 steps.

## Benches

In memory, release build, one core of the development machine
(`cargo test --release -p conformance --test cmd_bench -- --ignored
--nocapture`):

| Bench | Rate |
|---|---|
| analyzer, the kit's 797 strings | 21 600 strings/s |
| prompter, first displays of five commands | 16 200 screens/s |
| CL compiler, the 47-program corpus with listings | 49.5 programs/s |

## Design notes

- One analyzer. The service console, Command Entry, `QCMDEXC` and the
  CL compiler all call `cmd::analyze`; the setting (`interactive`,
  `program`, `exec`, `module`) and the job's mode and `LMTCPB` decide
  what `ALLOW`, `MODE` and `ALWLMTUSR` refuse.
- Definitions are data. The 343 commands are source files compiled at
  build time (`sys-cpps/build.rs`) and installed into `QSYS` at the
  service's start; `CHGCMDDFT` is an overlay on the object, `CRTPRXCMD`
  a flag and a target.
- Two paths for a CL command. A constant or bare-variable command
  calls its processing program with a block laid out at compile time
  (`QSYS/QCCRTLIB(...)`); an expression goes through `QCLEXEC` at run
  time. Both end in the same dispatch by site.
- Sites, not reimplementation. The service's processing programs are
  thin: the bridge turns the keyword form back into the console word
  of the library function that already existed; `direct.rs` holds the
  commands the old words could not express.
- Exact-length character results. CL's `*CAT` semantics need the
  padding of a variable and the exact text of a literal or a built-in;
  the compiler keeps intermediate results in `varchar` temps and
  copies them into fixed storage only where an instruction takes the
  bytes as they lie.
- The runtime learned two things: a message to `*PRV` from the first
  entry goes to the job's external queue, and an escape there ends the
  call with the exception pending; the parser now remaps monitor
  handler labels (a bug the corpus found; `MON02` of the IR kit had
  been expecting the wrong handler).

## Blocked

- **No real-box goldens.** The prompt displays, the analyzer kit's
  outcomes and the listing layout are this system's own; the
  comparison with an IBM i (`CMD_OPEN_QUESTIONS.md` 1, 2, 10) waits
  for captures.
- **`MONMSG CMPDTA`** is accepted and not compared (`CPD0790`).
- **Display files** in CL (`SNDRCVF`, `SNDF`, `WAIT`, `ENDRCV`) end in
  `CPF4101` until Phase 8 brings displays; `STRSST` is a stub for the
  same reason.
- **Save and restore** (`SAVLIB`, `RSTLIB`, `CHKTAP`) are stubs for
  Phase 11.
- **Unsupported commands**, defined and analyzed but without a
  processing program: `ADDMSGD ALCOBJ ANZPRFACT CHGDBG CHGDEVD CHGJOBQ
  CHGLF CHGLIB CHGMSGD CHGMSGF CHGNETA CHGPF CHGPFM CHGSRCPF CLOF
  CPYLIB CPYSRCF CRTDEVDSP CRTDSPF CRTMSGF DLCOBJ DSPDEVD DSPHDWRSC
  DSPMODSRC DSPNETA DSPOVR DSPPTF DSPSFWRSC EDTAUTL EDTLIBL INZPFM
  MRGMSGF OPNQRYF OVRMSGF POSDBF PRTUSRPRF RCLRSC RMVBNDDIRE RMVMSGD
  RNMM RTVCFGSTS RTVMBRD RTVMSG RTVNETA RTVUSRPRF RUNSQL RUNSQLSTM
  SNDJRNE STRSQL TFRJOB UPDSRVPGM VRYCFG WRKBNDDIR WRKCFGSTS WRKHDWRSC
  WRKJRN WRKJRNA WRKOBJLCK`. The message file and SQL ones have library
  functions (`msgf::store`, `sql::run`) that a later batch of direct
  implementations can reach; the device, PTF and hardware ones wait
  for their phases.
- **The `CPD0030` fallback** to the old console words without `svc.`
  stays (`MIGRATE(phase7)` in `work/qsjob/src/command/mod.rs`) until
  every script of the harness and every note of the docs says `svc.`;
  the harness already does.
