# QSH: Qshell on QS/400

Qshell is written from scratch in Rust. No existing shell, coreutils or
shell-utilities code or crate is used or ported; behavior follows the
POSIX shell specification and Qshell's documented and observed
behavior. This document has two parts: the command language (this
section, crate `qsh/qsh`) and the utilities (crate
`qsh/qsh-utils`).

## Language

### Crate layout

`qsh/qsh` (package `qsh`):

| Module | What |
|---|---|
| `syntax/` | the tree with spans (`ast.rs`), errors, the parser: `input.rs` (cursor, line continuation, alias splicing), `token.rs`, `word.rs` (quoting and expansions read straight into word parts), `command.rs` (grammar), `heredoc.rs` |
| `arith/` | `$((...))` and `let`: lexer, precedence-climbing parser, evaluator over signed 64-bit integers |
| `pattern/` | `*`, `?`, bracket expressions with classes; `path.rs` pathname expansion over a `Dirs` source |
| `expand/` | tilde, parameters (`param.rs`), command substitution, arithmetic, field splitting (`split.rs`), globbing, quote removal |
| `shell/` | the interpreter state (`mod.rs`), variables, options, descriptor table, jobs, traps; `exec/` lists, pipelines, simple and compound commands, redirections |
| `builtins/` | special and regular builtins |
| `sys/` | the `Sys` seam, `Stream`s, `errno`, the utility registry, `MemSys`, `HostSys` |
| `session/` | entry points: `qsh` arguments, `run`, `Interactive`, `batch` |
| `signal.rs`, `msg.rs` | signal table, diagnostics |

### Grammar

The full POSIX `sh` grammar as Qshell implements it:

- Words: single quotes, double quotes, backslash, line continuation
  (backslash-newline, removed everywhere except single quotes and
  quoted here-documents), comments at word start.
- Parameter expansion: `$name`, `${name}`, positional `$1`..`$9` and
  `${10}`, special `$? $$ $! $# $@ $* $- $0`; `${x-w} ${x:-w} ${x=w}
  ${x:=w} ${x?w} ${x:?w} ${x+w} ${x:+w} ${#x} ${x%w} ${x%%w} ${x#w}
  ${x##w}`. The word of the test forms is tilde-expanded and, unquoted,
  field-split.
- Command substitution `$(...)` (parsed in line with the outer text,
  so here-documents and `case` work inside) and backquotes (with their
  own backslash rule, parsed separately), nested to any depth up to the
  nesting limit.
- Arithmetic `$((...))`: decimal, `0` octal, `0x` hex and `base#digits`
  (2 to 64) constants; variables by name (a value that is itself an
  expression is evaluated, 32 levels deep at most); unary `+ - ! ~`,
  prefix and postfix `++ --`, `* / % + - << >> < <= > >= == != & ^ |
  && || ?:`, every assignment operator and `,`. `&&`, `||` and `?:`
  skip the side effects of the branch not taken. Overflow wraps.
  `$((` that does not close with `))` is a command substitution of a
  subshell.
- Tilde expansion at word start and after `=`/`:` in assignments.
- Field splitting by `IFS` (whitespace runs, non-whitespace
  delimiters, `""` adjacent to an expansion keeps an empty field),
  pathname expansion, quote removal.
- Simple commands with assignments and redirections `< > >> >| <> <&
  >& n> n<` and here-documents `<<` and `<<-` (expanded unless any part
  of the delimiter is quoted).
- Pipelines with `!`, and-or lists, `&`, `;`, newlines.
- Compound commands: `( )`, `{ }`, `if/elif/else`, `while`, `until`,
  `for` (with and without `in`), `case` (with `(` before patterns, `|`
  alternatives, last `;;` optional).
- Function definitions `name() compound-command [redirections]`.
- Aliases, expanded in command position before the parser sees the
  word; an alias ending in a blank expands the next word too; an alias
  is not re-expanded inside its own text. The shell parses one complete
  command at a time, so an alias defined on a line applies from the
  next line.

Every command node carries a `Span` (offsets and line); syntax errors
name the line. The parser never panics (a random-input test runs 25,000
inputs) and nesting deeper than 100 levels is an error, not a stack
overflow, on a 2 MB thread.

### Execution model

The shell keeps its own descriptor table (`fd number -> Fd`, an
`Arc<dyn Stream>`), working directory, umask, variables, functions,
aliases, options, traps and jobs. Everything else goes through `Sys`.

- A subshell (`( )`, command substitution, a script run because the
  file is not a program) is a copy of the shell state run on the same
  thread.
- Each element of a multi-command pipeline is a subshell started with
  `Sys::fork` (a thread for `MemSys` and `HostSys`) and joined by
  `Sys::pipe` pipes. A writer whose reader has gone gets `EPIPE`; a
  shell writing to such a pipe ends as `SIGPIPE` would, so `while :; do
  echo; done | head` ends.
- `&` lists are subshells started with `Sys::fork`; their standard input
  is `/dev/null` unless `set -m`. `$!` is the child's process ID.
- Programs are looked up as special builtin, function, regular builtin,
  then `PATH` (cached by `hash`) and run with `Sys::spawn`, which gets
  the argument list, the exported environment plus the command's
  assignments, the working directory, the umask and the descriptor map.
  `ENOEXEC` from `spawn` means "a file that is not a program": the
  shell runs it as a script in a new shell environment (exported
  variables only, no functions or aliases, fresh options).
- Traps run between commands: `Sys::take_signals(pid)` returns signals
  that arrived for the (sub)shell; a signal without a trap ends the
  shell with status 128+n unless its default is to ignore it; an
  interactive shell ignores `INT`, `TERM` and `QUIT`.
- With `set -m` on an interactive shell and a `Sys::job_control()`, a
  foreground program gets the terminal for its duration and a stopped
  one becomes a job.

### Builtins

Special builtins (assignments before them persist; their errors end a
non-interactive shell): `: . break continue eval exec exit export
readonly return set shift times trap unset`.

Regular builtins: `[ alias bg builtin cd command declare echo false fg
getopts hash jobs kill let local print printf pwd read source test true
type typeset ulimit umask unalias wait`.

Output formats (scripts parse these; POSIX forms unless noted):

| Command | Form |
|---|---|
| `set` | `name=value` per line, sorted, value quoted for re-input (`'...'`, `'\''` for a quote) |
| `export -p`, `readonly -p` | `export name=value`, `export name` when unset |
| `declare -p` | `declare -- name=value`, attributes as `-irx` |
| `set -o` | `name` padded to 16, then `on`/`off` |
| `set +o` | `set -o name` / `set +o name` |
| `trap` | `trap -- 'action' NAME` (`EXIT` for 0, names without `SIG`) |
| `alias` | `name='value'` |
| `jobs` | `[n] + State command` (`Running`, `Done`, `Done(n)`, `Stopped(SIGx)`); `-l` adds the process ID after the mark; `-p` process IDs only |
| `type`, `command -V` | `x is a shell builtin`, `x is a special shell builtin`, `x is a shell function`, `x is an alias for v`, `x is a shell keyword`, `x is /path` |
| `command -v` | name, path, or `alias x='v'` |
| `kill -l` | signal names on one line |
| `umask` | four octal digits; `-S` as `u=rwx,g=rx,o=rx` |
| `times` | `XmY.YYs XmY.YYs` twice |
| `ulimit -a` | one line per limit, label padded to 21 |
| `hash` | one path per line |

`echo` interprets `\a \b \c \f \n \r \t \v \\ \0nnn` and takes `-n`
(no newline). `print [-nrR] [-u n]` is ksh's. `printf` supports
`%s %b %c %d %i %o %u %x %X %e %E %f %F %g %G %%`, flags `-+ #0`, `*`
width and precision, `'c` character codes, and reuses the format until
the arguments are used. `read [-r] [-p prompt] [-u n]` reads a byte at
a time so nothing past the line is consumed. `local` keeps the outer
value until assigned; `declare`/`typeset` in a function make the name
local. `getopts` keeps its position inside clustered options between
calls; assigning `OPTIND` resets it.

### Variables

Set at start: the environment (exported), `IFS` (space, tab, newline),
`PATH` (`/usr/bin:.` when not in the environment), `PS1` (`$ `), `PS2`
(`> `), `PS4` (`+ `), `PWD`, `PPID`, `OPTIND`, `QSH_VERSION` (the
release, `V1R0M0`), `HOME` from the user's profile when the environment
has none. Dynamic: `RANDOM` (0 to 32767; assigning seeds it), `SECONDS`
(assigning sets the base), `LINENO`. `ENV` names a file an interactive
shell reads at start; `TRACEFILE` names the `set -o trace` file
(default `$HOME/qsh_trace`); `CDPATH` is searched by `cd`.

### Options

`set -abCefmntuvx`, `-o allexport errexit ignoreeof monitor noclobber
noexec noglob nolog notify nounset trace verbose xtrace`; `qsh` also
takes `-c`, `-i`, `-l` (login: `/etc/profile` and `$HOME/.profile`),
`-s`. `$-` lists the letters in the order `abCefimnstuvx`. `errexit`
does not apply to conditions, non-last members of and-or lists or
negated pipelines. `trace` appends each command's text to the trace
file. `nolog` is accepted (there is no history file).

### Signals

Signal names and numbers are the system's (`QSYSINC/SYS(SIGNAL)`), so
`kill -6` is `TERM` and `trap ... 4` is `INT`. Status for a command
ended by a signal is 128+n.

### Diagnostics

Every message is `qsh: <id> <text>` on standard error. The identifiers
are interface; the texts are Quartz-written.

| Id | When |
|---|---|
| 001-0002 | `cd` cannot enter a directory |
| 001-0014 | command not found (status 127) |
| 001-0019 | command found but not runnable (status 126) |
| 001-0021 | `set -u` and an unset parameter |
| 001-0022 | `${x?word}` |
| 001-0023 | a redirection cannot open its file |
| 001-0024 | a descriptor that is not open |
| 001-0029 | assigning a read-only variable |
| 001-0030 | a builtin failed for a system reason |
| 001-0040 | no such job |
| 001-0045 | not a signal |
| 001-0050 | syntax error (status 2) |
| 001-0051 | substitution not valid at run time |
| 001-0062 | builtin usage |
| 001-0063 | not a valid name |
| 001-0067 | arithmetic error |
| 001-0068 | not a number |

### The `Sys` seam

`qsh::sys::Sys` (Send + Sync, no per-shell state) is everything outside
the language:

- environment and identity: `environ`, `initial_cwd`, `pid`, `ppid`,
  `home_of`;
- files (absolute paths): `open(path, OpenFlags, mode) -> Fd`, `stat`,
  `lstat`, `read_dir`, `access`, `realpath`, `case_insensitive(dir)`,
  `chdir`, and for utilities `mkdir rmdir unlink rename link symlink
  readlink chmod chown utimes statfs` (default `ENOSYS`), and the
  names and IDs of users and groups, `processes`, `nodename`, `machine`
  and `sleep` (see Utilities);
- `pipe`;
- processes: `spawn(SpawnRequest) -> Box<dyn Child>` (a utility thread
  or a child job), `fork(name, body) -> Box<dyn Child>` (shell code
  run concurrently), `kill`, `take_signals(pid)`, `signal_action`,
  `job_control() -> Option<&dyn JobControl>`;
- time and limits: `now`, `times`, `get_limit`, `set_limit`.

`Stream` (`read`, `write`, `is_terminal`, `as_any`) is an open
description; `Fd = Arc<dyn Stream>` and the last drop closes it.
In-process kinds: pipes, `Bytes` (here-documents), `Buffer`
(collecting output), `Null`. Errors are `SysError { errno }` with the
system's `errno` numbers (`ENOEXEC`, which the system lacks, is 3600
and never leaves the shell).

Utilities implement `qsh::sys::Utility` (`run(&mut UtilCtx) -> i32`;
closures work) and are registered by path in a `Registry`. `UtilCtx`
carries the arguments, environment, working directory, umask,
descriptor map and `&dyn Sys`.

`MemSys` is the test system: an in-memory file system with the root's
naming rule (case-insensitive except under `/QOpenSys`), `/dev/null`,
registered utilities run on threads, recorded signals, a settable
clock and settable limits. `HostSys` maps the shell's `/` to a host
directory through `std`, runs registered utilities in-process and other
programs as host processes (copying non-file streams through threads).

### Entry points

- `session::Invocation::parse(argv)`, `Invocation::session()`,
  `Invocation::command(cmd)`.
- `session::shell_for(sys, &inv, stdin, stdout, stderr) -> Shell`:
  options, arguments, startup files.
- `session::run(sys, &inv, stdin, stdout, stderr) -> i32`: `-c`, a
  script or standard input, to the exit status after the `EXIT` trap.
- `session::Interactive::new(shell)`: `prompt()` (`PS1`/`PS2`
  expanded), `feed_line(line) -> Feed::{Ready, More, Exit(n)}`,
  `feed_eof()`, `notices()` (finished background jobs). The 5250 QSH
  session and the SSH TTY feed lines; a line ending in an odd number of
  backslashes continues.
- `session::batch::run_cmd(sys, cmd, stdin, job_stdout) ->
  BatchResult { status, message }` for `QSH CMD()`:
  `QIBM_QSH_CMD_OUTPUT` `STDOUT` (default, the job's stream), `NONE`,
  `FILE=path` (replace), `FILEAPPEND=path` (both standard output and
  error go there); the message is `QSH0005` (status, completion; escape
  when `QIBM_QSH_CMD_ESCAPE_MSG=Y` and the status is above 0),
  `QSH0006` (signal, escape) or `QSH0007` (exception, escape).
  `CmdOutput::parse` and `ending_message` are usable on their own.
- `Shell::run_text(src, line)`, `Shell::run_list`, `Shell::var`,
  `Shell::status`, `Shell::set_fd`.

### Tests

- `tests/corpus/*.sht`: 692 cases (quoting, parameters, every
  expansion operator, command substitution, arithmetic, splitting,
  globbing including `/QSYS.LIB` and `/QOpenSys` case rules, tilde,
  redirections, here-documents, pipelines, lists, compound commands,
  `case`, loops, functions, aliases, variables, options, `test`,
  `echo`/`print`/`printf`, `read`, `getopts`, traps and signals, jobs,
  `cd`/`pwd`/`umask`, `eval`/`.`/`exec`/`command`/`type`/`hash`,
  scripts) with expected stdout, stderr and status, run against
  `MemSys`. `QSH_UPDATE_CORPUS=1` rewrites expectations;
  `QSH_CORPUS_ONLY=text` runs matching cases.
- `tests/fuzz.rs`: random token soup and random bytes through the
  parser, deep nesting and recursion on a 2 MB stack, random safe
  scripts through the interpreter.
- `tests/session.rs`: the interactive session, startup files, `qsh`
  arguments, the batch path, `HostSys` on a temporary directory.

### Open questions

The oracle's sign-on server dropped every connection on 2026-10-24 and
the host timed out on 2026-10-29, so none of the following was settled
on the system. The probe scripts are
in `tools/conformance/qsh/lang/probes.txt` (run through `QSH CMD()`
with `QIBM_QSH_CMD_OUTPUT=FILE=` under `/home/QSTEST`, copied to a
`QSTEST` member with `CPYFRMSTMF` and read over JDBC).

1. Message identifiers: only 001-0014 (not found) and 001-0050 (syntax
   error) are believed to match Qshell; the others in the table are
   placeholders to replace with the system's numbers.
2. Whether `QIBM_QSH_CMD_OUTPUT=FILE=` also receives standard error
   (assumed yes).
3. `echo`: `-n` and escape handling as implemented; whether Qshell
   also takes `-e`.
4. Output formats of `set -o`, `ulimit -a`, `kill -l`, `hash`, `type`
   for a name not found (stdout or stderr, status), `times`.
5. Whether `printf`, `print`, `let`, `declare`, `typeset`, `local`,
   `builtin`, `source`, `fg`, `bg` are Qshell builtins, and whether
   `whence` exists.
6. `local x` without a value: keeps the outer value (ash) or unsets
   (ksh, bash).
7. The `trace` option's letter (`-t` assumed) and the trace file's
   format (command text only assumed; the system may add time stamps).
8. Exit status for a command ended by a signal (128+n assumed; ksh uses
   256+n).
9. `$-` letter order; whether `i` and `s` appear.
10. Syntax error line numbers at end of input (the line after the last
    newline is reported).
11. `$'...'`, `function name { }`, and `name()` with `-` in the name: not
    implemented; assumed absent from Qshell.
12. Whether the last element of a pipeline runs in the current shell
    (assumed not: `echo x | read v` leaves `v` unset).
13. Pathname expansion sort order (byte order assumed; the system may
    collate by CCSID).
14. Arithmetic width (64-bit assumed) and whether variables holding
    expressions are evaluated recursively.

## Utilities

### Crate layout

`qsh/qsh-utils` (package `qsh-utils`): every utility is written from
scratch (no uutils, busybox, coreutils-derived crate or ported GNU/BSD
code) as a `fn(&mut UtilCtx) -> i32`, a `qsh::sys::Utility` that reaches
the system only through `Sys` (no `std::fs`, no `unsafe`).
`qsh_utils::register(&mut Registry)` adds them all under `/usr/bin`;
`UTILITIES` lists them by name and `lookup(name)` finds one.

| Module | What |
|---|---|
| `common/` | `args.rs` POSIX option parsing (clusters, attached arguments, `--`, the old `-5`/`+2` forms); `io.rs` opening operands (`-` is standard input), buffered lines, `Out` (buffered output that notices a closed pipe), `confirm`; `time.rs` the civil calendar, POSIX `TZ` strings and `strftime`; `mode.rs` symbolic and octal modes and the `ls -l` mode string; `walk.rs` tree walks with pre/post visits, pruning, `-H`/`-L`/`-x` and loop detection; `exec.rs` `PATH` search and `Sys::spawn`, a script (`ENOEXEC`) going to `/usr/bin/sh` as `execvp` does |
| `regex/` | POSIX basic and extended regular expressions over bytes (`parse.rs` the tree, `mod.rs` the compiled program and a backtracking matcher that keeps the leftmost-longest match; without back-references a visited set keeps it linear) |
| `file/` | `cat chmod chgrp chown cp df du file find ln ls mkdir mkfifo mv pwd rm rmdir touch` (`file` is `filetype.rs`) |
| `text/` | `cmp cut expand grep egrep fgrep head iconv od paste pr sed sort split tail tee tr uniq wc`; `sed/` is `parse.rs` (the script) and `exec.rs` (the cycle); `catalog/` is `gencat` and `dspmsg` over the catalog format in `format.rs` |
| `proc/` | `basename date dirname echo env printenv expr id logname locale nohup ps pwdx sleep test true false uname xargs` |
| `archive/` | `pax tar compress uncompress zcat ajar`: `member.rs` the model every format reads and writes, `ustar.rs` and `cpio.rs` (odc, newc, crc) the formats behind `codec.rs`, `gather.rs`/`build.rs` members from files, `restore.rs` files from members, `select.rs` patterns, `subst.rs` `-s`, `listing.rs` verbose lines, `arcio.rs` blocked archive I/O, `lzw.rs` the `.Z` coder, `zip/` CRC-32, deflate, inflate and the zip container |
| `net/` | `hostname nslookup host dig rexec`; `dns.rs` asking the resolver, reverse names, record forms |
| `ibmi/` | `system db2 liblist sysval datarea dataq catsplf getjobid attr setccsid Rfile ipcs ipcrm clrtmp logger`; `msgs.rs` CL messages and the CL runner |

Messages are `name: text` on standard error, the text Quartz-written
(the system's `errno` text after a path). Statuses are POSIX: 0, 1 for
a failure, 2 for usage (`grep`, `sort`, `sed` input errors: 2), 126/127
when a program cannot run or is not found, `xargs` 123/124/125.

### Sys additions

`Stat` gained `nlink` and `blocks` (512-byte units). `Sys` gained, with
defaults so other systems need not implement them: `statfs(path) ->
FsInfo` (`df`), `user_name`/`group_name`/`user_id`/`group_id` (`ls -l`,
`chown`, `find -user`), `processes() -> Vec<ProcInfo>` (`ps`: job,
status, function, device, CPU time, arguments), `nodename`, `machine`
(`uname`) and `sleep`. `HostSys` implements `symlink`, `chmod`, `chown`
and `utimes`; `MemSys` implements all of them plus test setters
(`set_user`, `set_group`, `set_owner`, `set_fsinfo`, `add_process`,
`set_nodename`, `make_object` for a `/QSYS.LIB` object that is neither
library, file nor member: type `Other`, not openable, `EOPNOTSUPP`). A
`SysError` wrapped in an `io::Error` now converts back to itself, so
utilities report the system's `errno` after a failed read or write.

### Services

The utilities that reach past the file system go through hooks on
`Sys` (`qsh::sys::services`), every one `ENOSYS` by default:

| Hook | Used by | What the system supplies |
|---|---|---|
| `resolver() -> &dyn Resolver` | `nslookup host dig hostname` | `query(DnsQuery) -> DnsAnswer` (server, port, response code, authority flag, the name after the search list, answer, authority and additional records with data in presentation form) and `host_name()` (the TCP/IP host name and domain). The sockets resolver is wired in here. |
| `cl_runner() -> &dyn ClRunner` | `system logger Rfile -c` | `run_cl(ClRequest)` runs one command (in the shell's job or a new one, with environment variables, keeping spooled files or the job log) and returns its messages (id, type, text), its spooled files' pages and whether it failed. |
| `sql_runner() -> &dyn SqlRunner` | `db2` | `run_sql(SqlRequest)` (statement, system naming) returns a result set (columns with display widths and alignment, rows of text or null), a count, or an error (SQLSTATE, SQLCODE, text). |
| `tcp_connect(host, port) -> Fd` | `rexec` | a connected stream; `Stream::shutdown_write` (new, a no-op by default) ends the sending half |
| `credentials()` | `id logname clrtmp -c rexec` | the job's user profile, uid, group profile's gid (0 for none), supplemental groups |
| `mkfifo(path, mode)` | `mkfifo pax tar` | |
| `process_cwd(pid)` | `pwdx` | |
| `attributes(path, follow)`, `set_attribute(...)` | `attr setccsid file` | `NAME=value` pairs (`OBJTYPE CCSID CODEPAGE DATA_SIZE ALLOC_SIZE ASP ALWSAV PC_READ_ONLY PC_HIDDEN PC_SYSTEM PC_ARCHIVE SYS_ARCHIVE` in the test system) |
| `library_list()`, `change_library_list(LiblChange)` | `liblist Rfile -Q` | the job's list with each entry's portion; add first or last, remove, set the current library |
| `system_value(name)`, `network_attribute(name)` | `sysval` | the value as text, `ENOENT` when unknown |
| `read_data_area(name)`, `write_data_area(name, part, value)` | `datarea` | `*LDA`, `*GDA`, `*PDA` or a path; type (`*CHAR`, `*DEC` digits and places, `*LGL`), length, value |
| `data_queue(path, DtaqOp)` | `dataq` | send (with a key), receive (key, wait, remove or peek), clear, count |
| `spooled_files(job)`, `spooled_file(job, name, number)` | `catsplf` | a job's spooled files (`*` the current job); one file's pages of lines |
| `ipc_list()`, `ipc_remove(kind, id or key)` | `ipcs ipcrm` | System V queues, segments and semaphore sets with their numbers and times |

`errno` gained `ENXIO`, `ECONNREFUSED`, `EHOSTUNREACH`, `ETIMEDOUT`,
`EIDRM` and `ENOMSG` (the system's numbers). `stream::duplex()` makes a
connected pair of streams (a socket stand-in for tests).

`MemSys` answers every hook. The resolver, CL runner, SQL runner and
connector are handlers a test sets (`add_dns`, `set_dns_server`,
`set_dns_authoritative`, `set_host_name`, `set_cl`, `set_sql`,
`set_connector`); the rest are tables with setters
(`set_credentials`, `set_process_cwd`, `set_sysval`, `set_netattr`,
`set_data_area`, `make_data_queue`, `add_spooled_file`, `add_ipc`,
`set_library_list`), a FIFO node kind (opening one is `ENXIO`), and
attributes computed from the node with overrides. The corpus seeds
them in `tests/common/services.rs`: DNS records for `example.com`, a
small CL interpreter (`CRTLIB`, `DLTLIB`, `DSPLIBL`, `DSPJOB`,
`WRKENVVAR`, `SNDPGMMSG`, unknown commands failing with `CPD0030` and
`CPF0006`) that logs every command to `/tmp/cl.log`, canned SQL
results, an `rexec` server on `rhost:512`, system values, data areas
and a data queue in `QSHLIB`, spooled files and IPC objects.

### /QSYS.LIB

Utilities see `/QSYS.LIB` only through `Sys` paths: names are
`NAME.TYPE` in any case, libraries and files are directories, members
are files whose records read as lines, other objects are type `Other`
(shown by `ls -l` with type `-`, counted as `f` by `find -type`) and
refuse `open`. `cat`, `cp`, `grep`, `wc`, `sort` read members like any
file.

### Status

Every utility below is implemented; "probe" is the script under
`tools/conformance/qsh/utils/` for the oracle run (each sources
`_lib.sh`, which builds the corpus's seeded tree under a scratch
directory and removes it).

| Utility | Status | Probe |
|---|---|---|
| `basename`, `dirname` | done | `basename.sh`, `dirname.sh` |
| `cat` | done | `cat.sh` |
| `chmod` | done (octal, symbolic, `-R`) | `chmod.sh` |
| `chown`, `chgrp` | done (`-R`, `-h` accepted) | `chown.sh`, `chgrp.sh` |
| `cp` | done (`-fipRr -HLP`) | `cp.sh` |
| `cut` | done | `cut.sh` |
| `date` | done (display only; setting is refused) | `date.sh` |
| `df` | done (`-kP`, `-t` accepted) | `df.sh` |
| `du` | done (`-askx -HL`) | `du.sh` |
| `echo`, `true`, `false` | done (as programs for `xargs`/`find -exec`) | - |
| `env` | done | `env.sh` |
| `expand` | done | `expand.sh` |
| `find` | done (`-name -path -type -size -mtime -atime -ctime -newer -user -group -nouser -nogroup -perm -links -prune -print -exec -ok -depth -xdev -true -false`, `! -a -o ( )`, `-H -L`) | `find.sh` |
| `grep`, `egrep`, `fgrep` | done (`-EFclqhHinsvx -e -f`) | `grep.sh` |
| `head`, `tail` | done (`tail -f` reads to end of file and stops) | `head.sh`, `tail.sh` |
| `iconv` | done (CCSID tables of `ccsid::conv`) | `iconv.sh` |
| `ln` | done | `ln.sh` |
| `ls` | done (`-1ACFLRSadfgiklmnopqrstux`) | `ls.sh` |
| `mkdir`, `rmdir`, `rm`, `mv`, `touch` | done | `mkdir.sh`, `rmdir.sh`, `rm.sh`, `mv.sh`, `touch.sh` |
| `od` | done (`-t` types, `-A -j -N -v`, `-bcdosx`) | `od.sh` |
| `paste` | done | `paste.sh` |
| `ps` | done (`-aAdefl -o -p -u -g -G -t`) | `ps.sh` |
| `sed` | done (the full POSIX language) | `sed.sh` |
| `sleep` | done | `sleep.sh` |
| `sort` | done (`-cmou -bdfinr -t -k`) | `sort.sh` |
| `split` | done | `split.sh` |
| `tee` | done | `tee.sh` |
| `tr` | done | `tr.sh` |
| `uname` | done | `uname.sh` |
| `uniq` | done | `uniq.sh` |
| `wc` | done | `wc.sh` |
| `xargs` | done | `xargs.sh` |
| `cmp` | done (`-l -s`, skips) | `cmp.sh` |
| `pr` | done (`+page -column -adFfmprt -e -h -i -l -n -o -s -w`) | `pr.sh` |
| `expr` | done (the full POSIX grammar, `:` with BREs) | `expr.sh` |
| `gencat`, `dspmsg` | done (X/Open source; our catalog format; `NLSPATH`) | `gencat.sh`, `dspmsg.sh` |
| `file` | done (`-h`; `-m` accepted) | `file.sh` |
| `locale` | done (`-a -m -c -k`, the POSIX locale's data) | `locale.sh` |
| `nohup`, `printenv`, `pwd`, `test`, `mkfifo` | done | `nohup.sh`, `printenv.sh`, `pwd.sh`, `test.sh`, `mkfifo.sh` |
| `id`, `logname`, `pwdx` | done | `id.sh`, `logname.sh`, `pwdx.sh` |
| `pax` | done (list, `-r -w -rw`, `-a -b -c -d -f -H -k -L -l -n -p -s -u -v -x -X`; `ustar tar cpio sv4cpio sv4crc`; `-i` refused, `-o -t` accepted) | `pax.sh` |
| `tar` | done (`c r t u x` with `b f h L l m o p v w C`) | `tar.sh` |
| `compress`, `uncompress`, `zcat` | done (LZW `.Z`, `-b 9..16 -c -d -f -v`) | `compress.sh`, `uncompress.sh`, `zcat.sh` |
| `ajar` | done (`c t x u` with `v f m 0 M C`; deflate and inflate) | `ajar.sh` |
| `hostname` | done (`-s`) | `hostname.sh` |
| `nslookup` | done (options, server operand, interactive commands) | `nslookup.sh` |
| `host` | done (`-t -a -v -c -r`, the rest accepted) | `host.sh` |
| `dig` | done (`@server -t -x -q -p -c`, `+[no]short cmd comments question answer authority additional stats all recurse`) | `dig.sh` |
| `rexec` | done (`-n -u -l -p -P`, `.netrc`) | `rexec.sh` |
| `system` | done (`-b -e -i -k -K -n -q -s -v`) | `system.sh` |
| `db2` | done (`-S -t -v -f`) | `db2.sh` |
| `liblist` | done (`-a -f -l -d -c`; a builtin in Qshell, a utility here) | `liblist.sh` |
| `sysval` | done (`-p -n`) | `sysval.sh` |
| `datarea` | done (`-r -w -s`) | `datarea.sh` |
| `dataq` | done (`-s -r -c -n -p -k -t`) | `dataq.sh` |
| `catsplf` | done (`-a -e -p -j`) | `catsplf.sh` |
| `getjobid` | done (`-s`) | `getjobid.sh` |
| `attr`, `setccsid` | done (`-h -p`) | `attr.sh`, `setccsid.sh` |
| `Rfile` | done (`-r -w -a -b -K -Q -c`) | `Rfile.sh` |
| `ipcs`, `ipcrm` | done (`-qms -abcopt`; `-q -m -s -Q -M -S`) | `ipcs.sh`, `ipcrm.sh` |
| `clrtmp` | done (`-c`) | `clrtmp.sh` (messages only) |
| `logger` | done (`-i -f -p -t`, to `QHST` through `SNDMSG`) | `logger.sh` (messages only) |

### Inventory

Every entry of the system's `/usr/bin` (the 163 of the 7.5 capture in
`tools/conformance/oracle/net/pub400/qsh-usr-bin.txt`): 73 implemented,
5 aliases, 85 not supported. `liblist` (a Qshell builtin, not in
`/usr/bin`) is a utility here as well. Qshell has no `awk`, `diff`,
`comm`, `join`, `nl`, `fold` or `seq`; those on the system are PASE's,
and QS/400 ships nothing inside PASE.

| Entry | Status | Note |
|---|---|---|
| `ajar` | implemented |  |
| `appletviewer` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `apt` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `armsrv` | not supported | Application Response Measurement server (performance tools) |
| `armsrvconv` | not supported | Application Response Measurement converter (performance tools) |
| `attr` | implemented |  |
| `basename` | implemented |  |
| `bash` | not supported | an open-source PASE package; QS/400 ships nothing inside PASE |
| `cat` | implemented |  |
| `catsplf` | implemented |  |
| `chgrp` | implemented |  |
| `chmod` | implemented |  |
| `chown` | implemented |  |
| `clrtmp` | implemented |  |
| `cmp` | implemented |  |
| `compress` | implemented |  |
| `cp` | implemented |  |
| `cut` | implemented |  |
| `dataq` | implemented |  |
| `datarea` | implemented |  |
| `date` | implemented |  |
| `db2` | implemented |  |
| `db2profc` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `db2profc_trace` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `db2profp` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `ddns-confgen` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `delv` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `dfmadmin` | not supported | DataLink File Manager administration (no DataLink support) |
| `dhcpd` | not supported | the DHCP server (no DHCP server) |
| `dhcrelay` | not supported | the DHCP relay agent (no DHCP server) |
| `dig` | implemented |  |
| `dirname` | implemented |  |
| `dnssec-dsfromkey` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `dnssec-importkey` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `dnssec-keyfromlabel` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `dnssec-keygen` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `dnssec-revoke` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `dnssec-settime` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `dnssec-signzone` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `dnssec-verify` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `dspmsg` | implemented |  |
| `echo` | implemented |  |
| `egrep` | alias | `grep -E` |
| `env` | implemented |  |
| `expr` | implemented |  |
| `extcheck` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `fgrep` | alias | `grep -F` |
| `file` | implemented |  |
| `find` | implemented |  |
| `gencat` | implemented |  |
| `getjobid` | implemented |  |
| `grep` | implemented |  |
| `head` | implemented |  |
| `host` | implemented |  |
| `hostname` | implemented |  |
| `httpapi` | not supported | a third-party library's program (`LIBHTTP`), not part of the system |
| `hwkeytool` | not supported | the cryptographic coprocessor's key tool (no coprocessor) |
| `iconv` | implemented |  |
| `id` | implemented |  |
| `idlj` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `ikeyman` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `ipcrm` | implemented |  |
| `ipcs` | implemented |  |
| `ixlc` | not supported | the ILE C/C++ compiler (no C compiler) |
| `jar` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `jarsigner` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `java` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `javac` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `javadoc` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `javah` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `javap` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `jconsole` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `jdb` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `jdmpview` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `jextract` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `kdestroy` | not supported | Kerberos (network authentication service): not in this phase |
| `keytab` | not supported | Kerberos (network authentication service): not in this phase |
| `keytool` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `kinit` | not supported | Kerberos (network authentication service): not in this phase |
| `klist` | not supported | Kerberos (network authentication service): not in this phase |
| `kpasswd` | not supported | Kerberos (network authentication service): not in this phase |
| `ksetup` | not supported | Kerberos (network authentication service): not in this phase |
| `ldapadd` | not supported | LDAP client: no directory server or client yet |
| `ldapchangepwd` | not supported | LDAP client: no directory server or client yet |
| `ldapcompare` | not supported | LDAP client: no directory server or client yet |
| `ldapdelete` | not supported | LDAP client: no directory server or client yet |
| `ldapdiff` | not supported | LDAP client: no directory server or client yet |
| `ldapexop` | not supported | LDAP client: no directory server or client yet |
| `ldapmodify` | not supported | LDAP client: no directory server or client yet |
| `ldapmodrdn` | not supported | LDAP client: no directory server or client yet |
| `ldapsearch` | not supported | LDAP client: no directory server or client yet |
| `ln` | implemented |  |
| `locale` | implemented |  |
| `logger` | implemented |  |
| `logname` | implemented |  |
| `ls` | implemented |  |
| `mkdir` | implemented |  |
| `mkfifo` | implemented |  |
| `mv` | implemented |  |
| `named` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `named-checkconf` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `named-checkzone` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `named-compilezone` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `named-journalprint` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `named-rrchecker` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `native2ascii` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `nohup` | implemented |  |
| `nslookup` | implemented |  |
| `nsupdate` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `od` | implemented |  |
| `orbd` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `pack200` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `pax` | implemented |  |
| `policytool` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `pr` | implemented |  |
| `printenv` | implemented |  |
| `profconv` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `profdb` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `profp` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `ps` | implemented |  |
| `pwd` | implemented |  |
| `pwdx` | implemented |  |
| `qsh` | alias | the shell itself (`qsh` crate) |
| `rexec` | implemented |  |
| `rexx` | not supported | the REXX interpreter (no REXX) |
| `rm` | implemented |  |
| `rmdir` | implemented |  |
| `rmic` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `rmid` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `rmiregistry` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `rndc` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `rndc-confgen` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `Rfile` | implemented |  |
| `schemagen` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `sed` | implemented |  |
| `serialver` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `servertool` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `setccsid` | implemented |  |
| `sh` | alias | the shell itself (`qsh` crate) |
| `sort` | implemented |  |
| `split` | implemented |  |
| `sqlj` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `strEKM` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `system` | implemented |  |
| `sysval` | implemented |  |
| `tail` | implemented |  |
| `tar` | implemented |  |
| `tee` | implemented |  |
| `test` | implemented |  |
| `tnameserv` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `touch` | implemented |  |
| `tr` | implemented |  |
| `tsig-keygen` | not supported | DNS server tool (BIND): QS/400 has no DNS server; `delv` needs a validating resolver |
| `uname` | implemented |  |
| `uncompress` | implemented |  |
| `uniq` | implemented |  |
| `unpack200` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `wc` | implemented |  |
| `wsgen` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `wsimport` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `xargs` | implemented |  |
| `xjc` | not supported | Java tool: needs a JVM, which QS/400 does not ship |
| `zcat` | alias | `uncompress -c` |


### Formats

| Utility | Output |
|---|---|
| `ls -l` | mode, link count right-aligned one column past the widest, owner and group each left-aligned in 8, size right-aligned in 14, `Mon DD HH:MM` within six months else `Mon DD  YYYY`, name, ` -> target` (from the 7.5 capture of `ls -la /usr/bin`); a directory listing starts `total: N kilobytes` below a megabyte, `total: N.NNN megabytes` (or gigabytes) above |
| `ls` | one name a line unless standard output is a terminal (`-C`, `COLUMNS` or 80 wide) |
| `df` | `Filesystem 512-blocks Used Available Capacity Mounted on` (`1024-blocks` with `-k`), capacity rounded up |
| `du` | `blocks<TAB>path`, 512-byte blocks, `-k` kilobytes rounded up, a hard link counted once |
| `ps` | default `PID DEVICE TIME FUNCTION STATUS JOBID`; `-f` `USER PID PPID DEVICE TIME COMMAND`; `-l` `UID PID PPID DEVICE TIME FUNCTION STATUS JOBID`; `TIME` `[[dd-]hh:]mm:ss`; `-o` keywords `pid ppid pgid user uid group gid tty device time etime comm args jobid jobname jobuser jobnumber status function nice pcpu vsz`, `key=header` renames |
| `uname` | `-s` `OS400`, `-n` the system name, `-r` the release and `-v` the version digits of the `release` crate's `VxRyMz` (`V1R0M0`: `0` and `1`, as `V7R5M0` prints `5` and `7`), `-m` the machine; `-a` all five |
| `date` | `%a %b %e %H:%M:%S %Z %Y`, the `TZ` zone (POSIX `TZ` strings) or UTC |
| `wc` | each count right-aligned in 8, then ` name`; `total` for several |
| `uniq -c` | count right-aligned in 4, a space, the line |
| `od` | addresses 7 octal digits (`-A d` 7 decimal, `-A x` 6 hex), multi-byte units in big-endian order as the system prints them, `*` for repeated lines |
| `head`, `tail` | `==> name <==` before each of several files, a blank line between |
| `grep` | `name:` before lines when several files, `(standard input)` as the name of `-` |
| `cmp` | `a b differ: char N, line M` (POSIX); `-l` `N o1 o2` in octal; end of file on standard error |
| `pr` | 66-line pages: two blank lines, `Mon DD HH:MM YYYY name Page n`, two blank lines, the text, five blank lines |
| `pax -v`, `tar tv` | `mode links owner(8) group(8) size(8) date name`, ` -> target` for a link, ` == name` for a hard link |
| `compress -v` | `name: X% smaller, replaced with name.Z` on standard error |
| `nslookup` | `Server:\t\tS`, `Address:\tS#53`, blank, `Non-authoritative answer:`, then `Name:\tN` / `Address: A` for addresses and `N\tmail exchanger = ...`, `nameserver =`, `canonical name =`, `text =`, `name =` (PTR), `service =`, the SOA block; `** server can't find N: NXDOMAIN`, `*** Can't find N: No answer` (BIND's forms, which scripts parse) |
| `host` | `N has address A`, `has IPv6 address`, `is an alias for`, `mail is handled by`, `name server`, `domain name pointer`, `descriptive text`, `has T record`; `Host N not found: 3(NXDOMAIN)` |
| `dig` | `;; ->>HEADER<<-` and `;; flags:` lines, the sections in master-file form (`name.\tTTL\tIN\tTYPE\tdata`), `;; SERVER:`; our own banner line |
| `system` | spooled lines, then `ID: text` per message: completion, informational and notify on standard output, diagnostic and escape on standard error |
| `db2` | blank, names, dashes, rows (numbers right-aligned, null `-`), blank, `  n RECORD(S) SELECTED.`, blank; `DB20000I  THE SQL COMMAND COMPLETED SUCCESSFULLY.`; ` **** CLI ERROR *****`, `         SQLSTATE: s`, `NATIVE ERROR CODE: n`, the text |
| `liblist` | `NAME` left-aligned in 10, a blank, `SYS`/`PRD`/`CUR`/`USR` |
| `sysval -p` | `NAME value` |
| `getjobid` | `Process P runs in job N/U/J.`; `-s` `job user number` |
| `catsplf -p` | ASA control first: `1` a page's first line, blank otherwise; `-e` a form feed between pages |
| `ipcs` | `IPC status from S as of date`, a title and heading line per kind, `T ID KEY MODE OWNER GROUP` and the asked-for columns, keys `0x%08x`, times `HH:MM:SS` or `no-entry` |
| `id` | `uid=n(NAME) gid=n` (a group with a profile as `gid=n(NAME)`), `groups=` when there are supplemental groups |
| `pwdx` | `pid: path` |

### Tests

- `tests/corpus/*.sht`: 1,861 cases over the seeded `MemSys` (a tree
  under `/home/QSHUSER` with data files, a hidden file, a link, an old
  file owned by `QSYS`, raw-byte files, a `/QSYS.LIB` library with a
  source file, member, program and data area, and `/usr/bin/sh` running
  scripts in the same system, and the services of
  `tests/common/services.rs`), one file per group of utilities
  (`archive-*` 262, `misc-*` 310, `net` 124, `ibmi-*` 312);
  `QSH_UPDATE_CORPUS=1` rewrites the expectations, `QSH_CORPUS_ONLY=x`
  runs matching cases and `QSH_CORPUS_FILES=prefix` limits the run and
  the rewrite to matching files.
- `tests/regex.rs`: a table of POSIX spans, groups, errors, case
  folding, `find_all` stepping, pathological patterns that must finish,
  and 4,000 random patterns checked against a brute-force matcher plus
  20,000 random patterns that must not panic.
- `tests/torture.rs`: `sort` with each key and flag combination against
  a reference sort over random records, numeric sorting against a float
  reference, `sed` scripts against string references, a 100,000-byte
  line, 20,000 lines and pathological loops.
- `tests/archive.rs`: random data through LZW, deflate and inflate,
  every archive format and the zip container round trip, random trees
  through `pax`, `tar`, a `tar` and `compress` pipe and `ajar`, and
  random bytes into every decoder (an answer, never a panic).
- `tests/misc.rs`: 20,000 random `expr` expressions against a separate
  evaluator, grouping, and `pr` page invariants over random input and
  options.

### Open questions (utilities)

None of these was checked on the system; the probes ask each.

| # | Utility | Question | Assumed |
|---|---|---|---|
| 1 | all | Qshell's messages carry an identifier (`001-xxxx`); which, and the exact form | `name: text` without an identifier |
| 2 | `ls -l` | whether the 8/8/14 widths are fixed or grow; owner names over 8; the group column for named groups | fixed minimums, longer names widen the line |
| 3 | `ls -l` | the `total:` line's base (1024 or 1000), rounding, the unit names past a gigabyte; whether `-s` without `-l` prints it | 1024, rounded up, `gigabytes`; the same line |
| 4 | `ls` | `/QSYS.LIB` objects: the type letter, size and permissions shown, and `-F` marks | `-`, the object's size, no mark |
| 5 | `ls -lF` | whether a link shows `@` before ` -> ` | no |
| 6 | `df` | header and column widths, the file system names (`/dev/QASP01`, `/QSYS.LIB`), what `-t` does | as in Formats; `-t` ignored |
| 7 | `du` | tab or blanks between the count and the path | tab |
| 8 | `ps` | default columns, `-f`/`-l` sets, `TIME` format, keyword names for job fields | as in Formats |
| 9 | `uname` | `-m` (serial or machine type) and `-v`/`-r` digits for `V1R0M0` | `0000000` from `Sys::machine`; `0` and `1` |
| 10 | `date` | the zone when `TZ` is unset (Qshell uses `QTIMZON`); `%c` `%x` `%X` forms | UTC; POSIX locale forms |
| 11 | `od` | byte order of multi-byte units; float digits | big-endian; C's `%e` with 6 or 15 digits |
| 12 | `wc` | field widths and separators | `%8d` each, then a space and the name |
| 13 | `sort` | collation: bytes or the job's CCSID order | bytes |
| 14 | `sed` | `N` on the last line (POSIX: quit without printing; GNU prints); whether `n`/`N` clear the `t` flag; `l` folding width; statuses for script errors (1) and missing input (2) | POSIX for all |
| 15 | `grep` | status when a file is missing and a line matched | 2 (0 with `-q`) |
| 16 | `xargs` | the default command-line size; `-p` reads standard input (not `/dev/tty`) | 4096; standard input |
| 17 | `iconv` | the code set names Qshell accepts and `-l`'s form | `IBMCCSIDnnnnn`, `IBM-nnn`, `CPnnn`, `UTF-8`, `ISO8859-1` and numbers; `-l` lists `IBMCCSIDnnnnn` |
| 18 | `touch` | `-a` (the system keeps one time through `Sys::utimes`) | `-a` and `-m` both set the modification time |
| 19 | `tail -f` | following a file that grows | reads to end of file and stops |
| 20 | `chown` | whether names fold case and numbers name profiles | profile names in any case; numbers as IDs |
| 21 | `echo` | whether `/usr/bin/echo` takes `-n` and the same escapes as the builtin | yes |
| 22 | `cp`, `mv` | `-i` prompt text and whether prompts go to standard error | `name: replace x? ` on standard error |
| 23 | `system` | the option set (`-E -I -p` exist?), where each message type goes, the exit status after an escape message, the order of spooled output and messages | `-beikKnqsv`; as in Formats; 255; spooled output first |
| 24 | `db2` | the options (`-d`, `-r`, `-T`?), column widths and alignment, null display, the exit status after an error, whether it stops at the first error | `-Stvf`; as in Formats; 1; it goes on |
| 25 | `liblist` | the display form, `-a` without `-f`/`-l` (front?), messages | as in Formats; front |
| 26 | `sysval` | the `-p` form | `NAME value` |
| 27 | `datarea` | the options (`-s` form, `-l`?), trailing blanks on read, `*DEC` text form | `-rws start[:len]`; kept; sign and point |
| 28 | `dataq` | the options and defaults (send when an entry is given) | as implemented |
| 29 | `catsplf` | what `-e` and `-p` do, whether `-j` may be left out | form feeds; ASA characters; the current job |
| 30 | `getjobid` | the default text and the `-s` field order | `name user number` |
| 31 | `attr` | attribute names and value spellings, the listing form | as in Services; `NAME=value` |
| 32 | `Rfile` | the options (`-K`, `-l`, `-q`, `-s`), trailing blanks, `.FILE` without a member | as implemented |
| 33 | `ipcs` | the column layout, one heading or one per kind | one per kind |
| 34 | `clrtmp` | what `-c` selects | the user's own objects |
| 35 | `logger` | where messages go (`QHST` assumed), the tag and priority handling | `SNDMSG ... TOMSGQ(QSYS/QHST)` |
| 36 | `hostname` | qualified or short, case (`HOSTNAME=WWW.PUB400.COM` in the environment) | the resolver's `host_name()` as given |
| 37 | `nslookup`, `host`, `dig` | the exact BIND release forms on the system (tabs, the AAAA second query, `Authoritative answers can be found from:`) | BIND 9 forms as in Formats |
| 38 | `rexec` | Qshell's options (`-n` meaning), the `.netrc` use, prompting for a password | `-n` no standard input; `.netrc`; no prompt |
| 39 | `id` | `-G` with no supplemental groups; other users' groups | the primary group alone; 0 |
| 40 | `pax`, `tar` | `-i`, `-z`, `bcpio`; verbose columns; defaults on extract; `tar o`; the default archive | see the `pax`/`tar` rows above |
| 41 | `compress` | the `-v` text; the status for names already ending in `.Z` | as in Formats; 1 |
| 42 | `ajar` | the option set (`-u -C -m -h`, deleting), the manifest text, the verbose lines | `c t x u` with `v f m 0 M C` |
| 43 | `cmp` | `char` or `byte` in the difference line; the end-of-file message | `char` |
| 44 | `pr` | header spacing and date form, column widths and balancing, a start page past the end | as implemented |
| 45 | `gencat`, `dspmsg` | the system's catalog format; the search order (the working directory last is ours) | our format |
| 46 | `expr` | error statuses and texts; `+5` as an integer; string collation | 2; no; bytes |
| 47 | `locale` | the value when nothing is set, charmap names, where `*LOCALE` objects are listed from | `C`; `IBMCCSIDnnnnn`; `/QSYS.LIB` and `/QSYS.LIB/QSYS.LIB` |
| 48 | `file` | every description's wording; CCSID shown?; links followed by default? | as implemented |

The shell's `printf` writes an octal escape above `\177` as the UTF-8
of that code point, not as one byte (`printf '\351'` is `c3 a9`), so
the corpus keeps raw bytes in seeded files.

## QSH in a job

### Crates

| Crate | What |
|---|---|
| `qsh/qsh-job` | `JobSys`, the `Sys` of a real job; the broker between the shell's threads and the job's thread; `run::batch` (`QSH CMD()`), `run::invoke` (`qsh` arguments), `run::Session` (the line-fed terminal session) |
| `devtools/qshterm` | the QSH Command Entry display as pure logic: the output area, the command line, the keys |
| `qsh/pase` | the PASE mechanism (Linux only): the view of the IFS, the seccomp filter, starting a program in its own namespaces |
| `work/qsjob/src/qsh/` | the job's side: `side.rs` (the `Side` over the job's link and runtime), `cmd.rs` (`QSH`, `STRQSH`), `env.rs` (`ADDENVVAR`, `CHGENVVAR`, `RMVENVVAR`, `WRKENVVAR`), `child.rs` (child jobs, `spawn()`), `pase.rs` (`QP2SHELL`, `QP2SHELL2`, `QP2TERM`, `Qp2RunPase`), `signals.rs` |
| `work/qsjob/src/devtools/qsh.rs` | the terminal session's driver |
| `runtime/sysapi/src/envproc/` | the job's environment variables and the bound procedures `getenv`, `putenv`, `Qp0zGetEnv`, `Qp0zPutEnv`, `Qp0zDltEnv` (`QP0ZCPA`), `Qp0zGetAllSysEnv`, `Qp0zGetSysEnv`, `Qp0zPutSysEnv`, `Qp0zDltSysEnv` (`QP0ZSYSE`), `spawn`, `spawnp`, `wait`, `waitpid` and their `Qp0z` names (`QP0ZSPWN`), `kill` (`QP0SSRV1`), `getpid`, `getppid` (`QP0WSRV1`), `Qp2RunPase` (`QP2USER`) |
| `work/stg/src/qsh.rs`, `lic/slic/src/stgsvc/qsh.rs` | request 1300: the system-level variables (in the user space `QUSRSYS/QP0ZSYSENV`, changes need `*ALLOBJ`) and every profile's UID, GID and home directory |

### The job's `Sys`

A job's runtime (its link to the storage service, the analyzer, the ILE
job) is single-threaded and not `Send`; the shell is `Send + Sync` and
runs on worker threads. `JobSys` turns every operation into a task for a
`Broker`; the job's thread holds the `Desk` and serves tasks against its
`Side` while the shell runs (`run::with_shell`). A task's caller waits for
its answer; closing a file is posted without waiting.

- Files are the job's IFS (`IfsClient`, the `RemoteIfs` over the job's
  link): `open` gives an `IfsFile` stream over the service's descriptor.
  A `/QSYS.LIB` member opens in text mode with CCSID 819 and reads as
  lines without their trailing blanks. `realpath` resolves links and
  gives each name as its directory spells it where names fold.
- `/usr/bin` lists every utility of `qsh-utils` and `sh` and `qsh`, as
  IBM's shows its links to `QSHELL`, whether or not the IFS has the
  directory; they run on threads of the job.
- `/usr/bin/sh` and `/usr/bin/qsh` are the shell itself: a new shell on a
  thread, with the request's arguments, environment and directory.
- `/QSYS.LIB/LIB.LIB/PGM.PGM` (or `/QSYS.LIB/PGM.PGM`) is called in the
  job with the arguments as parameters (`CALL PGM(LIB/PGM) PARM(...)`);
  what it writes to the display is its standard output; 1 when it ended
  with an escape.
- An executable ELF file is a PASE program: it runs in a child job.
  Anything else executable is a script (`ENOEXEC`).
- The hooks: `system` runs CL through the job's analyzer (its output
  lines become messages, an escape last); `db2` runs SQL in the job's SQL
  session (requests 400 and 401); `nslookup`, `host` and `hostname` ask
  the job's resolver (`sysapi::net`, the TCP/IP domain configuration);
  `liblist`, `sysval` and `datarea` reach the job's library list, system
  values, network attributes and data areas; `ps` lists the active jobs;
  user and group names come from the profiles' UIDs; `df` from `statvfs`.
- Process IDs of in-process children start at 4,200,000, above any Linux
  process ID; a signal to one is recorded for its shell, a signal to any
  other process ID goes to that job (below).

### Entry points

- `QSH`/`STRQSH` without a command at a display: the terminal session.
- `QSH CMD(...)`: `run::batch` with `TERMINAL_TYPE=PIPELINE` (`5250` at a
  display). `QIBM_QSH_CMD_OUTPUT`: `STDOUT` (the default) is the terminal
  display in an interactive job (the output, then Enter ends it), the
  spooled file `QPRINT` in a batch or child job and the text of the
  service console's line; `FILE=` and `FILEAPPEND=` write the IFS file
  (standard error too); `NONE` drops it. The ending message is `QSH0005`
  (completion; escape with `QIBM_QSH_CMD_ESCAPE_MSG=Y` and a status above
  0), `QSH0006` (signal) or `QSH0007` (exception), from `QZSHMSGF`
  (`runtime/msgf/msgs/qzshmsgf.toml`, installed in `QSYS`).
- `crate::qsh::child::start_shell(host, user, argv, env, cwd, [stdin,
  stdout, stderr])` in `work/qsjob`: a child job (`QP0ZSPWN`, type `B`,
  subtype `D`) running `qsh` with the given arguments (`["qsh", "-l"]` for
  a login shell, `["qsh", "-c", text]`, a script) as the given user over
  the given descriptors; the returned `ChildJob`'s process ends with the
  shell's status. This is the SSH server's way to a shell.
- `qsh_job::run::invoke(side, setup, argv, stdio)`: the same inside a job
  that already is the shell's.

A shell of a job starts with Qshell's variables (`HOME`, `LOGNAME`,
`PATH=/usr/bin:.:/QOpenSys/usr/bin`, `HOSTNAME`, `HOSTTYPE`, `MACHTYPE`,
`OSTYPE=os400`, `QIBM_DESCRIPTOR_STD*`, `QIBM_USE_DESCRIPTOR_STDIO=Y`,
`SHLVL`, `TERMINAL_TYPE`, `TRACEOPT`) overlaid by the job's environment
variables, in the user's home directory when it exists.

### The terminal session

QSH Command Entry (`devtools/qshterm`, against the goldens `qsh-start`
and `qsh-session`): the title on row 1, fourteen rows of output from
row 3 at column 4, `===>` on row 18 with a four-row command field, two
rows of keys and the message row. A typed command is echoed as
`> command`; the prompt (`$`) follows each command's output. Long lines
wrap at 76 columns; F11 cuts them instead.

| Key | Action |
|---|---|
| Enter | the line to the shell (a command, or input for a running command that reads) |
| F3 | end the shell and leave |
| F5 | show the display again |
| F6 | print the output to `QPRINT` |
| F7, F8, Page Up, Page Down | page the output |
| F9 | retrieve earlier commands, newest first |
| F11 | wrap or cut long lines |
| F12 | leave; the shell waits in the job and the next `STRQSH` resumes it |
| F13 | clear the output |
| F17, F18 | the top, the bottom |
| F21 | run the command line as a CL command (IBM opens a command entry window) |

The shell runs on its own thread while the job's thread serves it and
redraws the output every half second; a command that reads its standard
input gets the next line typed.

### Process model

- Child jobs (`work/qsjob/src/qsh/child.rs`) are `qsjob` processes the job
  starts with the role `SPAWN`, batch immediate (`BCI`), named
  `QP0ZSPWN` (the parent's name with `SPAWN_SETJOBNAMEPARENT_NP`, the
  program's with `SPAWN_SETJOBNAMEARGV_NP`), in the parent's subsystem,
  job description, class and pool (cgroup). The request data (hex of a
  wire record) names what to run (the shell, a PASE program or a `*PGM`),
  the arguments, the environment, the directory, the descriptor that is
  standard input and whether `QIBM_MULTI_THREADED=Y` was set (stored in
  the request data and the child's environment). Standard output and
  error are the process's own.
- `spawn()` and `spawnp()` (`QP0ZSPWN`) start one; `spawnp` searches
  `PATH`. The child's input is the null device and its output and error
  the job's own: descriptors of the map other than `SPAWN_FDCLOSED` are
  not passed (the job's IFS and socket descriptors live in the storage
  service and the network layer; passing them is the host servers'
  descriptor-passing work). `waitpid()` and `wait()` give the status in
  the encoding of `<sys/wait.h>` (exit status in the low byte,
  `0x00020000` with the signal), `WNOHANG` honoured.
- `kill()` and the shell's `kill` to another job translate the system's
  signal number to Linux's by name (`signals.rs`) and signal its process;
  a job of another user needs `*JOBCTL`. A child ended by a signal
  reports the system's number.
- A shell's program that is a PASE program runs in a child job; its
  streams are copied through pipes.

### Environment variables

`ADDENVVAR`, `CHGENVVAR`, `RMVENVVAR` and `WRKENVVAR` with IBM's
parameters (checked against the oracle's definitions and prompts,
`command/sys-cpps/tests/qsh_defs.rs`). `LEVEL(*JOB)` changes the job's
variables, kept in the job's process; `LEVEL(*SYS)` the system's, kept
by the service in `QUSRSYS/QP0ZSYSENV` and changed only with `*ALLOBJ`
(`CPFA98E`). A job starts with a copy of the system's. Messages:
`CPCA980`, `CPCA981`, `CPCA988`, `CPFA980`, `CPFA981`, `CPFA984`.
`WRKENVVAR` is the `QUIENVVAR` panel (goldens `wrkenvvar` and
`wrkenvvar-sys`): options 1 (add, on the blank first entry), 2, 4, 5 and
6, F11 shows the CCSIDs, F17 and F18 page to the ends; names over 24
characters and values over 34 are cut with `>` as IBM's. Without a
display it lists as text.

### PASE

`QP2SHELL` and `QP2SHELL2` (`QSYS` programs: a path and argument
strings), `QP2TERM` (with a program as `QP2SHELL`; with none it needs a
PASE shell at `/QOpenSys/usr/bin/sh`, which QS/400 does not ship),
`Qp2RunPase`, and a shell running an ELF file start the program through
`qsh/pase`:

1. The job forks a keeper, which unshares its user, mount and PID
   namespaces and waits; the job writes the keeper's maps (root to root,
   so the view can be built, and the profile's UID and GID to themselves)
   and lets it go on.
2. The keeper forks the program's process, PID 1 of the new namespace.
3. That process makes its mounts private, mounts a `tmpfs` on
   `/run/qs400/pase` and builds the view: every top-level entry of
   `/QOpenSys` (directories and files bind-mounted, symbolic links
   copied; taken from `/run/qs400/qopensys`, the second mount of
   `/QOpenSys` that `slic` makes with programs allowed, since the IFS
   mount is `noexec`) so `/usr`, `/etc`, `/tmp`, `/var` and `/bin` are those of
   `/QOpenSys`, as an AIX program built for PASE expects; `/QOpenSys`
   itself, so the IFS spelling works; `/home` from the root file system;
   `/dev` with the host's null, zero, full, random, urandom and tty;
   `/proc` of the new PID namespace. `/QSYS.LIB` and the rest of the root
   file system are not in the view: they are the storage service's, not
   files.
4. It pivots its root there and detaches the old one (when the root is
   the initramfs, which `pivot_root` refuses, it moves the view over `/`
   and changes its root into it, as `switch_root` does; the old root
   stays covered), makes the root
   read-only, changes to the working directory (when in the view), sets
   standard input, output and error, drops every other descriptor at
   exec, clears its signal mask and supplementary groups, takes the
   profile's GID and UID, sets `no_new_privs` and installs the seccomp
   filter, then executes the program.
5. The keeper ends as the program did; the job waits for the keeper.

The program is in the job's pool cgroup (a descendant of the job's
process). The filter (`qsh/pase/src/seccomp.rs`) is a classic BPF
program written here: anything but `x86_64` is killed, x32 numbers and
every call not on the list fail with `EPERM`; the list is what a
userland needs (files, memory, signals, sockets, processes, time,
`futex`, `epoll`, `getrandom`, `rseq` and the rest) and leaves out
mounting, tracing, modules, rebooting, keyrings, BPF, namespaces,
`io_uring`, pidfds and setting the clock. The kernel needs
`CONFIG_USER_NS` and `CONFIG_PID_NS` (`kernel/config/qs400.config`,
checked by the build).

The fork happens in a multi-threaded process, so everything the children
use (paths, arguments, the filter) is prepared before it and the
children make system calls only; this is the one `unsafe` site,
`qsh/pase/src/platform.rs`.

Tests: the `qsh/pase` unit tests (the filter run by an interpreter of the
same semantics, the view's plan); `qsh/pase/tests/linux.rs` as root
(`wsl -d Ubuntu -u root -- bash qsh/pase/tests/linux-root.sh`) runs
`tools/pase-probe` (a test fixture built for the musl target, never
shipped) from a fake IFS: it sees UID and GID 1234 and 1235, is PID 1
with no other process in `/proc`, sees `/QOpenSys/tmp` and `/home`, and
`pidfd_open` is refused with `EPERM`. QEMU step 65 pushes the probe into
`/QOpenSys/tmp` and runs it with `QP2SHELL` and from `qsh`.

### Tests (QSH in a job)

- `qsh/qsh-job/tests/corpus.rs`: the utilities' `text-*` and `misc-*`
  corpus files (769 cases; 13 that read the clock or objects only the
  `MemSys` seed has are skipped) run over `JobSys` and a real IFS
  (`LocalIfs` over the memory backend, seeded as the `MemSys` corpus is,
  owned by a user without special authority); standard output, standard
  error and status must match.
- `qsh/qsh-job/tests/job.rs`: the shell as `/usr/bin/sh` and `qsh`, the
  `/usr/bin` listing, programs called in the job, `system`, scripts in
  the IFS, and every `QIBM_QSH_CMD_OUTPUT` mode and ending message.
- `devtools/qshterm/tests/screens.rs`: the session displays against the
  pub400 goldens; `ui/uim/tests/envvar_panels.rs`: `WRKENVVAR` at both
  levels; `command/sys-cpps/tests/qsh_defs.rs`: the six commands'
  definitions and prompts.
- QEMU step 65: `ADDENVVAR LEVEL(*SYS)`, `QSH CMD()` at the console, a
  submitted `QSH CMD('ls /home; echo $?')` whose `QPRINT` is read back,
  and the PASE probe through `QP2SHELL` and `qsh`.

### Open questions (QSH in a job)

1. IBM runs a shell's programs in child jobs; QS/400 runs `*PGM` objects
   in the shell's job and only PASE programs in children.
2. `QSH CMD()` with `STDOUT` at a display: IBM's terminal shows output as
   it comes; ours shows it when the command ends.
3. F21: IBM opens a command entry window; ours runs the command line.
4. `QSH0003` and `QSH0004` are not sent at session start and end.
5. `spawn()` descriptor maps beyond `SPAWN_FDCLOSED`.
6. The top level of the PASE view is rebuilt each start: a file created
   directly in its `/` is not kept.
7. `QZSHMSGF` is installed in `QSYS`; IBM's is in `QSHELL`, a library
   QS/400 does not have.
