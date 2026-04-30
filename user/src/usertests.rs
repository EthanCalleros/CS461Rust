#![no_std]
#![no_main]

use ulib::*;

const MAXFILE: usize = 140;
const KERNBASE: usize = 0x80000000;
const BIG: usize = 100 * 1024 * 1024;
const MAXARG: usize = 32;
const RTC_ADDR: u16 = 0x70;
const RTC_DATA: u16 = 0x71;

static mut BUF: [u8; 8192] = [0; 8192];
static mut NAME: [u8; 3] = [0; 3];

fn failexit(msg: &[u8]) {
    printf!(1, "!! FAILED {}\n", core::str::from_utf8(msg).unwrap_or("???"));
    exit();
}

// does chdir() call iput(p->cwd) in a transaction?
fn iputtest() {
    printf!(1, "iput test\n");

    if mkdir(b"iputdir\0") < 0 {
        failexit(b"mkdir\0");
    }
    if chdir(b"iputdir\0") < 0 {
        failexit(b"chdir iputdir\0");
    }
    if unlink(b"../iputdir\0") < 0 {
        failexit(b"unlink ../iputdir\0");
    }
    if chdir(b"/\0") < 0 {
        failexit(b"chdir /\0");
    }
    printf!(1, "iput test ok\n");
}

// does exit() call iput(p->cwd) in a transaction?
fn exitiputtest() {
    printf!(1, "exitiput test\n");

    let pid = fork();
    if pid < 0 {
        failexit(b"fork\0");
    }
    if pid == 0 {
        if mkdir(b"iputdir\0") < 0 {
            failexit(b"mkdir\0");
        }
        if chdir(b"iputdir\0") < 0 {
            failexit(b"child chdir\0");
        }
        if unlink(b"../iputdir\0") < 0 {
            failexit(b"unlink ../iputdir\0");
        }
        exit();
    }
    wait();
    printf!(1, "exitiput test ok\n");
}

// does the error path in open() for attempt to write a
// directory call iput() in a transaction?
fn openiputtest() {
    printf!(1, "openiput test\n");
    if mkdir(b"oidir\0") < 0 {
        failexit(b"mkdir oidir\0");
    }
    let pid = fork();
    if pid < 0 {
        failexit(b"fork\0");
    }
    if pid == 0 {
        let fd = open(b"oidir\0", O_RDWR);
        if fd >= 0 {
            failexit(b"open directory for write succeeded\0");
        }
        exit();
    }
    sleep(1);
    if unlink(b"oidir\0") != 0 {
        failexit(b"unlink\0");
    }
    wait();
    printf!(1, "openiput test ok\n");
}

// simple file system tests

fn opentest() {
    printf!(1, "open test\n");
    let fd = open(b"echo\0", 0);
    if fd < 0 {
        failexit(b"open echo\0");
    }
    close(fd);
    let fd = open(b"doesnotexist\0", 0);
    if fd >= 0 {
        failexit(b"open doesnotexist succeeded!\0");
    }
    printf!(1, "open test ok\n");
}

fn writetest() {
    printf!(1, "small file test\n");
    let fd = open(b"small\0", O_CREATE | O_RDWR);
    if fd < 0 {
        failexit(b"error: creat small\0");
    }
    for i in 0..100 {
        unsafe {
            if write(fd, b"aaaaaaaaaa") != 10 {
                printf!(1, "error: write aa {} new file failed\n", i);
                exit();
            }
            if write(fd, b"bbbbbbbbbb") != 10 {
                printf!(1, "error: write bb {} new file failed\n", i);
                exit();
            }
        }
    }
    close(fd);
    let fd = open(b"small\0", O_RDONLY);
    if fd < 0 {
        failexit(b"error: open small\0");
    }
    unsafe {
        let i = read(fd, &mut BUF[..2000]);
        if i != 2000 {
            failexit(b"read\0");
        }
    }
    close(fd);

    if unlink(b"small\0") < 0 {
        failexit(b"unlink small\0");
        exit();
    }
    printf!(1, "small file test ok\n");
}

fn writetest1() {
    printf!(1, "big files test\n");

    let fd = open(b"big\0", O_CREATE | O_RDWR);
    if fd < 0 {
        failexit(b"error: creat big\0");
    }

    unsafe {
        for i in 0..MAXFILE {
            let ptr = BUF.as_mut_ptr() as *mut i32;
            *ptr = i as i32;
            if write(fd, &BUF[..512]) != 512 {
                failexit(b"error: write big file\0");
            }
        }
    }

    close(fd);

    let fd = open(b"big\0", O_RDONLY);
    if fd < 0 {
        failexit(b"error: open big\0");
    }

    let mut n = 0;
    loop {
        unsafe {
            let i = read(fd, &mut BUF[..512]);
            if i == 0 {
                if n == MAXFILE as i32 - 1 {
                    printf!(1, "read only {} blocks from big. failed", n);
                    exit();
                }
                break;
            } else if i != 512 {
                printf!(1, "read failed {}\n", i);
                exit();
            }
            let ptr = BUF.as_ptr() as *const i32;
            if *ptr != n {
                printf!(1, "read content of block {} is {}. failed\n", n, *ptr);
                exit();
            }
            n += 1;
        }
    }
    close(fd);
    if unlink(b"big\0") < 0 {
        failexit(b"unlink big\0");
        exit();
    }
    printf!(1, "big files ok\n");
}

fn createtest() {
    printf!(1, "many creates, followed by unlink test\n");

    unsafe {
        NAME[0] = b'a';
        NAME[2] = 0;
        for i in 0..52 {
            NAME[1] = b'0' + i;
            let fd = open(&NAME[..3], O_CREATE | O_RDWR);
            close(fd);
        }
        for i in 0..52 {
            NAME[1] = b'0' + i;
            unlink(&NAME[..3]);
        }
        for i in 0..52 {
            NAME[1] = b'0' + i;
            let fd = open(&NAME[..3], O_RDWR);
            if fd >= 0 {
                failexit(b"open should fail.\0");
            }
        }
    }

    printf!(1, "many creates, followed by unlink; ok\n");
}

fn dirtest() {
    printf!(1, "mkdir test\n");

    if mkdir(b"dir0\0") < 0 {
        failexit(b"mkdir\0");
    }

    if chdir(b"dir0\0") < 0 {
        failexit(b"chdir dir0\0");
    }

    if chdir(b"..\0") < 0 {
        failexit(b"chdir ..\0");
    }

    if unlink(b"dir0\0") < 0 {
        failexit(b"unlink dir0\0");
    }
    printf!(1, "mkdir test ok\n");
}

fn exectest() {
    printf!(1, "exec test\n");
    let mut argv = [
        b"echo\0".as_ptr() as *const u8,
        b"ALL\0".as_ptr() as *const u8,
        b"TESTS\0".as_ptr() as *const u8,
        b"PASSED\0".as_ptr() as *const u8,
        0 as *const u8,
    ];
    if exec(b"echo\0", &argv) < 0 {
        failexit(b"exec echo\0");
    }
    printf!(1, "exec test ok\n");
}

fn nullptrtest() {
    printf!(1, "null pointer test\n");
    printf!(1, "expect one killed process\n");
    let ppid = getpid();
    if fork() == 0 {
        unsafe {
            *(0 as *mut u64) = 10;
        }
        printf!(1, "can write to unmapped page 0, failed");
        kill(ppid);
        exit();
    } else {
        wait();
    }
    printf!(1, "null pointer test ok\n");
}

// simple fork and pipe read/write

fn pipe1() {
    printf!(1, "pipe1 starting\n");
    let mut fds = [0; 2];
    if pipe(&mut fds) != 0 {
        failexit(b"pipe()\0");
    }
    let pid = fork();
    let mut seq: u8 = 0;
    if pid == 0 {
        close(fds[0]);
        for _n in 0..5 {
            unsafe {
                for i in 0..1033 {
                    BUF[i] = seq;
                    seq = seq.wrapping_add(1);
                }
                if write(fds[1], &BUF[..1033]) != 1033 {
                    failexit(b"pipe1 oops 1\0");
                }
            }
        }
        exit();
    } else if pid > 0 {
        close(fds[1]);
        let mut total = 0;
        let mut cc = 1;
        loop {
            unsafe {
                let n = read_raw(fds[0], BUF.as_mut_ptr(), cc as usize);
                if n <= 0 {
                    break;
                }
                for i in 0..n as usize {
                    if (BUF[i] & 0xff) != (seq & 0xff) {
                        failexit(b"pipe1 oops 2\0");
                    }
                    seq = seq.wrapping_add(1);
                }
                total += n;
                cc = cc * 2;
                if cc > 8192 {
                    cc = 8192;
                }
            }
        }
        if total != 5 * 1033 {
            printf!(1, "pipe1 oops 3 total {}\n", total);
            exit();
        }
        close(fds[0]);
        wait();
    } else {
        failexit(b"fork()\0");
    }
    printf!(1, "pipe1 ok\n");
}

// meant to be run w/ at most two CPUs
fn preempt() {
    printf!(1, "preempt: ");
    let pid1 = fork();
    if pid1 == 0 {
        loop {}
    }

    let pid2 = fork();
    if pid2 == 0 {
        loop {}
    }

    let mut pfds = [0; 2];
    pipe(&mut pfds);
    let pid3 = fork();
    if pid3 == 0 {
        close(pfds[0]);
        if write(pfds[1], b"x") != 1 {
            printf!(1, "preempt write error");
        }
        close(pfds[1]);
        loop {}
    }

    close(pfds[1]);
    unsafe {
        if read(pfds[0], &mut BUF[..8192]) != 1 {
            printf!(1, "preempt read error");
            return;
        }
    }
    close(pfds[0]);
    printf!(1, "kill... ");
    kill(pid1);
    kill(pid2);
    kill(pid3);
    printf!(1, "wait... ");
    wait();
    wait();
    wait();
    printf!(1, "preempt ok\n");
}

// try to find any races between exit and wait
fn exitwait() {
    for i in 0..100 {
        let pid = fork();
        if pid < 0 {
            printf!(1, "fork");
            return;
        }
        if pid != 0 {
            if wait() != pid {
                printf!(1, "wait wrong pid\n");
                return;
            }
        } else {
            exit();
        }
    }
    printf!(1, "exitwait ok\n");
}

fn mem() {
    printf!(1, "mem test\n");
    let ppid = getpid();
    if fork() == 0 {
        let mut m1: *mut u8 = core::ptr::null_mut();
        loop {
            let m2 = malloc(100001);
            if m2 as usize == 0 {
                break;
            }
            unsafe {
                *(m2 as *mut *mut u8) = m1;
            }
            m1 = m2;
        }
        printf!(1, "alloc ended\n");
        while !m1.is_null() {
            unsafe {
                let m2 = *(m1 as *mut *mut u8);
                free(m1);
                m1 = m2;
            }
        }
        let m1 = malloc(1024 * 20);
        if m1 as usize == 0 {
            printf!(1, "couldn't allocate mem?!!\n");
            kill(ppid);
            exit();
        }
        free(m1);
        printf!(1, "mem ok\n");
        exit();
    } else {
        wait();
    }
}

// More file system tests

// two processes write to the same file descriptor
// is the offset shared? does inode locking work?
fn sharedfd() {
    printf!(1, "sharedfd test\n");

    unlink(b"sharedfd\0");
    let fd = open(b"sharedfd\0", O_CREATE | O_RDWR);
    if fd < 0 {
        printf!(1, "fstests: cannot open sharedfd for writing");
        return;
    }
    let pid = fork();
    unsafe {
        let c: u8 = if pid == 0 { b'c' } else { b'p' };
        let mut buf = [0u8; 10];
        for _ in 0..10 {
            buf[0] = c;
        }
        for _i in 0..1000 {
            if write(fd, &buf) != 10 {
                printf!(1, "fstests: write sharedfd failed\n");
                break;
            }
        }
    }
    if pid == 0 {
        exit();
    } else {
        wait();
    }
    close(fd);
    let fd = open(b"sharedfd\0", 0);
    if fd < 0 {
        printf!(1, "fstests: cannot open sharedfd for reading\n");
        return;
    }
    let mut nc = 0;
    let mut np = 0;
    unsafe {
        let mut buf = [0u8; 10];
        loop {
            let n = read(fd, &mut buf[..10]);
            if n <= 0 {
                break;
            }
            for i in 0..n as usize {
                if buf[i] == b'c' {
                    nc += 1;
                }
                if buf[i] == b'p' {
                    np += 1;
                }
            }
        }
    }
    close(fd);
    unlink(b"sharedfd\0");
    if nc == 10000 && np == 10000 {
        printf!(1, "sharedfd ok\n");
    } else {
        printf!(1, "sharedfd oops {} {}\n", nc, np);
        exit();
    }
}

// four processes write different files at the same
// time, to test block allocation.
fn fourfiles() {
    printf!(1, "fourfiles test\n");

    let names = [b"f0\0", b"f1\0", b"f2\0", b"f3\0"];

    for pi in 0..4 {
        let fname = names[pi];
        unlink(fname);

        let pid = fork();
        if pid < 0 {
            failexit(b"fork\0");
        }

        if pid == 0 {
            let fd = open(fname, O_CREATE | O_RDWR);
            if fd < 0 {
                failexit(b"create\0");
            }

            unsafe {
                memset(BUF.as_mut_ptr(), b'0' + pi as u8, 512);
                for _i in 0..12 {
                    let n = write(fd, &BUF[..500]);
                    if n != 500 {
                        printf!(1, "write failed {}\n", n);
                        exit();
                    }
                }
            }
            exit();
        }
    }

    for _pi in 0..4 {
        wait();
    }

    for i in 0..2 {
        let fname = names[i];
        let fd = open(fname, 0);
        let mut total = 0;
        unsafe {
            loop {
                let n = read(fd, &mut BUF[..8192]);
                if n <= 0 {
                    break;
                }
                for j in 0..n as usize {
                    if BUF[j] != b'0' + i as u8 {
                        failexit(b"wrong char\0");
                    }
                }
                total += n;
            }
        }
        close(fd);
        if total != 12 * 500 {
            printf!(1, "wrong length {}\n", total);
            exit();
        }
        unlink(fname);
    }

    printf!(1, "fourfiles ok\n");
}

// four processes create and delete different files in same directory
fn createdelete() {
    printf!(1, "createdelete test\n");

    for pi in 0..4 {
        let pid = fork();
        if pid < 0 {
            failexit(b"fork\0");
        }

        if pid == 0 {
            unsafe {
                let mut name = [0u8; 32];
                name[0] = b'p' + pi as u8;
                name[2] = 0;
                for i in 0..20 {
                    name[1] = b'0' + i;
                    let fd = open(&name[..3], O_CREATE | O_RDWR);
                    if fd < 0 {
                        failexit(b"create\0");
                    }
                    close(fd);
                    if i > 0 && (i % 2) == 0 {
                        name[1] = b'0' + (i / 2);
                        if unlink(&name[..3]) < 0 {
                            failexit(b"unlink\0");
                        }
                    }
                }
            }
            exit();
        }
    }

    for _pi in 0..4 {
        wait();
    }

    unsafe {
        let mut name = [0u8; 32];
        for i in 0..20 {
            for pi in 0..4 {
                name[0] = b'p' + pi;
                name[1] = b'0' + i;
                let fd = open(&name[..2], 0);
                if (i == 0 || i >= 10) && fd < 0 {
                    printf!(1, "oops createdelete {} didn't exist\n", i);
                    exit();
                } else if (i >= 1 && i < 10) && fd >= 0 {
                    printf!(1, "oops createdelete {} did exist\n", i);
                    exit();
                }
                if fd >= 0 {
                    close(fd);
                }
            }
        }

        for i in 0..20 {
            for pi in 0..4 {
                name[0] = b'p' + i;
                name[1] = b'0' + i;
                unlink(&name[..2]);
            }
        }
    }

    printf!(1, "createdelete ok\n");
}

// can I unlink a file and still read it?
fn unlinkread() {
    printf!(1, "unlinkread test\n");
    let fd = open(b"unlinkread\0", O_CREATE | O_RDWR);
    if fd < 0 {
        failexit(b"create unlinkread\0");
    }
    write(fd, b"hello");
    close(fd);

    let fd = open(b"unlinkread\0", O_RDWR);
    if fd < 0 {
        failexit(b"open unlinkread\0");
    }
    if unlink(b"unlinkread\0") != 0 {
        failexit(b"unlink unlinkread\0");
    }

    let fd1 = open(b"unlinkread\0", O_CREATE | O_RDWR);
    write(fd1, b"yyy");
    close(fd1);

    unsafe {
        if read(fd, &mut BUF[..8192]) != 5 {
            failexit(b"unlinkread read failed\0");
        }
        if BUF[0] != b'h' {
            failexit(b"unlinkread wrong data\0");
        }
        if write(fd, &BUF[..10]) != 10 {
            failexit(b"unlinkread write\0");
        }
    }
    close(fd);
    unlink(b"unlinkread\0");
    printf!(1, "unlinkread ok\n");
}

fn linktest() {
    printf!(1, "linktest\n");

    unlink(b"lf1\0");
    unlink(b"lf2\0");

    let fd = open(b"lf1\0", O_CREATE | O_RDWR);
    if fd < 0 {
        failexit(b"create lf1\0");
    }
    if write(fd, b"hello") != 5 {
        failexit(b"write lf1\0");
    }
    close(fd);

    if link(b"lf1\0", b"lf2\0") < 0 {
        failexit(b"link lf1 lf2\0");
    }
    unlink(b"lf1\0");

    if open(b"lf1\0", 0) >= 0 {
        failexit(b"unlinked lf1 but it is still there!\0");
    }

    let fd = open(b"lf2\0", 0);
    if fd < 0 {
        failexit(b"open lf2\0");
    }
    unsafe {
        if read(fd, &mut BUF[..8192]) != 5 {
            failexit(b"read lf2\0");
        }
    }
    close(fd);

    if link(b"lf2\0", b"lf2\0") >= 0 {
        failexit(b"link lf2 lf2 succeeded! oops\0");
    }

    unlink(b"lf2\0");
    if link(b"lf2\0", b"lf1\0") >= 0 {
        failexit(b"link non-existant succeeded! oops\0");
    }

    if link(b".\0", b"lf1\0") >= 0 {
        failexit(b"link . lf1 succeeded! oops\0");
    }

    printf!(1, "linktest ok\n");
}

// test concurrent create/link/unlink of the same file
fn concreate() {
    printf!(1, "concreate test\n");
    unsafe {
        let mut file = [0u8; 3];
        file[0] = b'C';
        file[2] = 0;
        for i in 0..40 {
            file[1] = b'0' + i;
            unlink(&file[..3]);
            let pid = fork();
            if pid != 0 && (i % 3) == 1 {
                link(b"C0\0", &file[..3]);
            } else if pid == 0 && (i % 5) == 1 {
                link(b"C0\0", &file[..3]);
            } else {
                let fd = open(&file[..3], O_CREATE | O_RDWR);
                if fd < 0 {
                    printf!(1, "concreate create {} failed\n", i);
                    exit();
                }
                close(fd);
            }
            if pid == 0 {
                exit();
            } else {
                wait();
            }
        }

        let mut fa = [0u8; 40];
        memset(fa.as_mut_ptr(), 0, 40);
        let fd = open(b".\0", 0);
        let mut n = 0;
        let mut de = Dirent {
            inum: 0,
            name: [0; 14],
        };
        loop {
            if read_raw(fd, &mut de as *mut Dirent as *mut u8, core::mem::size_of::<Dirent>()) <= 0 {
                break;
            }
            if de.inum == 0 {
                continue;
            }
            if de.name[0] == b'C' && de.name[2] == 0 {
                let i = (de.name[1] - b'0') as usize;
                if i >= 40 {
                    printf!(1, "concreate weird file\n");
                    exit();
                }
                if fa[i] != 0 {
                    printf!(1, "concreate duplicate file\n");
                    exit();
                }
                fa[i] = 1;
                n += 1;
            }
        }
        close(fd);

        if n != 40 {
            failexit(b"concreate not enough files in directory listing\0");
        }

        for i in 0..40 {
            file[1] = b'0' + i;
            let pid = fork();
            if pid < 0 {
                failexit(b"fork\0");
            }
            if ((i % 3) == 0 && pid == 0) || ((i % 3) == 1 && pid != 0) {
                close(open(&file[..3], 0));
                close(open(&file[..3], 0));
                close(open(&file[..3], 0));
                close(open(&file[..3], 0));
            } else {
                unlink(&file[..3]);
                unlink(&file[..3]);
                unlink(&file[..3]);
                unlink(&file[..3]);
            }
            if pid == 0 {
                exit();
            } else {
                wait();
            }
        }
    }

    printf!(1, "concreate ok\n");
}

// another concurrent link/unlink/create test,
// to look for deadlocks.
fn linkunlink() {
    printf!(1, "linkunlink test\n");

    unlink(b"x\0");
    let pid = fork();
    if pid < 0 {
        failexit(b"fork\0");
    }

    let mut x: u32 = if pid != 0 { 1 } else { 97 };
    for _i in 0..100 {
        x = x.wrapping_mul(1103515245).wrapping_add(12345);
        if (x % 3) == 0 {
            close(open(b"x\0", O_RDWR | O_CREATE));
        } else if (x % 3) == 1 {
            link(b"cat\0", b"x\0");
        } else {
            unlink(b"x\0");
        }
    }

    if pid != 0 {
        wait();
    } else {
        exit();
    }

    printf!(1, "linkunlink ok\n");
}

// directory that uses indirect blocks
fn bigdir() {
    printf!(1, "bigdir test\n");
    unlink(b"bd\0");

    let fd = open(b"bd\0", O_CREATE);
    if fd < 0 {
        failexit(b"bigdir create\0");
    }
    close(fd);

    unsafe {
        let mut name = [0u8; 10];
        for i in 0..500 {
            name[0] = b'x';
            name[1] = b'0' + (i / 64) as u8;
            name[2] = b'0' + (i % 64) as u8;
            name[3] = 0;
            if link(b"bd\0", &name[..4]) != 0 {
                failexit(b"bigdir link\0");
            }
        }

        unlink(b"bd\0");
        for i in 0..500 {
            name[0] = b'x';
            name[1] = b'0' + (i / 64) as u8;
            name[2] = b'0' + (i % 64) as u8;
            name[3] = 0;
            if unlink(&name[..4]) != 0 {
                failexit(b"bigdir unlink failed\0");
            }
        }
    }

    printf!(1, "bigdir ok\n");
}

fn subdir() {
    printf!(1, "subdir test\n");

    unlink(b"ff\0");
    if mkdir(b"dd\0") != 0 {
        failexit(b"subdir mkdir dd\0");
    }

    let fd = open(b"dd/ff\0", O_CREATE | O_RDWR);
    if fd < 0 {
        failexit(b"create dd/ff\0");
    }
    write(fd, b"ff");
    close(fd);

    if unlink(b"dd\0") >= 0 {
        failexit(b"unlink dd (non-empty dir) succeeded!\0");
    }

    if mkdir(b"/dd/dd\0") != 0 {
        failexit(b"subdir mkdir dd/dd\0");
    }

    let fd = open(b"dd/dd/ff\0", O_CREATE | O_RDWR);
    if fd < 0 {
        failexit(b"create dd/dd/ff\0");
    }
    write(fd, b"FF");
    close(fd);

    let fd = open(b"dd/dd/../ff\0", 0);
    if fd < 0 {
        failexit(b"open dd/dd/../ff\0");
    }
    unsafe {
        let cc = read(fd, &mut BUF[..8192]);
        if cc != 2 || BUF[0] != b'f' {
            failexit(b"dd/dd/../ff wrong content\0");
        }
    }
    close(fd);

    if link(b"dd/dd/ff\0", b"dd/dd/ffff\0") != 0 {
        failexit(b"link dd/dd/ff dd/dd/ffff\0");
    }

    if unlink(b"dd/dd/ff\0") != 0 {
        failexit(b"unlink dd/dd/ff\0");
    }
    if open(b"dd/dd/ff\0", O_RDONLY) >= 0 {
        failexit(b"open (unlinked) dd/dd/ff succeeded\0");
    }

    if chdir(b"dd\0") != 0 {
        failexit(b"chdir dd\0");
    }
    if chdir(b"dd/../../dd\0") != 0 {
        failexit(b"chdir dd/../../dd\0");
    }
    if chdir(b"dd/../../../dd\0") != 0 {
        failexit(b"chdir dd/../../dd\0");
    }
    if chdir(b"./.\0") != 0 {
        failexit(b"chdir ./.\0");
    }

    let fd = open(b"dd/dd/ffff\0", 0);
    if fd < 0 {
        failexit(b"open dd/dd/ffff\0");
    }
    unsafe {
        if read(fd, &mut BUF[..8192]) != 2 {
            failexit(b"read dd/dd/ffff wrong len\0");
        }
    }
    close(fd);

    if open(b"dd/dd/ff\0", O_RDONLY) >= 0 {
        failexit(b"open (unlinked) dd/dd/ff succeeded\0");
    }

    if open(b"dd/ff/ff\0", O_CREATE | O_RDWR) >= 0 {
        failexit(b"create dd/ff/ff succeeded\0");
    }
    if open(b"dd/xx/ff\0", O_CREATE | O_RDWR) >= 0 {
        failexit(b"create dd/xx/ff succeeded\0");
    }
    if open(b"dd\0", O_CREATE) >= 0 {
        failexit(b"create dd succeeded\0");
    }
    if open(b"dd\0", O_RDWR) >= 0 {
        failexit(b"open dd rdwr succeeded\0");
    }
    if open(b"dd\0", O_WRONLY) >= 0 {
        failexit(b"open dd wronly succeeded\0");
    }
    if link(b"dd/ff/ff\0", b"dd/dd/xx\0") == 0 {
        failexit(b"link dd/ff/ff dd/dd/xx succeeded\0");
    }
    if link(b"dd/xx/ff\0", b"dd/dd/xx\0") == 0 {
        failexit(b"link dd/xx/ff dd/dd/xx succeededn\0");
    }
    if link(b"dd/ff\0", b"dd/dd/ffff\0") == 0 {
        failexit(b"link dd/ff dd/dd/ffff succeeded\0");
    }
    if mkdir(b"dd/ff/ff\0") == 0 {
        failexit(b"mkdir dd/ff/ff succeeded\0");
    }
    if mkdir(b"dd/xx/ff\0") == 0 {
        failexit(b"mkdir dd/xx/ff succeeded\0");
    }
    if mkdir(b"dd/dd/ffff\0") == 0 {
        failexit(b"mkdir dd/dd/ffff succeeded\0");
    }
    if unlink(b"dd/xx/ff\0") == 0 {
        failexit(b"unlink dd/xx/ff succeeded\0");
    }
    if unlink(b"dd/ff/ff\0") == 0 {
        failexit(b"unlink dd/ff/ff succeeded\0");
    }
    if chdir(b"dd/ff\0") == 0 {
        failexit(b"chdir dd/ff succeeded\0");
    }
    if chdir(b"dd/xx\0") == 0 {
        failexit(b"chdir dd/xx succeeded\0");
    }

    if unlink(b"dd/dd/ffff\0") != 0 {
        failexit(b"unlink dd/dd/ff\0");
    }
    if unlink(b"dd/ff\0") != 0 {
        failexit(b"unlink dd/ff\0");
    }
    if unlink(b"dd\0") == 0 {
        failexit(b"unlink non-empty dd succeeded\0");
    }
    if unlink(b"dd/dd\0") < 0 {
        failexit(b"unlink dd/dd\0");
    }
    if unlink(b"dd\0") < 0 {
        failexit(b"unlink dd\0");
    }

    printf!(1, "subdir ok\n");
}

// test writes that are larger than the log.
fn bigwrite() {
    printf!(1, "bigwrite test\n");

    unlink(b"bigwrite\0");
    let mut sz = 499;
    while sz < 12 * 512 {
        let fd = open(b"bigwrite\0", O_CREATE | O_RDWR);
        if fd < 0 {
            failexit(b"cannot create bigwrite\0");
        }
        unsafe {
            for _i in 0..2 {
                let cc = write(fd, &BUF[..sz]);
                if cc != sz as i32 {
                    printf!(1, "write({}) ret {}\n", sz, cc);
                    exit();
                }
            }
        }
        close(fd);
        unlink(b"bigwrite\0");
        sz += 471;
    }

    printf!(1, "bigwrite ok\n");
}

fn bigfile() {
    printf!(1, "bigfile test\n");

    unlink(b"bigfile\0");
    let fd = open(b"bigfile\0", O_CREATE | O_RDWR);
    if fd < 0 {
        failexit(b"cannot create bigfile\0");
    }
    unsafe {
        for i in 0..20 {
            memset(BUF.as_mut_ptr(), i as u8, 600);
            if write(fd, &BUF[..600]) != 600 {
                failexit(b"write bigfile\0");
            }
        }
    }
    close(fd);

    let fd = open(b"bigfile\0", 0);
    if fd < 0 {
        failexit(b"cannot open bigfile\0");
    }
    let mut total = 0;
    unsafe {
        for i in 0.. {
            let cc = read(fd, &mut BUF[..300]);
            if cc < 0 {
                failexit(b"read bigfile\0");
            }
            if cc == 0 {
                break;
            }
            if cc != 300 {
                failexit(b"short read bigfile\0");
            }
            if BUF[0] != (i / 2) as u8 || BUF[299] != (i / 2) as u8 {
                failexit(b"read bigfile wrong data\0");
            }
            total += cc;
        }
    }
    close(fd);
    if total != 20 * 600 {
        failexit(b"read bigfile wrong total\0");
    }
    unlink(b"bigfile\0");

    printf!(1, "bigfile test ok\n");
}

fn fourteen() {
    // DIRSIZ is 14.
    printf!(1, "fourteen test\n");

    if mkdir(b"12345678901234\0") != 0 {
        failexit(b"mkdir 12345678901234\0");
    }
    if mkdir(b"12345678901234/123456789012345\0") != 0 {
        failexit(b"mkdir 12345678901234/123456789012345\0");
    }
    let fd = open(b"123456789012345/123456789012345/123456789012345\0", O_CREATE);
    if fd < 0 {
        failexit(b"create 123456789012345/123456789012345/123456789012345\0");
    }
    close(fd);
    let fd = open(b"12345678901234/12345678901234/12345678901234\0", 0);
    if fd < 0 {
        failexit(b"open 12345678901234/12345678901234/12345678901234\0");
    }
    close(fd);

    if mkdir(b"12345678901234/12345678901234\0") == 0 {
        failexit(b"mkdir 12345678901234/12345678901234 succeeded\0");
    }
    if mkdir(b"123456789012345/12345678901234\0") == 0 {
        failexit(b"mkdir 12345678901234/123456789012345 succeeded\0");
    }

    printf!(1, "fourteen ok\n");
}

fn rmdot() {
    printf!(1, "rmdot test\n");
    if mkdir(b"dots\0") != 0 {
        failexit(b"mkdir dots\0");
    }
    if chdir(b"dots\0") != 0 {
        failexit(b"chdir dots\0");
    }
    if unlink(b".\0") == 0 {
        failexit(b"rm . worked\0");
    }
    if unlink(b"..\0") == 0 {
        failexit(b"rm .. worked\0");
    }
    if chdir(b"/\0") != 0 {
        failexit(b"chdir /\0");
    }
    if unlink(b"dots/.\0") == 0 {
        failexit(b"unlink dots/. worked\0");
    }
    if unlink(b"dots/..\0") == 0 {
        failexit(b"unlink dots/.. worked\0");
    }
    if unlink(b"dots\0") != 0 {
        failexit(b"unlink dots\0");
    }
    printf!(1, "rmdot ok\n");
}

fn dirfile() {
    printf!(1, "dir vs file\n");

    let fd = open(b"dirfile\0", O_CREATE);
    if fd < 0 {
        failexit(b"create dirfile\0");
    }
    close(fd);
    if chdir(b"dirfile\0") == 0 {
        failexit(b"chdir dirfile succeeded\0");
    }
    let fd = open(b"dirfile/xx\0", 0);
    if fd >= 0 {
        failexit(b"create dirfile/xx succeeded\0");
    }
    let fd = open(b"dirfile/xx\0", O_CREATE);
    if fd >= 0 {
        failexit(b"create dirfile/xx succeeded\0");
    }
    if mkdir(b"dirfile/xx\0") == 0 {
        failexit(b"mkdir dirfile/xx succeeded\0");
    }
    if unlink(b"dirfile/xx\0") == 0 {
        failexit(b"unlink dirfile/xx succeeded\0");
    }
    if link(b"README\0", b"dirfile/xx\0") == 0 {
        failexit(b"link to dirfile/xx succeeded\0");
    }
    if unlink(b"dirfile\0") != 0 {
        failexit(b"unlink dirfile\0");
    }

    let fd = open(b".\0", O_RDWR);
    if fd >= 0 {
        failexit(b"open . for writing succeeded\0");
    }
    let fd = open(b".\0", 0);
    if write(fd, b"x") > 0 {
        failexit(b"write . succeeded\0");
    }
    close(fd);

    printf!(1, "dir vs file OK\n");
}

// test that iput() is called at the end of _namei()
fn iref() {
    printf!(1, "empty file name\n");

    // the 50 is NINODE
    for i in 0..51 {
        if mkdir(b"irefd\0") != 0 {
            failexit(b"mkdir irefd\0");
        }
        if chdir(b"irefd\0") != 0 {
            failexit(b"chdir irefd\0");
        }

        mkdir(b"\0");
        link(b"README\0", b"\0");
        let fd = open(b"\0", O_CREATE);
        if fd >= 0 {
            close(fd);
        }
        let fd = open(b"xx\0", O_CREATE);
        if fd >= 0 {
            close(fd);
        }
        unlink(b"xx\0");
    }

    chdir(b"/\0");
    printf!(1, "empty file name OK\n");
}

// test that fork fails gracefully
fn forktest() {
    printf!(1, "fork test\n");

    let mut n = 0;
    for _ in 0..1000 {
        let pid = fork();
        if pid < 0 {
            break;
        }
        if pid == 0 {
            exit();
        }
        n += 1;
    }

    if n == 1000 {
        failexit(b"fork claimed to work 1000 times\0");
    }

    for _i in 0..n {
        if wait() < 0 {
            failexit(b"wait stopped early\0");
        }
    }

    if wait() != -1 {
        failexit(b"wait got too many\0");
    }

    printf!(1, "fork test OK\n");
}

fn sbrktest() {
    printf!(1, "sbrk test\n");
    let oldbrk = sbrk(0);

    // can one sbrk() less than a page?
    let a = sbrk(0);
    let mut curr_a = a;
    for i in 0..5000 {
        let b = sbrk(1);
        if b != curr_a {
            printf!(1, "sbrk test failed {} {:p} {:p}\n", i, a, b);
            exit();
        }
        unsafe {
            *b = 1;
        }
        curr_a = unsafe { b.add(1) };
    }
    let pid = fork();
    if pid < 0 {
        failexit(b"sbrk test fork\0");
    }
    let c = sbrk(1);
    let c = sbrk(1);
    unsafe {
        if c != curr_a.add(1) {
            failexit(b"sbrk test failed post-fork\0");
        }
    }
    if pid == 0 {
        exit();
    }
    wait();

    // can one grow address space to something big?
    let a = sbrk(0);
    let amt = (BIG as isize - a as isize) as i32;
    let p = sbrk(amt);
    if p != a {
        failexit(b"sbrk test failed to grow big address space; enough phys mem?\0");
    }
    let lastaddr = unsafe { (BIG as *mut u8).sub(1) };
    unsafe {
        *lastaddr = 99;
    }

    // can one de-allocate?
    let a = sbrk(0);
    let c = sbrk(-4096);
    if c as usize == 0xffffffff {
        failexit(b"sbrk could not deallocate\0");
    }
    let c = sbrk(0);
    unsafe {
        if c != a.sub(4096) {
            printf!(1, "sbrk deallocation produced wrong address, a {:p} c {:p}\n", a, c);
            exit();
        }
    }

    // can one re-allocate that page?
    let a = sbrk(0);
    let c = sbrk(4096);
    if c != a || sbrk(0) != unsafe { a.add(4096) } {
        printf!(1, "sbrk re-allocation failed, a {:p} c {:p}\n", a, c);
        exit();
    }
    unsafe {
        if *lastaddr == 99 {
            // should be zero
            failexit(b"sbrk de-allocation didn't really deallocate\0");
        }
    }

    let a = sbrk(0);
    let c = sbrk(-((a as usize - oldbrk as usize) as i32));
    if c != a {
        printf!(1, "sbrk downsize failed, a {:p} c {:p}\n", a, c);
        exit();
    }

    printf!(1, "expecting 10 killed processes:\n");
    // can we read the kernel's memory?
    let mut a = KERNBASE as *mut u8;
    while (a as usize) < KERNBASE + 1000000 {
        let ppid = getpid();
        let pid = fork();
        if pid < 0 {
            failexit(b"fork\0");
        }
        if pid == 0 {
            unsafe {
                printf!(1, "oops could read {:p} = {}\n", a, *a);
            }
            kill(ppid);
            exit();
        }
        wait();
        a = unsafe { a.add(100000) };
    }

    // if we run the system out of memory, does it clean up the last
    // failed allocation?
    let mut fds = [0i32; 2];
    if pipe(&mut fds) != 0 {
        failexit(b"pipe()\0");
    }
    printf!(1, "expecting failed sbrk()s:\n");
    let mut pids = [0i32; 10];
    for i in 0..10 {
        pids[i] = fork();
        if pids[i] == 0 {
            // allocate a lot of memory
            let ret = sbrk((BIG as isize - sbrk(0) as isize) as i32);
            if ret as usize == 0xffffffffffffffff || ret.is_null() {
                printf!(1, "sbrk returned -1 as expected\n");
            }
            write(fds[1], b"x");
            // sit around until killed
            loop {
                sleep(1000);
            }
        }
        if pids[i] != -1 {
            unsafe {
                read(fds[0], &mut BUF[..1]);
            }
        }
    }

    // if those failed allocations freed up the pages they did allocate,
    // we'll be able to allocate one here
    let c = sbrk(4096);
    for i in 0..10 {
        if pids[i] == -1 {
            continue;
        }
        kill(pids[i]);
        wait();
    }
    if c as usize == 0xffffffffffffffff {
        failexit(b"failed sbrk leaked memory\0");
    }

    if sbrk(0) > oldbrk {
        let curr = sbrk(0);
        sbrk(-(((curr as usize).wrapping_sub(oldbrk as usize)) as isize) as i32);
    }

    printf!(1, "sbrk test OK\n");
}

fn validatetest() {
    printf!(1, "validate test\n");
    let hi: usize = 1100 * 1024;

    let mut p: usize = 4096;
    while p <= hi {
        // try to crash the kernel by passing in a bad string pointer
        if link(b"nosuchfile\0", unsafe { core::slice::from_raw_parts(p as *const u8, 1) }) != -1 {
            failexit(b"link should not succeed\0");
        }
        p += 4096;
    }

    printf!(1, "validate ok\n");
}

// does uninitialized data start out zero?
static mut UNINIT: [u8; 10000] = [0; 10000];

fn bsstest() {
    printf!(1, "bss test\n");
    unsafe {
        for i in 0..10000 {
            if UNINIT[i] != 0 {
                failexit(b"bss test\0");
            }
        }
    }
    printf!(1, "bss test ok\n");
}

// does exec return an error if the arguments
// are larger than a page?
fn bigargtest() {
    unlink(b"bigarg-ok\0");
    let pid = fork();
    if pid == 0 {
        let longstr = b"bigargs test: failed\n                                                                                                                                                                                                       \0";
        let mut args: [*const u8; 33] = [core::ptr::null(); 33];
        for i in 0..31 {
            args[i] = longstr.as_ptr();
        }
        args[31] = core::ptr::null();
        printf!(1, "bigarg test\n");
        exec(b"echo\0", &args[..32]);
        printf!(1, "bigarg test ok\n");
        let fd = open(b"bigarg-ok\0", O_CREATE);
        close(fd);
        exit();
    } else if pid < 0 {
        failexit(b"bigargtest: fork\0");
    }
    wait();
    let fd = open(b"bigarg-ok\0", 0);
    if fd < 0 {
        failexit(b"bigarg test failed!\0");
    }
    close(fd);
    unlink(b"bigarg-ok\0");
}

// what happens when the file system runs out of blocks?
fn fsfull() {
    printf!(1, "fsfull test\n");

    let mut nfiles: i32 = 0;
    let mut done = false;
    while !done {
        let mut name = [0u8; 64];
        name[0] = b'f';
        name[1] = b'0' + (nfiles / 1000) as u8;
        name[2] = b'0' + ((nfiles % 1000) / 100) as u8;
        name[3] = b'0' + ((nfiles % 100) / 10) as u8;
        name[4] = b'0' + (nfiles % 10) as u8;
        name[5] = 0;
        printf!(1, "writing {}\n", nfiles);
        let fd = open(&name[..6], O_CREATE | O_RDWR);
        if fd < 0 {
            printf!(1, "open {} failed\n", nfiles);
            break;
        }
        let mut total = 0;
        loop {
            unsafe {
                let cc = write(fd, &BUF[..512]);
                if cc < 512 {
                    done = total == 0;
                    break;
                }
                total += cc;
            }
        }
        printf!(1, "wrote {} bytes\n", total);
        close(fd);
        if done {
            break;
        }
        nfiles += 1;
    }

    while nfiles >= 0 {
        let mut name = [0u8; 64];
        name[0] = b'f';
        name[1] = b'0' + (nfiles / 1000) as u8;
        name[2] = b'0' + ((nfiles % 1000) / 100) as u8;
        name[3] = b'0' + ((nfiles % 100) / 10) as u8;
        name[4] = b'0' + (nfiles % 10) as u8;
        name[5] = 0;
        unlink(&name[..6]);
        nfiles -= 1;
    }

    printf!(1, "fsfull test finished\n");
}

fn uio() {
    printf!(1, "uio test\n");
    let pid = fork();
    if pid == 0 {
        // Attempt port I/O - should be killed by kernel
        unsafe {
            core::arch::asm!("out dx, al", in("dx") 0x70u16, in("al") 0x09u8);
        }
        let _val: u8;
        unsafe {
            core::arch::asm!("in al, dx", out("al") _val, in("dx") 0x71u16);
        }
        printf!(1, "uio: port I/O succeeded (unexpected)\n");
        exit();
    } else if pid < 0 {
        failexit(b"fork\0");
    }
    wait();
    printf!(1, "uio test done\n");
}

fn argptest() {
    let fd = open(b"init\0", O_RDONLY);
    if fd < 0 {
        failexit(b"open\0");
    }
    unsafe {
        let p = sbrk(0).sub(1);
        read_raw(fd, p, 0xffffffff);
    }
    close(fd);
    printf!(1, "arg test passed\n");
}

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> ! {
    printf!(1, "usertests starting\n");

    if open(b"usertests.ran\0", 0) >= 0 {
        failexit(b"already ran user tests -- rebuild fs.img\0");
    }
    close(open(b"usertests.ran\0", O_CREATE));

    argptest();
    createdelete();
    linkunlink();
    concreate();
    fourfiles();
    sharedfd();

    bigargtest();
    bigwrite();
    bigargtest();
    bsstest();
    sbrktest();
    validatetest();

    opentest();
    writetest();
    writetest1();
    createtest();

    openiputtest();
    exitiputtest();
    iputtest();

    mem();
    pipe1();
    preempt();
    exitwait();
    nullptrtest();

    rmdot();
    fourteen();
    bigfile();
    subdir();
    linktest();
    unlinkread();
    dirfile();
    iref();
    forktest();
    bigdir();
    uio();

    exectest();

    exit();
}