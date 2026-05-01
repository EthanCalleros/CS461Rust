// Boot loader.
//
// Part of the boot block, along with bootasm.S, which calls bootmain().
// bootasm.S has put the processor into protected mode and set up a
// rudimentary stack; this C code reads the kernel from disk using
// Multiboot conventions and jumps to its entry point.

#include "../types_boot.h"

#define SECTSIZE 512

void readseg(uchar*, uint, uint);

// Multiboot header structure
struct mbheader {
  uint magic;
  uint flags;
  uint checksum;
  uint header_addr;
  uint load_addr;
  uint load_end_addr;
  uint bss_end_addr;
  uint entry_addr;
};

void
bootmain(void)
{
  struct mbheader *hdr;
  uchar *scratch = (uchar*)0x10000;
  uint n;

  // Read first 8KB of kernel to find Multiboot header
  readseg(scratch, 8192, 0);

  for (n = 0; n < 8192; n += 4) {
    uint *p = (uint*)(scratch + n);
    if (*p == 0x1BADB002) {
      // Verify checksum
      if (p[0] + p[1] + p[2] == 0) {
        hdr = (struct mbheader*)p;
        if (hdr->flags & 0x10000) {
          // Load kernel
          readseg((uchar*)hdr->load_addr,
                  hdr->load_end_addr - hdr->load_addr,
                  n - (hdr->header_addr - hdr->load_addr));

          // Zero BSS
          if (hdr->bss_end_addr > hdr->load_end_addr) {
            stosb((void*)hdr->load_end_addr, 0,
                  hdr->bss_end_addr - hdr->load_end_addr);
          }

          // Jump to kernel entry point
          ((void(*)(void))(hdr->entry_addr))();
        }
      }
    }
  }

  // Kernel not found — hang
  for(;;)
    ;
}

void
waitdisk(void)
{
  // Wait for disk ready (bit 6 set, bit 7 clear)
  while((inb(0x1F7) & 0xC0) != 0x40)
    ;
}

// Read a single sector at offset into dst.
void
readsect(void *dst, uint offset)
{
  waitdisk();
  outb(0x1F2, 1);   // count = 1
  outb(0x1F3, offset);
  outb(0x1F4, offset >> 8);
  outb(0x1F5, offset >> 16);
  outb(0x1F6, (offset >> 24) | 0xE0);
  outb(0x1F7, 0x20);  // cmd 0x20 - read sectors

  waitdisk();
  insl(0x1F0, dst, SECTSIZE/4);
}

// Read 'count' bytes at 'offset' from kernel into physical address 'pa'.
void
readseg(uchar *pa, uint count, uint offset)
{
  uchar *epa;

  epa = pa + count;
  pa -= offset % SECTSIZE;
  offset = (offset / SECTSIZE) + 1;  // sector 0 is boot block

  for(; pa < epa; pa += SECTSIZE, offset++)
    readsect(pa, offset);
}
