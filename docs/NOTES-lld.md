# lld の RX 対応メモ（2026-11-03、issue #72）

CDEF-inc/llvm-project の `lld/ELF/Arch/RX.cpp`。`ld.lld` が RX（`EM_RX`、
`-m elf32rx`）の ELF を GNU ld（binutils 2.43 の `bfd/elf32-rx.c`）と同じ
結果でリンクする。lld が既定のリンカで（rx-rust #74 から）、GNU ld も
選べる。

## 使い方

- rustc: `rx-none-elf` の既定リンカは `rust-lld`
  （`LinkerFlavor::Gnu(Cc::No, Lld::Yes)`）。rustc は sysroot の
  `lib/rustlib/<host>/bin/rust-lld` を使う。`scripts/build-rustc.sh` が
  build/llvm の lld をそこに置き、配布物（`scripts/dist-toolchain.sh`）にも
  入る。配布物の `bin/ld.lld`（`rust-lld` へのリンク）は C などを直接
  リンクするためのもの。
- GNU ld にするには `-C linker=rx-elf-ld`（rustc は名前から GNU ld 型の
  フラグを選ぶ）。配布物は GNU binutils を含まない（`rx-elf-objcopy` などは
  LLVM のツールへのリンク）ので、GNU ld は別に用意する。
- テストスクリプト（`run-codegen-test.sh`、`run-rust-test.sh`、
  `hw-build.sh`）も既定は build/llvm の `ld.lld`。`RX_LD=<rx-elf-ld>` で
  GNU ld でリンクする（`hw-build.sh` は Rust アプリにも `-C linker` を
  渡す）。`run-rust-rom.sh` には `RUST_EXTRA_FLAGS="-C linker=<rx-elf-ld>"`。
- リンカスクリプトの解釈には GNU ld との違いがある（下の `.fixedvec` の
  例）。GNU ld で通っていたスクリプトが lld であふれたら、領域のない
  出力セクションを疑う。
- C のオブジェクトと混ぜてよい（GCC の libgcc もそのままリンクできる）。

## 再配置

GNU ld と同じ計算（`elf32-rx.c` の `rx_elf_relocate_section`）:

- `R_RX_DIR*`: S + A をリトルエンディアンで書く（コードもデータも。
  `_REV` はビッグエンディアン）。範囲は GNU ld と同じで、外れるとエラー。
  `DIR16UL` / `DIR16UW` / `DIR8UL` / `DIR8UW` は 4 / 2 で割って書く
  （割り切れないとエラー）。big-endian のデータ（`.big` 指定のセクション）
  は扱わない。
- PC 相対（`DIR8S_PCREL` / `DIR16S_PCREL` / `DIR24S_PCREL`）: 値は
  S + A − P + 1。フィールドは命令の 2 バイト目からで、変位は命令の先頭から
  測るため。`DIR3U_PCREL`（bra.s / beq.s / bne.s の 3 ビット、オペコード
  バイト内）は +1 しない。範囲は 3〜10。
- スタック再配置: `R_RX_SYM` が S + A を積み、`R_RX_OP*` が取り出して
  結果を積み、`R_RX_ABS*` が取り出して書く。`a - b` のように assembler が
  解決できない式に出る。オペランドの順は GNU ld と同じ（`OPsub` は
  次 − 先頭）。値は 32 ビットで折り返す（INT32_MIN / −1 は INT32_MIN、
  INT32_MIN % −1 は 0）。シフト量が 0〜31 の外なら、0 除算と同じくエラー。
- `R_RX_RH_RELAX`: リンカ緩和の印（下の「リンカ緩和」）。`--no-relax` なら無視する。

対応しないもの（エラーにする）: `R_RX_RH_*`（RELAX 以外の緩和用）、
GP 相対（`R_RX_GPRELB/W/L`）、セクション基準の演算子（`OPsctsize` /
`OPscttop` / `OPromtop` / `OPramtop`）。LLVM と GNU as の出すコードには
現れない（GNU as は `-mrelax` / `-msmall-data-limit` などでのみ出す）。

GNU ld は `R_RX_DIR8` などの一部を「未対応」としてエラーにするが、lld は
定義どおりに書く（lit テスト `rx-reloc.s` の `.byte small`）。

## e_flags

GNU ld の RX エミュレーションは `--no-flag-mismatch-warnings` が既定で、
フラグの違う入力を黙って混ぜ、出力は ABI / 64BIT_DOUBLES / DSP / PID の
和と、命令セット（V2 / V3）の最上位。SINSNS は最初にそれを設定した入力の
もの。lld も同じにする（不一致をエラーにすると、GCC の libgcc
（`-m64bit-doubles` なし）と Rust のオブジェクトを混ぜられない）。

## lld の都合で入れたもの

- `-m elf32rx` を受け付ける（rustc が渡す）。
- アーカイブ中の `lib.rmeta*` メンバは黙って飛ばす。rustc の `object`
  crate が RX を知らなかったころは、rlib のメタデータを ELF に包まず生のまま
  入れており、lld はそれを「ELF でない」と警告していた。今の rustc
  （CDEF-inc/rust#37、CDEF-inc/object の `Architecture::Rx` を使う）は RX の ELF
  （e_flags はコードのオブジェクトと同じ、文字列命令のビットを除く）に包むので、
  この特例は古い rlib のためだけに残っている。
- `rx72m-rom.ld`: `.fixedvec`（0xffffff80）にメモリ領域を割り当てた。
  GNU ld は領域なしの出力セクションを領域の外に置くが、lld は属性の合う
  直前の領域（ROM、0xffffff80 で終わる）に入れようとしてあふれていた。

## 検証

- lit: `lld/test/ELF/rx-reloc.s`（各再配置の値、GNU ld と同じバイト列を
  確認済み）、`rx-reloc-error.s`（範囲外）、`rx-reloc-stack.test`（スタックの
  演算の桁あふれ、シフト量、0 除算。yaml2obj で書く）、`rx-eflags.s`。lld の ELF
  テストは全部通る。
- `scripts/check-lld.sh`: codegen テスト 14 本 × rxv1 / rxv3 × llc 直接 /
  GNU as 経由を GNU ld と lld でリンクし、セクションの配置・中身・
  エントリ・e_flags を比べる（54 リンク、142 セクション、すべて一致）。
  アーカイブからのメンバの取り出し順は lld（深さ優先）と GNU ld
  （アーカイブを周回）で違い、配置が変わるので、GNU ld のマップから
  libgcc のメンバを GNU ld の順で明示して渡す。
- `run-all-tests.sh`: 全体を lld でリンクして回し、最後に rx62n / rxv3 で
  codegen テスト（-O2）と Rust アプリ（qemu-demo、isr-save、alloc-smoke、
  float-smoke、hw-irq）を GNU ld でリンクして QEMU で実行（#73 では逆）。
- 実機（AP-RX72M-0A）: `RX_LD=build/llvm/bin/ld.lld scripts/hw-build.sh`
  の 57 本を `hw-run.sh` で実行し、すべて失敗 0（NOTES-hw.md の「結果」）。

## リンカ緩和（2026-11-03、rx-rust #96 の B）

`-C target-feature=+relax`（LLVM の `-mattr=+relax`）でコンパイルしたコードを、lld がリンク時に短くする。
既定は無効。release の .text が約 3% 小さくなる（alloc-smoke 37,927 → 36,836 バイト、float-smoke
10,274 → 9,970、qemu-demo 2,558 → 2,418、isr-save 1,289 → 1,189。rxv3）。

- **LLVM（MC）が出すもの**（GNU as `-relax` と同じ形）:
  - **緩和の印:** `bsr.a` / `bra.a`（シンボルへ）と `mov.l #sym, rd` の前に `R_RX_RH_RELAX`。
    addend は GNU の `include/elf/rx.h` と同じで、`bsr.a` は 0x201（BRA、続く再配置 1 つ）、
    `mov.l` は 0x21（li フィールドが 12 ビット目、1 つ）。
  - **ラベルの差の扱い:** 緩和しうる命令をまたぐラベルの差は、アセンブル時に畳まず再配置にする。
    - 分岐の変位は PC 相対の再配置。
    - 行番号表は `DW_LNS_fixed_advance_pc` と `R_RX_SYM` / `OPsub` / `ABS16`（GNU as と同じ）。
    - CFI は `DW_CFA_advance_loc1/2/4` と同じスタック再配置。
    - データ中の差は `ABS8` / `ABS16` / `ABS32`。
  - **シンボル:** 再配置はセクション + オフセットでなくシンボル（ローカルラベルも）を指す。
    lld がバイトを消すとシンボルを動かすので、それに付いていく。
  - **関数内の分岐（#96 の A）の長さ:** 緩和の前の距離で選ぶ。リンカはバイトを消すだけなので、距離は
    縮むことはあっても伸びない。選んだ形はそのまま届く。
  - **コード中の整列:** 最大の詰め物（整列 − 1 個の NOP）と、その先頭に `R_RX_RH_RELAX`
    （`RX_RELAXA_ALIGN | 整列`）。これは RISC-V と同じやり方で、GNU as とは違う（GNU as は、計算した
    詰め物の後ろに印を置く）。
- **lld がすること**（`relaxOnce` / `finalizeRelax`、RISC-V と同じ枠組み）:
  - `bsr.a` → `bsr.w`、`bra.a` → `bra.w` / `bra.b`（変位が届けば）。
  - `mov.l #imm32` → `#simm24` / `#simm16` / `#simm8`（アドレスが収まれば。li フィールドを書き換え、
    即値の上位バイトを消す）。
  - 不要になった整列の NOP を消す。
  - 消したバイトに合わせて、シンボルの値と大きさ、再配置の位置を詰める。届く距離が変わるので、
    収束するまで繰り返す（4 回目からは、消すバイトを増やさない）。
- **対応しないもの:**
  - GNU as `-relax` の、整列の印（形が違う）と DSP / IMM6 以外の印。IMM6 は扱うが、確かめていない。
  - `bra.a` 以外の条件分岐の伸び縮み。
- **確かめたこと:**
  - lit の `rx-relax.s`。
  - QEMU（rx62n / rxv3）で codegen テストと Rust アプリ。
  - 緩和したビルドでの LLDB のセッション（`check-lldb.sh`、31 項目。行番号・CFI・変数が正しい）と、
    割り込み入口の CFI（`check-isr-cfi.py`）。
  - 実機（NOTES-hw.md）。

## しないこと

- GNU ld の `-Map` 互換の出力。lld の `-Map` は形式が違う。
