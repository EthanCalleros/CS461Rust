
build/bootblocktmp.o:     file format elf32-i386


Disassembly of section .text:

00007c00 <start>:
    7c00:	fa                   	cli
    7c01:	31 c0                	xor    %eax,%eax
    7c03:	8e d8                	mov    %eax,%ds
    7c05:	8e c0                	mov    %eax,%es
    7c07:	8e d0                	mov    %eax,%ss

00007c09 <seta20.1>:
    7c09:	e4 64                	in     $0x64,%al
    7c0b:	a8 02                	test   $0x2,%al
    7c0d:	75 fa                	jne    7c09 <seta20.1>
    7c0f:	b0 d1                	mov    $0xd1,%al
    7c11:	e6 64                	out    %al,$0x64

00007c13 <seta20.2>:
    7c13:	e4 64                	in     $0x64,%al
    7c15:	a8 02                	test   $0x2,%al
    7c17:	75 fa                	jne    7c13 <seta20.2>
    7c19:	b0 df                	mov    $0xdf,%al
    7c1b:	e6 60                	out    %al,$0x60
    7c1d:	0f 01 16             	lgdtl  (%esi)
    7c20:	68 7c 0f 20 c0       	push   $0xc0200f7c
    7c25:	66 83 c8 01          	or     $0x1,%ax
    7c29:	0f 22 c0             	mov    %eax,%cr0
    7c2c:	ea                   	.byte 0xea
    7c2d:	31 7c 08 00          	xor    %edi,0x0(%eax,%ecx,1)

00007c31 <start32>:
    7c31:	66 b8 10 00          	mov    $0x10,%ax
    7c35:	8e d8                	mov    %eax,%ds
    7c37:	8e c0                	mov    %eax,%es
    7c39:	8e d0                	mov    %eax,%ss
    7c3b:	66 b8 00 00          	mov    $0x0,%ax
    7c3f:	8e e0                	mov    %eax,%fs
    7c41:	8e e8                	mov    %eax,%gs
    7c43:	bc 00 7c 00 00       	mov    $0x7c00,%esp
    7c48:	e8 d4 00 00 00       	call   7d21 <bootmain>

00007c4d <spin>:
    7c4d:	eb fe                	jmp    7c4d <spin>
    7c4f:	90                   	nop

00007c50 <gdt>:
	...
    7c58:	ff                   	(bad)
    7c59:	ff 00                	incl   (%eax)
    7c5b:	00 00                	add    %al,(%eax)
    7c5d:	9a cf 00 ff ff 00 00 	lcall  $0x0,$0xffff00cf
    7c64:	00                   	.byte 0
    7c65:	92                   	xchg   %eax,%edx
    7c66:	cf                   	iret
	...

00007c68 <gdtdesc>:
    7c68:	17                   	pop    %ss
    7c69:	00 50 7c             	add    %dl,0x7c(%eax)
	...

00007c6e <waitdisk>:
    7c6e:	ba f7 01 00 00       	mov    $0x1f7,%edx
    7c73:	ec                   	in     (%dx),%al
    7c74:	83 e0 c0             	and    $0xffffffc0,%eax
    7c77:	3c 40                	cmp    $0x40,%al
    7c79:	75 f8                	jne    7c73 <waitdisk+0x5>
    7c7b:	c3                   	ret

00007c7c <readsect>:
    7c7c:	57                   	push   %edi
    7c7d:	53                   	push   %ebx
    7c7e:	8b 5c 24 10          	mov    0x10(%esp),%ebx
    7c82:	e8 e7 ff ff ff       	call   7c6e <waitdisk>
    7c87:	b8 01 00 00 00       	mov    $0x1,%eax
    7c8c:	ba f2 01 00 00       	mov    $0x1f2,%edx
    7c91:	ee                   	out    %al,(%dx)
    7c92:	ba f3 01 00 00       	mov    $0x1f3,%edx
    7c97:	89 d8                	mov    %ebx,%eax
    7c99:	ee                   	out    %al,(%dx)
    7c9a:	89 d8                	mov    %ebx,%eax
    7c9c:	c1 e8 08             	shr    $0x8,%eax
    7c9f:	ba f4 01 00 00       	mov    $0x1f4,%edx
    7ca4:	ee                   	out    %al,(%dx)
    7ca5:	89 d8                	mov    %ebx,%eax
    7ca7:	c1 e8 10             	shr    $0x10,%eax
    7caa:	ba f5 01 00 00       	mov    $0x1f5,%edx
    7caf:	ee                   	out    %al,(%dx)
    7cb0:	89 d8                	mov    %ebx,%eax
    7cb2:	c1 e8 18             	shr    $0x18,%eax
    7cb5:	83 c8 e0             	or     $0xffffffe0,%eax
    7cb8:	ba f6 01 00 00       	mov    $0x1f6,%edx
    7cbd:	ee                   	out    %al,(%dx)
    7cbe:	b8 20 00 00 00       	mov    $0x20,%eax
    7cc3:	ba f7 01 00 00       	mov    $0x1f7,%edx
    7cc8:	ee                   	out    %al,(%dx)
    7cc9:	e8 a0 ff ff ff       	call   7c6e <waitdisk>
    7cce:	8b 7c 24 0c          	mov    0xc(%esp),%edi
    7cd2:	b9 80 00 00 00       	mov    $0x80,%ecx
    7cd7:	ba f0 01 00 00       	mov    $0x1f0,%edx
    7cdc:	fc                   	cld
    7cdd:	f3 6d                	rep insl (%dx),%es:(%edi)
    7cdf:	5b                   	pop    %ebx
    7ce0:	5f                   	pop    %edi
    7ce1:	c3                   	ret

00007ce2 <readseg>:
    7ce2:	57                   	push   %edi
    7ce3:	56                   	push   %esi
    7ce4:	53                   	push   %ebx
    7ce5:	8b 5c 24 10          	mov    0x10(%esp),%ebx
    7ce9:	8b 74 24 18          	mov    0x18(%esp),%esi
    7ced:	89 df                	mov    %ebx,%edi
    7cef:	03 7c 24 14          	add    0x14(%esp),%edi
    7cf3:	89 f0                	mov    %esi,%eax
    7cf5:	25 ff 01 00 00       	and    $0x1ff,%eax
    7cfa:	29 c3                	sub    %eax,%ebx
    7cfc:	c1 ee 09             	shr    $0x9,%esi
    7cff:	83 c6 01             	add    $0x1,%esi
    7d02:	39 fb                	cmp    %edi,%ebx
    7d04:	73 17                	jae    7d1d <readseg+0x3b>
    7d06:	56                   	push   %esi
    7d07:	53                   	push   %ebx
    7d08:	e8 6f ff ff ff       	call   7c7c <readsect>
    7d0d:	81 c3 00 02 00 00    	add    $0x200,%ebx
    7d13:	83 c6 01             	add    $0x1,%esi
    7d16:	83 c4 08             	add    $0x8,%esp
    7d19:	39 fb                	cmp    %edi,%ebx
    7d1b:	72 e9                	jb     7d06 <readseg+0x24>
    7d1d:	5b                   	pop    %ebx
    7d1e:	5e                   	pop    %esi
    7d1f:	5f                   	pop    %edi
    7d20:	c3                   	ret

00007d21 <bootmain>:
    7d21:	55                   	push   %ebp
    7d22:	57                   	push   %edi
    7d23:	56                   	push   %esi
    7d24:	53                   	push   %ebx
    7d25:	83 ec 0c             	sub    $0xc,%esp
    7d28:	6a 00                	push   $0x0
    7d2a:	68 00 20 00 00       	push   $0x2000
    7d2f:	68 00 00 01 00       	push   $0x10000
    7d34:	e8 a9 ff ff ff       	call   7ce2 <readseg>
    7d39:	83 c4 0c             	add    $0xc,%esp
    7d3c:	bb 00 00 00 00       	mov    $0x0,%ebx
    7d41:	bd 00 00 00 00       	mov    $0x0,%ebp
    7d46:	eb 0e                	jmp    7d56 <bootmain+0x35>
    7d48:	ff 56 1c             	call   *0x1c(%esi)
    7d4b:	83 c3 04             	add    $0x4,%ebx
    7d4e:	81 fb 00 20 00 00    	cmp    $0x2000,%ebx
    7d54:	74 55                	je     7dab <bootmain+0x8a>
    7d56:	8d b3 00 00 01 00    	lea    0x10000(%ebx),%esi
    7d5c:	81 bb 00 00 01 00 02 	cmpl   $0x1badb002,0x10000(%ebx)
    7d63:	b0 ad 1b 
    7d66:	75 e3                	jne    7d4b <bootmain+0x2a>
    7d68:	8b 46 04             	mov    0x4(%esi),%eax
    7d6b:	89 c2                	mov    %eax,%edx
    7d6d:	03 56 08             	add    0x8(%esi),%edx
    7d70:	81 fa fe 4f 52 e4    	cmp    $0xe4524ffe,%edx
    7d76:	75 d3                	jne    7d4b <bootmain+0x2a>
    7d78:	a9 00 00 01 00       	test   $0x10000,%eax
    7d7d:	74 cc                	je     7d4b <bootmain+0x2a>
    7d7f:	8b 46 10             	mov    0x10(%esi),%eax
    7d82:	8d 14 18             	lea    (%eax,%ebx,1),%edx
    7d85:	2b 56 0c             	sub    0xc(%esi),%edx
    7d88:	52                   	push   %edx
    7d89:	8b 56 14             	mov    0x14(%esi),%edx
    7d8c:	29 c2                	sub    %eax,%edx
    7d8e:	52                   	push   %edx
    7d8f:	50                   	push   %eax
    7d90:	e8 4d ff ff ff       	call   7ce2 <readseg>
    7d95:	8b 4e 18             	mov    0x18(%esi),%ecx
    7d98:	8b 7e 14             	mov    0x14(%esi),%edi
    7d9b:	83 c4 0c             	add    $0xc,%esp
    7d9e:	39 cf                	cmp    %ecx,%edi
    7da0:	73 a6                	jae    7d48 <bootmain+0x27>
    7da2:	29 f9                	sub    %edi,%ecx
    7da4:	89 e8                	mov    %ebp,%eax
    7da6:	fc                   	cld
    7da7:	f3 aa                	rep stos %al,%es:(%edi)
    7da9:	eb 9d                	jmp    7d48 <bootmain+0x27>
    7dab:	eb fe                	jmp    7dab <bootmain+0x8a>
