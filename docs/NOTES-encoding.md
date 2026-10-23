# RX 命令エンコーディング / ELF メモ（binutils 2.43 から抽出した事実）

参照元: `gnu/src/binutils-2.43/opcodes/rx-decode.opc`（デコード表）, `bfd/elf32-rx.c`, `gas/config/tc-rx.c`, `include/elf/rx.h`
※ binutils は GPL。コードは流用せず、ISA の事実のみを参照して独自に TableGen を書く。

## 基本
- 命令長 1〜8 バイト可変。オペコードビットはバイト列として先頭から（MSB first）。
- 即値・ディスプレースメントはリトルエンディアン（RX_OPCODE_BIG_ENDIAN=0 の場合）。
- 即値サイズフィールド `im`/`li`（2bit）: 01=simm8, 10=simm16, 11=simm24, 00=imm32。
  - mov `1111 10sd rdst im sz` は (im,sz) が (1,0)/(2,1)/(0,2) のとき符号拡張なし、それ以外は符号拡張。
- メモリオペランド `ld`/`sd`（2bit）: 00=[Rn], 01=dsp8[Rn], 10=dsp16[Rn], 11=Rn（レジスタ直接）。
  - dsp はオペランドサイズでスケーリング（.B=1, .W=2, .L=4）。dsp16 は LE。
- memex（`mi`/`mx`）: 00=.B(符号), 01=.W(符号), 10=.L, 11=.UW。`06` プレフィクス形式で使う。

## 分岐
- PC 相対の基準はその命令の先頭アドレス。
  - `bra.s`（3bit dsp, 3〜10）, `b{cond}.b`（8bit）, `bra.w`/`b{eq,ne}.w`（16bit）, `bra.a`/`bsr.a`（24bit, オペコード 0x04/0x05）
  - `bsr.w` 0x39, `bra.w` 0x38, `bra.b` 0x2e, `rts` 0x02, `nop` 0x03
  - `jsr Rs` = 7F 1s, `jmp Rs` = 7F 0s, `bsr.l Rs` = 7F 5s, `bra.l Rs` = 7F 4s

## ELF
- e_machine = EM_RX (173)。RELA 形式。
- e_flags: `E_FLAG_RX_64BIT_DOUBLES`=1<<0, `E_FLAG_RX_ABI`=1<<3, `E_FLAG_RX_V2`=1<<8, `E_FLAG_RX_V3`=1<<9。
  Rust 用は 64bit double が必須 → bit0 を立てる。GCC 側も `-m64bit-doubles` でリンク。
- 主要リロケーション:
  - R_RX_DIR32 (0x01) 32bit 絶対 LE（`mov.l #sym`, データ）
  - R_RX_DIR24S_PCREL (0x09) / DIR16S_PCREL (0x0a) / DIR8S_PCREL (0x0b)
  - R_RX_DIR3U_PCREL (0x12)（bra.s）
- **注意: ld.bfd は PC 相対で `S + A - P + 1` を計算する**（P はフィールド位置、フィールドは命令先頭+1 にあるため）。
  したがって LLVM 側は addend に -1 を入れず、オペコード直後のフィールドに素直に reloc を張ればよい。
  DIR3U_PCREL のみ +1 補正なし。
- リンクは当面 GNU ld（rx-elf-ld）を使う。lld 対応は後回し。リラクゼーション用 R_RX_RH_RELAX は出さない。
