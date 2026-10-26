# M1 MC 層メモ（2026-10-23）

## 実装済み
- `lib/Target/RX`: 命令定義（RXInstrInfo.td）、MCCodeEmitter、AsmParser、AsmBackend、ELFObjectWriter、ELF ターゲットストリーマ
- `include/llvm/BinaryFormat/ELFRelocs/RX.def`（R_RX_* 全種、binutils include/elf/rx.h と同じ番号）
- 命令（各サイズの符号化を別命令として定義し、AsmMatcher のオペランドクラスの包含関係で gas と同じ最短符号を選ぶ）
  - mov.{b,w,l} Rs,Rd / [Rs] / dsp5[Rs]（r0-r7 の 2 バイト形式）/ dsp8 / dsp16（ロード・ストア）、mov.l #imm（uimm4/uimm8/simm8/16/24/32）
  - movu.{b,w}（レジスタ、ロード各形式）
  - add/sub/cmp/mul/and/or（レジスタ、#uimm4、#simm8〜32、3 オペランド）、xor 3 オペランド（RXv3）
  - FC 系 2 レジスタ: sbb neg adc abs max min emul emulu div divu tst xor not
  - FD 70 系即値: adc max min emul emulu div divu xor
  - 単項: not neg abs sat rorc rolc
  - シフト・回転: shlr/shar/shll（#imm, Rs, 3 オペランド #imm）、rotl/rotr（#imm, Rs）、revw/revl
  - スタック: push.{b,w,l} pop pushm popm rtsd #imm / rtsd #imm, Rlo-Rhi
  - 制御: brk dbt rts nop satr rtfi rte wait jmp jsr bra.l bsr.l
  - 分岐: bra.{s,b,w,a} bsr.{w,a} beq/bne.{s,w} b<cc>.b（eq ne geu ltu gtu leu pz n ge lt gt le o no、別名 bz bnz bc bnc）
  - stz/stnz #imm（simm8〜32）、RXv2 の stz/stnz Rs, Rd
  - 単精度 FPU: fadd/fsub/fmul/fdiv/fcmp（Rs、[Rs]、dsp8/dsp16[Rs]（.L 省略可）、#imm32）、ftoi/round（Rs、メモリ）、itof Rs, Rd
  - RXv2 の FPU: fadd/fsub/fmul 3 オペランド、fsqrt/ftou（Rs、メモリ）、utof Rs, Rd
  - DSP: RXv1 形式（A0 暗黙）mulhi/mullo/machi/maclo、mvtachi/mvtaclo、mvfachi/mvfacmi/mvfaclo、racw
  - RXv2 の DSP（A0/A1 を明示）: mulhi mullo mullh emula machi maclo maclh emaca msbhi msblo msblh emsba、
    mvtac{hi,lo,gu}、mvfac{hi,mi,lo,gu} #0〜2、racw/rdacw/racl/rdacl #1〜2
  - RXv2 の排他アクセス: movli [Rs], Rd / movco Rs, [Rd]
  - RXv2 の命令は `-mcpu=rxv2` 以上（`+rxv2`）でのみ受け付ける。FPU 命令は CPU で制限しない（gas も rx100/rx200 以外は通す）
- フィックスアップ: pcrel3（.S、オペコード内）、pcrel8/16/24（命令先頭+1、値は命令先頭基準）、FK_Data_1/2/4
- e_flags: gas と同じ（E_FLAG_RX_ABI、V2/V3、`-mattr=+64bit-doubles` で E_FLAG_RX_64BIT_DOUBLES）

## 検証（scripts/mc-diff.sh）
| テスト | 内容 | 結果 |
|---|---|---|
| `test/mc/basic.s`（lines） | 1 行 1 命令、符号切替の境界値 | 210/210 一致 |
| `test/mc/branch.s`（file） | ローカルラベル分岐・外部シンボル分岐 | .text / e_flags / リロケーション一致 |
| `test/mc/symbol.s`（file） | `#sym` 即値、`.long sym` | .text / e_flags / リロケーション一致 |
| `test/mc/fpu.s`（lines） | 単精度 FPU と RXv2 の追加命令、dsp8/dsp16 の境界 | 55/55 一致 |
| `test/mc/dsp.s`（lines） | DSP の RXv1 形式と A0/A1 指定形式 | 51/51 一致 |
| `test/mc/v2.s`（lines） | stz/stnz、movli/movco | 19/19 一致 |
- fpu.s / dsp.s / v2.s の `# RXv2` より後の行は `-mcpu=rxv1` で拒否され、前の行は受け付けられることを確認
- llvm-mc のアセンブリ出力を GNU as で再アセンブルしても同一バイト列（basic/branch/symbol、fpu/dsp/v2）
- GCC（-m64bit-doubles）のオブジェクトと `rx-elf-ld` でリンクでき、呼び出しが解決されることを確認

実行方法:
```
bash scripts/build-mc.sh                      # パッチ適用 + llvm-mc/llc ビルド
bash scripts/mc-diff.sh lines test/mc/basic.s
bash scripts/mc-diff.sh file  test/mc/branch.s
bash scripts/mc-diff.sh file  test/mc/symbol.s
bash scripts/mc-diff.sh lines test/mc/fpu.s   # dsp.s, v2.s も同様
```

## gas との既知の違い
- **サイズ指定なしの分岐**（`bra label`、`beq label`、`bsr label`）は未対応。gas は距離に応じて緩和する。
  M2 では codegen 側でサイズ付き分岐を出し、BranchRelaxation パスで伸ばす方針。
- **定数の分岐先**: gas は `bra.b 8` の 8 を絶対番地として扱うが、LLVM は変位として扱う（実用上は使わない）。
- **`#sym` 即値**: LLVM は常に 32bit 形式 + R_RX_DIR32。gas は同一ファイル内で値が決まるシンボルなら短い形式に緩和する
  （未定義シンボルでは一致を確認済み）。
- `sp` は LLVM では r0 の別名として受け付けるが、gas は受け付けない。
- gas の既定コードセクション名は `P`（Renesas 流）、LLVM は `.text`。リンカスクリプトで両方を拾うこと（M5）。

## 未実装（M1 の残り）
- memex 付きメモリオペランド（`add [r1].l, r2`、06 プレフィクス形式）、`.UB` 短縮形式
- mov #imm → メモリ、メモリ → メモリ、`[Rn+]`/`[-Rn]`、インデックス `[Ri, Rb]`
- ビット操作（bset/bclr/btst/bnot/bm<cc>）、xchg、string 命令
- itof/utof のメモリオペランド（memex でサイズを指定する形式）
- DFPU（dadd 等）、RXv3 の追加命令（bfmov/bfmovz、save/rstr）
- RXv3e の Protect Zone 命令（NOTES-rxv3e.md）
- 逆アセンブラ（当面は rx-elf-objdump を使う）
