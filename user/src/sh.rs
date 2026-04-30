#![no_std]
#![no_main]

// Shell.

use ulib::*;

const MAXARGS: usize = 10;

// Parsed command types
const EXEC: i32 = 1;
const REDIR: i32 = 2;
const PIPE: i32 = 3;
const LIST: i32 = 4;
const BACK: i32 = 5;

// Command representations — we use tagged enum pointers via raw allocation.
// Each struct starts with a `cmd_type` field.

#[repr(C)]
struct Cmd {
    cmd_type: i32,
}

#[repr(C)]
struct ExecCmd {
    cmd_type: i32,
    argv: [*mut u8; MAXARGS],
    eargv: [*mut u8; MAXARGS],
}

#[repr(C)]
struct RedirCmd {
    cmd_type: i32,
    cmd: *mut Cmd,
    file: *mut u8,
    efile: *mut u8,
    mode: i32,
    fd: i32,
}

#[repr(C)]
struct PipeCmd {
    cmd_type: i32,
    left: *mut Cmd,
    right: *mut Cmd,
}

#[repr(C)]
struct ListCmd {
    cmd_type: i32,
    left: *mut Cmd,
    right: *mut Cmd,
}

#[repr(C)]
struct BackCmd {
    cmd_type: i32,
    cmd: *mut Cmd,
}

fn panic(s: &[u8]) -> ! {
    write(2, s);
    write(2, b"\n");
    exit();
}

fn fork1() -> i32 {
    let pid = fork();
    if pid == -1 {
        panic(b"fork");
    }
    pid
}

// Execute cmd. Never returns.
fn runcmd(cmd: *mut Cmd) -> ! {
    if cmd.is_null() {
        exit();
    }

    unsafe {
        match (*cmd).cmd_type {
            EXEC => {
                let ecmd = cmd as *mut ExecCmd;
                if (*ecmd).argv[0].is_null() {
                    exit();
                }
                // Build null-terminated argv for exec
                let mut argv_ptrs: [*const u8; MAXARGS + 1] = [core::ptr::null(); MAXARGS + 1];
                let mut i = 0;
                while i < MAXARGS && !(*ecmd).argv[i].is_null() {
                    argv_ptrs[i] = (*ecmd).argv[i] as *const u8;
                    i += 1;
                }
                argv_ptrs[i] = core::ptr::null();
                let name = core::slice::from_raw_parts((*ecmd).argv[0], strlen((*ecmd).argv[0]) + 1);
                exec(name, &argv_ptrs[..i + 1]);
                printf!(2, "exec failed\n");
            }
            REDIR => {
                let rcmd = cmd as *mut RedirCmd;
                close((*rcmd).fd);
                let file = core::slice::from_raw_parts((*rcmd).file as *const u8, strlen((*rcmd).file as *const u8) + 1);
                if open(file, (*rcmd).mode) < 0 {
                    printf!(2, "open failed\n");
                    exit();
                }
                runcmd((*rcmd).cmd);
            }
            LIST => {
                let lcmd = cmd as *mut ListCmd;
                if fork1() == 0 {
                    runcmd((*lcmd).left);
                }
                wait();
                runcmd((*lcmd).right);
            }
            PIPE => {
                let pcmd = cmd as *mut PipeCmd;
                let mut p = [0i32; 2];
                if pipe(&mut p) < 0 {
                    panic(b"pipe");
                }
                if fork1() == 0 {
                    close(1);
                    dup(p[1]);
                    close(p[0]);
                    close(p[1]);
                    runcmd((*pcmd).left);
                }
                if fork1() == 0 {
                    close(0);
                    dup(p[0]);
                    close(p[0]);
                    close(p[1]);
                    runcmd((*pcmd).right);
                }
                close(p[0]);
                close(p[1]);
                wait();
                wait();
            }
            BACK => {
                let bcmd = cmd as *mut BackCmd;
                if fork1() == 0 {
                    runcmd((*bcmd).cmd);
                }
            }
            _ => {
                panic(b"runcmd");
            }
        }
    }
    exit();
}

fn getcmd(buf: &mut [u8]) -> i32 {
    write(2, b"$ ");
    memset(buf.as_mut_ptr(), 0, buf.len());
    gets(buf);
    if buf[0] == 0 {
        return -1; // EOF
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> ! {
    // Ensure three file descriptors are open.
    loop {
        let fd = open(b"console\0", O_RDWR);
        if fd >= 3 {
            close(fd);
            break;
        }
        if fd < 0 {
            break;
        }
    }

    let mut buf = [0u8; 100];

    // Read and run input commands.
    while getcmd(&mut buf) >= 0 {
        // Handle cd specially
        if buf[0] == b'c' && buf[1] == b'd' && buf[2] == b' ' {
            // Chop newline
            let len = buf.iter().position(|&b| b == b'\n' || b == 0).unwrap_or(buf.len());
            if len > 0 && buf[len - 1] == b'\n' {
                buf[len - 1] = 0;
            }
            // Null terminate at end
            buf[len] = 0;
            if chdir(&buf[3..len + 1]) < 0 {
                printf!(2, "cannot cd\n");
            }
            continue;
        }

        if fork1() == 0 {
            runcmd(parsecmd(&mut buf));
        }
        wait();
    }
    exit();
}

// ============================================================================
// Constructors
// ============================================================================

unsafe fn make_execcmd() -> *mut Cmd {
    let cmd = malloc(core::mem::size_of::<ExecCmd>()) as *mut ExecCmd;
    memset(cmd as *mut u8, 0, core::mem::size_of::<ExecCmd>());
    (*cmd).cmd_type = EXEC;
    cmd as *mut Cmd
}

unsafe fn make_redircmd(subcmd: *mut Cmd, file: *mut u8, efile: *mut u8, mode: i32, fd: i32) -> *mut Cmd {
    let cmd = malloc(core::mem::size_of::<RedirCmd>()) as *mut RedirCmd;
    memset(cmd as *mut u8, 0, core::mem::size_of::<RedirCmd>());
    (*cmd).cmd_type = REDIR;
    (*cmd).cmd = subcmd;
    (*cmd).file = file;
    (*cmd).efile = efile;
    (*cmd).mode = mode;
    (*cmd).fd = fd;
    cmd as *mut Cmd
}

unsafe fn make_pipecmd(left: *mut Cmd, right: *mut Cmd) -> *mut Cmd {
    let cmd = malloc(core::mem::size_of::<PipeCmd>()) as *mut PipeCmd;
    memset(cmd as *mut u8, 0, core::mem::size_of::<PipeCmd>());
    (*cmd).cmd_type = PIPE;
    (*cmd).left = left;
    (*cmd).right = right;
    cmd as *mut Cmd
}

unsafe fn make_listcmd(left: *mut Cmd, right: *mut Cmd) -> *mut Cmd {
    let cmd = malloc(core::mem::size_of::<ListCmd>()) as *mut ListCmd;
    memset(cmd as *mut u8, 0, core::mem::size_of::<ListCmd>());
    (*cmd).cmd_type = LIST;
    (*cmd).left = left;
    (*cmd).right = right;
    cmd as *mut Cmd
}

unsafe fn make_backcmd(subcmd: *mut Cmd) -> *mut Cmd {
    let cmd = malloc(core::mem::size_of::<BackCmd>()) as *mut BackCmd;
    memset(cmd as *mut u8, 0, core::mem::size_of::<BackCmd>());
    (*cmd).cmd_type = BACK;
    (*cmd).cmd = subcmd;
    cmd as *mut Cmd
}

// ============================================================================
// Parsing
// ============================================================================

const WHITESPACE: &[u8] = b" \t\r\n\x0b";
const SYMBOLS: &[u8] = b"<|>&;()";

fn is_in(c: u8, set: &[u8]) -> bool {
    set.contains(&c)
}

/// Get the next token from the input string.
/// Returns the token character, and optionally sets *q and *eq to the
/// start and end of the token text.
unsafe fn gettoken(ps: &mut *mut u8, es: *mut u8, q: &mut *mut u8, eq: &mut *mut u8) -> u8 {
    let mut s = *ps;

    // Skip whitespace
    while s < es && is_in(*s, WHITESPACE) {
        s = s.add(1);
    }
    *q = s;

    let ret;
    match *s {
        0 => {
            ret = 0;
        }
        b'|' | b'(' | b')' | b';' | b'&' | b'<' => {
            ret = *s;
            s = s.add(1);
        }
        b'>' => {
            s = s.add(1);
            if *s == b'>' {
                ret = b'+'; // >> append
                s = s.add(1);
            } else {
                ret = b'>';
            }
        }
        _ => {
            ret = b'a'; // word token
            while s < es && !is_in(*s, WHITESPACE) && !is_in(*s, SYMBOLS) {
                s = s.add(1);
            }
        }
    }
    *eq = s;

    // Skip trailing whitespace
    while s < es && is_in(*s, WHITESPACE) {
        s = s.add(1);
    }
    *ps = s;
    ret
}

unsafe fn peek(ps: &mut *mut u8, es: *mut u8, toks: &[u8]) -> bool {
    let mut s = *ps;
    while s < es && is_in(*s, WHITESPACE) {
        s = s.add(1);
    }
    *ps = s;
    *s != 0 && is_in(*s, toks)
}

fn parsecmd(buf: &mut [u8]) -> *mut Cmd {
    unsafe {
        let mut s = buf.as_mut_ptr();
        let es = buf.as_mut_ptr().add(strlen(buf.as_ptr()));
        let cmd = parseline(&mut s, es);
        peek(&mut s, es, b"");
        if s != es {
            printf!(2, "leftovers in command\n");
            panic(b"syntax");
        }
        nulterminate(cmd);
        cmd
    }
}

unsafe fn parseline(ps: &mut *mut u8, es: *mut u8) -> *mut Cmd {
    let mut cmd = parsepipe(ps, es);
    while peek(ps, es, b"&") {
        let mut q: *mut u8 = core::ptr::null_mut();
        let mut eq: *mut u8 = core::ptr::null_mut();
        gettoken(ps, es, &mut q, &mut eq);
        cmd = make_backcmd(cmd);
    }
    if peek(ps, es, b";") {
        let mut q: *mut u8 = core::ptr::null_mut();
        let mut eq: *mut u8 = core::ptr::null_mut();
        gettoken(ps, es, &mut q, &mut eq);
        cmd = make_listcmd(cmd, parseline(ps, es));
    }
    cmd
}

unsafe fn parsepipe(ps: &mut *mut u8, es: *mut u8) -> *mut Cmd {
    let mut cmd = parseexec(ps, es);
    if peek(ps, es, b"|") {
        let mut q: *mut u8 = core::ptr::null_mut();
        let mut eq: *mut u8 = core::ptr::null_mut();
        gettoken(ps, es, &mut q, &mut eq);
        cmd = make_pipecmd(cmd, parsepipe(ps, es));
    }
    cmd
}

unsafe fn parseredirs(mut cmd: *mut Cmd, ps: &mut *mut u8, es: *mut u8) -> *mut Cmd {
    while peek(ps, es, b"<>") {
        let mut q: *mut u8 = core::ptr::null_mut();
        let mut eq: *mut u8 = core::ptr::null_mut();
        let tok = gettoken(ps, es, &mut q, &mut eq);
        let mut file_q: *mut u8 = core::ptr::null_mut();
        let mut file_eq: *mut u8 = core::ptr::null_mut();
        if gettoken(ps, es, &mut file_q, &mut file_eq) != b'a' {
            panic(b"missing file for redirection");
        }
        match tok {
            b'<' => {
                cmd = make_redircmd(cmd, file_q, file_eq, O_RDONLY, 0);
            }
            b'>' => {
                cmd = make_redircmd(cmd, file_q, file_eq, O_WRONLY | O_CREATE, 1);
            }
            b'+' => {
                // >> append
                cmd = make_redircmd(cmd, file_q, file_eq, O_WRONLY | O_CREATE, 1);
            }
            _ => {}
        }
    }
    cmd
}

unsafe fn parseblock(ps: &mut *mut u8, es: *mut u8) -> *mut Cmd {
    if !peek(ps, es, b"(") {
        panic(b"parseblock");
    }
    let mut q: *mut u8 = core::ptr::null_mut();
    let mut eq: *mut u8 = core::ptr::null_mut();
    gettoken(ps, es, &mut q, &mut eq);
    let cmd = parseline(ps, es);
    if !peek(ps, es, b")") {
        panic(b"syntax - missing )");
    }
    gettoken(ps, es, &mut q, &mut eq);
    parseredirs(cmd, ps, es)
}

unsafe fn parseexec(ps: &mut *mut u8, es: *mut u8) -> *mut Cmd {
    if peek(ps, es, b"(") {
        return parseblock(ps, es);
    }

    let ret = make_execcmd();
    let ecmd = ret as *mut ExecCmd;

    let mut argc = 0usize;
    let mut ret = parseredirs(ret, ps, es);
    while !peek(ps, es, b"|)&;") {
        let mut q: *mut u8 = core::ptr::null_mut();
        let mut eq: *mut u8 = core::ptr::null_mut();
        let tok = gettoken(ps, es, &mut q, &mut eq);
        if tok == 0 {
            break;
        }
        if tok != b'a' {
            panic(b"syntax");
        }
        (*ecmd).argv[argc] = q;
        (*ecmd).eargv[argc] = eq;
        argc += 1;
        if argc >= MAXARGS {
            panic(b"too many args");
        }
        ret = parseredirs(ret, ps, es);
    }
    (*ecmd).argv[argc] = core::ptr::null_mut();
    (*ecmd).eargv[argc] = core::ptr::null_mut();
    ret
}

/// NUL-terminate all the counted strings.
unsafe fn nulterminate(cmd: *mut Cmd) -> *mut Cmd {
    if cmd.is_null() {
        return core::ptr::null_mut();
    }

    match (*cmd).cmd_type {
        EXEC => {
            let ecmd = cmd as *mut ExecCmd;
            let mut i = 0;
            while !(*ecmd).argv[i].is_null() {
                *(*ecmd).eargv[i] = 0;
                i += 1;
            }
        }
        REDIR => {
            let rcmd = cmd as *mut RedirCmd;
            nulterminate((*rcmd).cmd);
            *(*rcmd).efile = 0;
        }
        PIPE => {
            let pcmd = cmd as *mut PipeCmd;
            nulterminate((*pcmd).left);
            nulterminate((*pcmd).right);
        }
        LIST => {
            let lcmd = cmd as *mut ListCmd;
            nulterminate((*lcmd).left);
            nulterminate((*lcmd).right);
        }
        BACK => {
            let bcmd = cmd as *mut BackCmd;
            nulterminate((*bcmd).cmd);
        }
        _ => {}
    }
    cmd
}
