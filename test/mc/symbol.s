; Symbolic immediates and data. Checked with scripts/mc-diff.sh file.
	.global	f
f:
	mov.l	#ext, r1
	mov.l	#ext+4, r15
	add	#ext, r2, r3
	cmp	#ext, r4
	mov.l	#ext, [r9]
	mov.l	#ext+4, 8[r2]
	rts
	.long	ext
	.long	ext+8
