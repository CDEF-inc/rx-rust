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
  入る。配布物の `bin/ld.lld` は C などを直接リンクするためのもの。
- GNU ld にするには `-C linker=rx-elf-ld`（rustc は名前から GNU ld 型の
  フラグを選ぶ）。
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
- `R_RX_RH_RELAX`: 無視する（lld は緩和をしない。GNU ld も `--relax`
  なしでは何もしない）。

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
  crate は RX を知らないため、rlib のメタデータを ELF に包まず生のまま
  入れており、lld はそれを「ELF でない」と警告していた。
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

## しないこと

- 緩和（`--relax`）。GNU ld の RX 緩和は Red Hat の `RH_*` 再配置が前提で、
  LLVM はそれを出さない。
- GNU ld の `-Map` 互換の出力。lld の `-Map` は形式が違う。
