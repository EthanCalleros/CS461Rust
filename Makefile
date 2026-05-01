# =============================================================================
# xv6-Rust Makefile
#
# Builds the kernel (via Cargo + cross-assembler), user programs, bootblock,
# filesystem image, and disk image. Run under Linux, WSL, or macOS with
# an x86_64 cross-toolchain installed.
#
# Requirements:
#   - Rust nightly with x86_64-unknown-none target:
#       rustup target add x86_64-unknown-none
#   - GNU cross-toolchain (gcc, ld, objcopy, objdump, ar) for x86_64-elf
#   - QEMU: qemu-system-x86_64
#   - Perl (for sign.pl if using legacy boot)
# =============================================================================

# --- Toolchain detection ---
UNAME_S := $(shell uname -s)
UNAME_P := $(shell uname -p)
ifeq ($(UNAME_S),Linux)
  ifeq ($(UNAME_P),aarch64)
    TOOLPREFIX = x86_64-linux-gnu-
  else
    TOOLPREFIX =
  endif
else ifeq ($(UNAME_S),Darwin)
  TOOLPREFIX = x86_64-elf-
endif

CC = $(TOOLPREFIX)gcc
AS = $(TOOLPREFIX)as
LD = $(TOOLPREFIX)ld
OBJCOPY = $(TOOLPREFIX)objcopy
OBJDUMP = $(TOOLPREFIX)objdump
AR = $(TOOLPREFIX)ar
QEMU = qemu-system-x86_64

# --- Build flags ---
XFLAGS = -m64 -DX64 -mcmodel=large -mtls-direct-seg-refs -mno-red-zone
CFLAGS = -fno-pic -static -fno-builtin -fno-strict-aliasing -Wall -MD -ggdb \
         -fno-omit-frame-pointer -ffreestanding -fno-common -nostdlib \
         $(XFLAGS) -O0
CFLAGS += $(shell $(CC) -fno-stack-protector -E -x c /dev/null >/dev/null 2>&1 && echo -fno-stack-protector)
ASFLAGS = -gdwarf-2 -Wa,-divide $(XFLAGS)
LDFLAGS = -m elf_x86_64 -nostdlib

# --- CPU count for QEMU ---
ifndef CPUS
CPUS := 2
endif

# --- Directories ---
CARGO_TARGET = target/x86_64-unknown-none/debug
CARGO_TARGET_REL = target/x86_64-unknown-none/release
ASM_DIR = crates/arch/src/asm
BUILD_DIR = build

# =============================================================================
# Top-level targets
# =============================================================================

.PHONY: all clean qemu qemu-nox qemu-gdb kernel-build user-build

all: xv6.img fs.img

# =============================================================================
# Kernel build (Cargo + assembly blobs)
# =============================================================================

# Standalone assembly blobs that get linked into the kernel as binary data.
# These are flat binaries (no ELF headers) loaded at specific physical addrs.

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

$(BUILD_DIR)/initcode: $(ASM_DIR)/initcode.S | $(BUILD_DIR)
	$(CC) $(CFLAGS) -nostdinc -I. -c -o $(BUILD_DIR)/initcode.o $<
	$(LD) $(LDFLAGS) -N -e start -Ttext 0x1000 -o $(BUILD_DIR)/initcodetmp.o $(BUILD_DIR)/initcode.o
	$(OBJCOPY) -S -O binary -j .text $(BUILD_DIR)/initcodetmp.o $@
	$(OBJDUMP) -S $(BUILD_DIR)/initcodetmp.o > $(BUILD_DIR)/initcode.asm

$(BUILD_DIR)/entryother: $(ASM_DIR)/entryother.S | $(BUILD_DIR)
	$(CC) $(CFLAGS) -nostdinc -I. -c -o $(BUILD_DIR)/entryother.o $<
	$(LD) $(LDFLAGS) -N -e start -Ttext 0x7000 -o $(BUILD_DIR)/entryothertmp.o $(BUILD_DIR)/entryother.o
	$(OBJCOPY) -S -O binary -j .text $(BUILD_DIR)/entryothertmp.o $@
	$(OBJDUMP) -S $(BUILD_DIR)/entryothertmp.o > $(BUILD_DIR)/entryother.asm

# Build the kernel ELF via Cargo.
# The arch crate's build.rs assembles initcode.S and entryother.S into
# binary blobs and links them in automatically (providing _binary_* symbols).
# Cargo uses rust-lld with kernel.ld (set in kernel/build.rs).
kernel-build:
	cargo build -p kernel

# The final kernel binary. Named "kernel.elf" to avoid collision with
# the kernel/ source directory.
kernel: kernel-build | $(BUILD_DIR)
	cp $(CARGO_TARGET)/kernel $(BUILD_DIR)/kernel.elf
	$(OBJDUMP) -S $(BUILD_DIR)/kernel.elf > $(BUILD_DIR)/kernel.asm 2>/dev/null || true
	$(OBJDUMP) -t $(BUILD_DIR)/kernel.elf | sed '1,/SYMBOL TABLE/d; s/ .* / /; /^$$/d' | sort > $(BUILD_DIR)/kernel.sym 2>/dev/null || true

# =============================================================================
# Bootblock (16-bit real mode → 32-bit protected mode → jumps to kernel)
# =============================================================================

bootblock: $(ASM_DIR)/bootasm.S boot/bootmain.c | $(BUILD_DIR)
	$(CC) -fno-builtin -fno-pic -m32 -nostdinc -I. -c -o $(BUILD_DIR)/bootasm.o $(ASM_DIR)/bootasm.S
	$(CC) -fno-builtin -fno-pic -m32 -nostdinc -I. -O -c -o $(BUILD_DIR)/bootmain.o boot/bootmain.c
	$(LD) -m elf_i386 -nostdlib -N -e start -Ttext 0x7C00 \
		-o $(BUILD_DIR)/bootblocktmp.o $(BUILD_DIR)/bootasm.o $(BUILD_DIR)/bootmain.o
	$(OBJDUMP) -S $(BUILD_DIR)/bootblocktmp.o > $(BUILD_DIR)/bootblock.asm
	$(OBJCOPY) -S -O binary -j .text $(BUILD_DIR)/bootblocktmp.o $(BUILD_DIR)/bootblock
	perl sign.pl $(BUILD_DIR)/bootblock

# =============================================================================
# User programs
# =============================================================================

# List of user programs (Rust binaries in the user/ crate)
UPROGS = \
	_cat _echo _forktest _grep _init _kill _ln _ls _mkdir \
	_rm _sh _stressfs _usertests _wc _zombie

# Build all user programs via Cargo in release mode (debug builds are
# too large for xv6's small filesystem).
user-build:
	cargo build -p user --release

# Extract individual user ELFs into the format mkfs expects
$(UPROGS): user-build
	@for prog in $(UPROGS); do \
		name=$${prog#_}; \
		cp $(CARGO_TARGET_REL)/$$name $$prog 2>/dev/null || true; \
	done

# =============================================================================
# Filesystem image
# =============================================================================

MKFS_TARGET = x86_64-unknown-linux-gnu

fs.img: mkfs_bin $(UPROGS) README.md
	./mkfs_bin fs.img README.md $(UPROGS)

mkfs_bin: mkfs/src/main.rs mkfs/Cargo.toml
	cd mkfs && cargo build --release
	cp mkfs/target/$(MKFS_TARGET)/release/mkfs ./mkfs_bin

# =============================================================================
# Disk image (bootblock + kernel)
# =============================================================================

xv6.img: bootblock kernel fs.img
	dd if=/dev/zero of=xv6.img count=10000
	dd if=$(BUILD_DIR)/bootblock of=xv6.img conv=notrunc
	dd if=$(BUILD_DIR)/kernel.elf of=xv6.img seek=1 conv=notrunc

# =============================================================================
# QEMU targets
# =============================================================================

GDBPORT = $(shell expr `id -u` % 5000 + 25000)
QEMUGDB = $(shell if $(QEMU) -help | grep -q '^-gdb'; \
	then echo "-gdb tcp::$(GDBPORT)"; \
	else echo "-s -p $(GDBPORT)"; fi)

QEMUOPTS = -cpu qemu64,+rdtscp -nic none -hda xv6.img -hdb fs.img \
            -smp sockets=$(CPUS) -m 512 $(QEMUEXTRA)

qemu: xv6.img fs.img
	$(QEMU) -serial mon:stdio $(QEMUOPTS)

qemu-nox: xv6.img fs.img
	$(QEMU) -nographic $(QEMUOPTS)

# Debug target: logs interrupts/exceptions, stops on triple fault instead
# of rebooting. Very useful for diagnosing early boot crashes.
qemu-debug: xv6.img fs.img
	$(QEMU) -nographic $(QEMUOPTS) -no-reboot -d int,cpu_reset -D qemu-debug.log
	@echo "Debug log written to qemu-debug.log"

qemu-gdb: xv6.img fs.img
	@echo "*** Now run 'gdb'." 1>&2
	$(QEMU) -serial mon:stdio $(QEMUOPTS) -S $(QEMUGDB)

qemu-nox-gdb: xv6.img fs.img
	@echo "*** Now run 'gdb'." 1>&2
	$(QEMU) -nographic $(QEMUOPTS) -S $(QEMUGDB)

# =============================================================================
# Utilities
# =============================================================================

# sign.pl — creates the 510-byte + 0x55AA boot signature
sign.pl:
	@echo '#!/usr/bin/env perl' > sign.pl
	@echo 'open(SIG, $$ARGV[0]) || die "open $$ARGV[0]: $$!";' >> sign.pl
	@echo 'my $$buf; read(SIG, $$buf, 1000); close(SIG);' >> sign.pl
	@echo 'if(length($$buf) > 510) { print STDERR "boot block too large: " . length($$buf) . " bytes (max 510)\n"; exit 1; }' >> sign.pl
	@echo '$$buf .= "\0" x (510 - length($$buf));' >> sign.pl
	@echo '$$buf .= "\x55\xAA";' >> sign.pl
	@echo 'open(SIG, ">$$ARGV[0]") || die "open >$$ARGV[0]: $$!";' >> sign.pl
	@echo 'print SIG $$buf; close(SIG);' >> sign.pl
	@chmod +x sign.pl

clean:
	rm -rf $(BUILD_DIR) xv6.img fs.img mkfs_bin
	rm -f $(UPROGS) *.o *.d *.asm *.sym
	cargo clean

.PHONY: all clean qemu qemu-nox qemu-gdb qemu-nox-gdb kernel-build user-build
