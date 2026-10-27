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
  - RXv3: bfmov/bfmovz #slsb, #dlsb, #width, Rs, Rd、save/rstr（Rs、#uimm8）
  - RXv3e（`-mcpu=rxv3e`）: trcall #uimm8、trrte、trmvfc cr, Rd、trmvtc Rs, cr / #simm8〜32, cr、int Rs、
    mvtipl #0〜31（他のコアは #0〜15）。制御レジスタ ivc / wpri / wid（mvtc / mvfc / pushc / popc）、
    trmvtc / trmvfc では isp / intb / bpc / bpsw / extb / ivc / trctb / wintb。これらのレジスタ名も rxv3e でのみ受け付ける
  - DFPU（`+dfpu`、rxv3 / rxv3e の既定）: dadd/dsub/dmul/ddiv、dabs/dneg/dsqrt/dround/dtoi/dtou/dtof、
    dcmp{un,eq,lt,le}、ftod/itod/utod、dmov.d（DR↔DR、[Rb]/dsp8/dsp16（8 倍）、Rs→DRHn、#imm32→DRHn）、
    dmov.l（Rs↔DRHn/DRLn、#imm32→DRHn/DRLn）、dpushm/dpopm.{d,l}、mvtdc/mvfdc、mvfdr
  - レジスタ: DR0〜DR15、半分を指す drh0〜15 / drl0〜15（dmov.l 用、アセンブラのみ）、
    DFPU 制御レジスタ dpsw/dcmr/decnt/depc（別名 dcr0〜dcr3）
  - gas は DFPU 命令に `-mcpu=rxv3-dfpu` が要る（e_flags は rxv3 と同じ）。mc-diff.sh は gas 側をこれで動かす
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
| `test/mc/dfpu.s`（lines） | DFPU 全命令、dmov.d の dsp8/dsp16 境界、レジスタ範囲 | 59/59（うち 10 行はマニュアルの符号と比較） |
| `test/mc/v3.s`（lines） | bfmov/bfmovz、save/rstr | 17/17 一致 |
| `test/mc/v3e.s`（lines、`# mcpu: rxv3e`） | Protect Zone 命令、int Rs、5 ビットの mvtipl、追加の制御レジスタ | 53/53（うち 50 行はマニュアルの符号と比較。GNU as は RXv3e 未対応） |
- fpu.s / dsp.s / v2.s の `# RXv2` より後の行は `-mcpu=rxv1` で拒否され、前の行は受け付けられることを確認
- dfpu.s は `-mcpu=rxv2` と `-mcpu=rxv3 -mattr=-dfpu` で、v3.s は `-mcpu=rxv2` で全行拒否。rxv3e では全行受け付け
- v3e.s は `-mcpu=rxv3` では int #3、mvtipl #0 / #15 以外の全行を拒否
- llvm-mc のアセンブリ出力を GNU as で再アセンブルしても同一バイト列（basic/branch/symbol、fpu/dsp/v2、dfpu/v3。
  ただし下記の ftod/itod/utod/mvtdc は GNU as 側が誤るので除く）
- GCC（-m64bit-doubles）のオブジェクトと `rx-elf-ld` でリンクでき、呼び出しが解決されることを確認

実行方法:
```
bash scripts/build-mc.sh                      # パッチ適用 + llvm-mc/llc ビルド
bash scripts/mc-diff.sh lines test/mc/basic.s
bash scripts/mc-diff.sh file  test/mc/branch.s
bash scripts/mc-diff.sh file  test/mc/symbol.s
bash scripts/mc-diff.sh lines test/mc/fpu.s   # dsp.s, v2.s, dfpu.s, v3.s も同様
```

## gas との既知の違い
- **GNU as 2.43 の誤り: ftod / itod / utod / mvtdc**。RXv3 マニュアル（4 章、p.376・378）の符号は
  `1111 1101 0111 0111 1000 rs[3:0] rd[3:0] op`（dmov.l Rs, DRHd と同じく汎用レジスタが 2 バイト目の下位）だが、
  gas はこの 2 つのフィールドを入れ替えて出力する（`itod r3, dr4` → `fd 77 84 39`。binutils 自身の objdump は
  これを `itod r4, dr3` と読む。mvtdc は objdump も同じ向きに誤っている）。LLVM はマニュアルどおりに符号化し、
  dfpu.s のこれらの行は `; bytes:` でマニュアルの符号と比較する（mc-diff.sh）。llvm-project#2 のレビューで判明。
- **サイズ指定なしの書き方**（2026-10-28 対応、test/mc/sizeless.s）: gas と同じく `mov` / `push` / `sstr` /
  `suntil` / `swhile` / `rmpa` は `.l`、`movu` は `.w`、`bra Rn` / `bsr Rn` は `.l`。`mov #imm, メモリ` はサイズが要る
  （gas もエラー）。ラベルへの `bra` / `bsr` / `b<cond>` は疑似命令 BR_SIZELESS にし、RXAsmBackend の緩和で
  gas と同じ形にする: bra は .s → .b → .w → .a、bsr は .w → .a、beq / bne は .s → .b → .w → `b<!cond>.s` + bra.a、
  その他の条件は .b → `b<!cond>.b` + bra.w → `b<!cond>.b` + bra.a。距離は列の先頭から測る（gas と同じ）。
  解決できない分岐先（未定義・global・別セクション）は gas と同じく bra.a / bsr.a / beq.w・bne.w / b<cond>.b。
  codegen はサイズ付き分岐を出し、BranchRelaxation パスで伸ばす。
- **未定義シンボルの差の即値**（`#(_e - _s)`、`.long _e - _s`）: gas と同じく 32bit の欄に
  R_RX_SYM A+C、R_RX_SYM B、R_RX_OPsub、R_RX_ABS32 のスタック計算リロケーションを出す。
- **定数の分岐先**: gas は `bra.b 8` の 8 を絶対番地として扱うが、LLVM は変位として扱う（実用上は使わない）。
  サイズなしの `bra 8` は LLVM ではエラーにする。
- **`#sym` 即値**: LLVM は常に 32bit 形式 + R_RX_DIR32。gas は同一ファイル内で値が決まるシンボルなら短い形式に緩和する
  （未定義シンボルでは一致を確認済み）。`#(2f - 1b)` のように後で値が決まる式も同じ（gas は配置後の値で短い形式）。
- `sp` は LLVM では r0 の別名として受け付けるが、gas は受け付けない。
- gas の既定コードセクション名は `P`（Renesas 流）、LLVM は `.text`。リンカスクリプトで両方を拾うこと（M5）。

## 未実装（M1 の残り）
- memex 付きメモリオペランド（`add [r1].l, r2`、06 プレフィクス形式、gas はサイズなしの `add [r1], r2` を .l とする）、
  `.UB` 短縮形式
- `push.l` のメモリ形式（`push [r1]`）
- ビット操作（bset/bclr/btst/bnot/bm<cc>）、xchg
- itof/utof のメモリオペランド（memex でサイズを指定する形式）
- 逆アセンブラ（RX の MC 層にまだない）
- 逆アセンブラ（当面は rx-elf-objdump を使う）
