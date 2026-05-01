/* entry.S
 *
 * Copyright (c) 2013 Brian Swetland
 *
 * Permission is hereby granted, free of charge, to any person obtaining
 * a copy of this software and associated documentation files (the
 * "Software"), to deal in the Software without restriction, including
 * without limitation the rights to use, copy, modify, merge, publish,
 * distribute, sublicense, and/or sell copies of the Software, and to
 * permit persons to whom the Software is furnished to do so, subject to
 * the following conditions:
 *
 * The above copyright notice and this permission notice shall be
 * included in all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
 * MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
 * NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE
 * LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
 * OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
 * WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 *
 */

/* Constants (inlined from mmu.h — no C preprocessor available via global_asm!) */
.set mboot_magic, 0x1badb002
.set mboot_flags, 0x00010000

.set PTE_P,    0x001
.set PTE_W,    0x002
.set PTE_PS,   0x080
.set MSR_EFER, 0xC0000080
.set CR0_PG,   0x80000000
.set CR0_WP,   0x00010000
.set CR0_MP,   0x00000002

.code64
.global mboot_header
.global mboot_entry

mboot_header:
  .long mboot_magic
  .long mboot_flags
  .long (-mboot_magic -mboot_flags)     # checksum
  .long mboot_load_addr                 # header_addr
  .long mboot_load_addr
  .long mboot_load_end
  .long mboot_bss_end
  .long mboot_entry_addr

.code32
mboot_entry:
# zero 2 pages for our bootstrap page tables
  xor     %eax, %eax    # value=0
  mov     $0x1000, %edi # starting at 4096
  mov     $0x2000, %ecx # size=8192
  rep     stosb         # memset(4096, 0, 8192)

# map both virtual address 0 and KERNBASE to the same PDPT
# note: 32-bit operations manipulating 64-bit page table
# IMPORTANT: Use register-indirect addressing (mov %eax, (%edi))
# instead of absolute addressing (mov %eax, 0x1000), because
# LLVM encodes absolute-address MOV with 8-byte addresses in a
# 64-bit ELF even under .code32, which corrupts the instruction
# stream when running in real 32-bit mode.
#
# PML4T[0] -> 0x2000 (PDPT)
# PML4T[256] -> 0x2000 (PDPT)
  mov     $(0x2000 | PTE_P | PTE_W), %eax
  mov     $0x1000, %edi
  mov     %eax, (%edi)  # PML4T[0]
  mov     $0x1800, %edi
  mov     %eax, (%edi)  # PML4T[256]

# PDPT[0] -> 0x0 (1 GB flat map page)
  mov     $(0x0 | PTE_P | PTE_PS | PTE_W), %eax
  mov     $0x2000, %edi
  mov     %eax, (%edi)  # PDPT[0]

# Clear ebx for initial processor boot.
# When secondary processors boot, they'll call through
# entry32mp (from entryother), but with a nonzero ebx.
# We'll reuse these bootstrap pagetables and GDT.
  xor     %ebx, %ebx

.global entry32mp
entry32mp:
# CR3 -> 0x1000 (PML4T)
  mov     $0x1000, %eax
  mov     %eax, %cr3

# Use register-indirect lgdt to avoid RIP-relative encoding
  mov     $(gdtr64 - mboot_header + mboot_load_addr), %eax
  lgdt    (%eax)

# PAE is required for 64-bit paging: CR4.PAE=1
  mov     %cr4, %eax
  bts     $5, %eax
  mov     %eax, %cr4

# access EFER Model specific register
  mov     $MSR_EFER, %ecx
  rdmsr
  bts     $0, %eax #enable system call extensions
  bts     $8, %eax #enable long mode
  wrmsr

# enable paging
  mov     %cr0, %eax
  orl     $(CR0_PG | CR0_WP | CR0_MP), %eax
  mov     %eax, %cr0

# shift to 64bit segment
  ljmp    $8, $(entry64low - mboot_header + mboot_load_addr)

.align 16
gdtr64:
  .word gdt64_end - gdt64_begin - 1;
  .quad gdt64_begin - mboot_header + mboot_load_addr

.align 16
gdt64_begin:
  .long 0x00000000 # 0: null desc
  .long 0x00000000
  .long 0x00000000 # 1: Code, R/X, Nonconforming
  .long 0x00209800
  .long 0x00000000 # 2: Data, R/W, Expand Down
  .long 0x00009000
gdt64_end:

.align 16
.code64
entry64low:
  movabs  $entry64high, %rax
  jmp     *%rax

.global _start
_start:
entry64high:

# ensure data segment registers are sane
  xor     %rax, %rax
  mov     %ax, %ss
  mov     %ax, %ds
  mov     %ax, %es
  mov     %ax, %fs
  mov     %ax, %gs

  # this would enable floating point instructions
  # mov     %cr4, %rax
  # or      $(CR4_PAE | CR4_OSXFSR | CR4_OSXMMEXCPT) , %rax
  # mov     %rax, %cr4

# check to see if we're booting a secondary core
  test    %ebx, %ebx
  jnz     entry64mp  # jump if booting a secondary code
# setup initial stack
  movabs  $0xFFFF800000010000, %rax
  mov     %rax, %rsp

# enter main()
  jmp     main  # end of initial (the first) core ASM

.global __deadloop
__deadloop:
# we should never return here...
  jmp     .

entry64mp:
# obtain kstack from data block before entryother
  mov     $0x7000, %rax
  mov     -16(%rax), %rsp
  jmp     mpenter  # end of secondary code ASM

.global wrmsr
wrmsr:
  mov     %rdi, %rcx     # arg0 -> msrnum
  mov     %rsi, %rax     # val.low -> eax
  shr     $32, %rsi
  mov     %rsi, %rdx     # val.high -> edx
  wrmsr
  retq

.global ignore_sysret
ignore_sysret: #return error code 38, meaning function unimplemented
  mov     $-38, %rax
  sysretq