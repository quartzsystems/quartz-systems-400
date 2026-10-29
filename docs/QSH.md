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
| `file/` | `cat chmod chgrp chown cp df du find ln ls mkdir mv rm rmdir touch` |
| `text/` | `cut expand grep egrep fgrep head iconv od paste sed sort split tail tee tr uniq wc`; `sed/` is `parse.rs` (the script) and `exec.rs` (the cycle) |
| `proc/` | `basename date dirname echo env ps sleep true false uname xargs` |

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

### Tests

- `tests/corpus/*.sht`: 853 cases over the seeded `MemSys` (a tree
  under `/home/QSHUSER` with data files, a hidden file, a link, an old
  file owned by `QSYS`, raw-byte files, a `/QSYS.LIB` library with a
  source file, member, program and data area, and `/usr/bin/sh` running
  scripts in the same system), one file per group of utilities;
  `QSH_UPDATE_CORPUS=1` rewrites the expectations, `QSH_CORPUS_ONLY=x`
  runs matching cases.
- `tests/regex.rs`: a table of POSIX spans, groups, errors, case
  folding, `find_all` stepping, pathological patterns that must finish,
  and 4,000 random patterns checked against a brute-force matcher plus
  20,000 random patterns that must not panic.
- `tests/torture.rs`: `sort` with each key and flag combination against
  a reference sort over random records, numeric sorting against a float
  reference, `sed` scripts against string references, a 100,000-byte
  line, 20,000 lines and pathological loops.

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

The shell's `printf` writes an octal escape above `\177` as the UTF-8
of that code point, not as one byte (`printf '\351'` is `c3 a9`), so
the corpus keeps raw bytes in seeded files.
