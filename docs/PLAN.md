# RX (RXv3) 向け Rust ツールチェイン — LLVM RX バックエンド自作計画

## 前提
- upstream LLVM / Rust に RX ターゲットは無い。Renesas/OSTS の LLVM も RL78・RISC-V のみ（RX は GCC のみ）。
- 方針: rustc 同梱の LLVM フォーク (`rust/src/llvm-project`) に `lib/Target/RX` を新規実装し、rustc に `rx-none-elf` ターゲットを追加する。
- 対象コア: RXv3（RX66N/RX72N 等）と RXv3e（RX74M/RX74N）、リトルエンディアン、倍精度 FPU あり。
  RXv3e のコード生成は RXv3 + DFPU と同一（`NOTES-rxv3e.md`）。
- ABI は GNURX（Renesas 版 GCC、`-misa=v3 -mdfpu -m64bit-doubles`）と互換にし、libgcc / newlib / Renesas FIT(C) とリンク可能にする。
  上流 GCC 14.2 は `-mcpu=rx100|rx200|rx600|rx610` のみで RXv2/v3/DFPU を知らない。ABI の基本（引数・アラインメント）は共通。
- C のシンボルには `_` が前置される（GCC rx: `USER_LABEL_PREFIX "_"`）。LLVM の DataLayout `m:` と
  Rust の `#[no_mangle]`/`extern "C"` 名の扱いを M2/M4 で決めること。
- リポジトリ: https://github.com/CDEF-inc/rx-rust（作業場所: WSL Ubuntu `~/rx-rust`）

## ディレクトリ
- `rust/` — サブモジュール CDEF-inc/rust `rx-none-elf`（rust-lang/rust `7189d39d9ce` + RX）。
  その中の `src/llvm-project` は CDEF-inc/llvm-project `rx/23.1`（LLVM 23.1.4 `f738f3b0` + RX バックエンド）
- `crates/rx-rt/` — 起動コード・ベクタ表・リンカスクリプト（M5）
- `scripts/`, `test/` — ビルド・検証スクリプトとテスト、`docs/` — このファイルと NOTES-*.md
- `gnu/install/`（`scripts/build-gnu.sh`、リポジトリ外）— 参照用 rx-elf binutils 2.43 / 上流 GCC 14.2 + libgcc（エンコーディング検証・ABI 比較・リンカ）
- `~/work/gnurx/` — GNURX 14.2 ソース（RXv2/v3/DFPU の ABI 参照）。`~/opt/gnurx-14.2` はコンパイラのみ（as/ld なし）
- `build/llvm/` — LLVM ビルド（Release + assertions、X86 + RX、ccache）
- `ref/` — RXv3e ISA マニュアル（PDF とテキスト版）。Renesas の著作物なのでコミットしない（.gitignore）

## マイルストーン
### M0 環境 ✅ 2026-10-23
- [x] rust / llvm-project 取得、LLVM 単体を `-DLLVM_EXPERIMENTAL_TARGETS_TO_BUILD=RX` でビルドできる骨組み
  - Triple に `rx` 追加（Triple.h/.cpp, TargetDataLayout.cpp, ELFObjectFile.h）
  - `lib/Target/RX`: TargetInfo, MCTargetDesc（AsmInfo/InstPrinter）, 空の TargetMachine, TableGen（レジスタ, brk/rts/nop）
  - CPU: generic/rxv1/rxv2/rxv3/rxv3e、機能: rxv2/rxv3/rxv3e/dfpu
  - 確認: `llc --version` に `rx - Renesas RX`、`llc -mtriple=rx-none-elf -mcpu=help`
- [x] 参照 rx-elf-gcc / objdump（`gnu/install/bin`）
- 実測（rx-elf-gcc -mcpu=rx600 -m64bit-doubles）: i64/f64/long double は size 8・align 4、char は unsigned、BIGGEST_ALIGNMENT=4
  → DataLayout `e-m:o-p:32:32-i64:32-f64:32-n8:16:32-S32`（M2 で `m:o` に変更し C シンボルに `_` を付与）
- GNURX 由来の事実: DWARF 番号 R0-15=0-15, PSW=16, PC=17, DRn=32+n。`-mdfpu` でも f64 引数/戻り値は整数レジスタ。DR0-15 は全て caller-saved
- ビルド手順: `scripts/build-llvm.sh`（GCC は `scripts/build-gnu.sh`。`cmake -G Ninja ... -DLLVM_PARALLEL_COMPILE_JOBS=6 -DLLVM_PARALLEL_LINK_JOBS=1`）。
  WSL のメモリが 7GB なので並列度を上げすぎない。GCC ビルドと同時に走らせない

### M1 MC 層（アセンブラ・オブジェクト出力）— 基本命令は完了（詳細 `NOTES-mc.md`）
- [x] `RXRegisterInfo.td`（R0=SP〜R15, PSW, FPSW, ACC0/1, DR0〜DR15、GPRLow=R0〜R7）
- [x] `RXInstrFormats.td` / `RXInstrInfo.td`（可変長 1〜8 バイト命令）— 整数・シフト・スタック・分岐
- [x] AsmParser / InstPrinter / MCCodeEmitter / AsmBackend / ELFObjectWriter（EM_RX=173, R_RX_*）/ e_flags
- [x] 検証: `scripts/mc-diff.sh` で GNU as と .text・e_flags・リロケーションを比較（basic 210/210、branch、symbol 一致）
- [x] 単精度 FPU、DSP（A0/A1）、stz/stnz、RXv2 の追加命令（fsqrt/ftou/utof、FPU 3 オペランド、movli/movco、DSP 拡張）。
  fpu 55/55、dsp 51/51、v2 19/19 一致（issue #1）
- [ ] memex・06 プレフィクス形式（itof/utof のメモリ形式を含む）、ビット操作、DFPU と RXv3 の追加命令、string 命令
- [x] codegen はサイズ付き分岐 + BranchRelaxation で対応（M2）。アセンブラ入力のサイズ指定なし分岐は未対応のまま
- [x] `sc<cond>.l`（M2 で追加、gas と 14/14 一致）
- [ ] RXv3e の Protect Zone 命令（trcall/trmvfc/trmvtc/trrte）と mvfc/mvtc の追加レジスタ。binutils 未対応なのでマニュアルの符号表で検証

### M2 コード生成（i32 整数のみ）✅ 2026-10-23（詳細 `NOTES-codegen.md`）
- [x] CallingConv.td（GNURX 互換。引数 R1〜R4、戻り値 R1、callee-saved R6〜R13、char/short は int に昇格）
- [x] FrameLowering（push.l/pop で CSR、SP 相対、FP なし）、ISelDAGToDAG / ISelLowering、分岐・比較・ロード/ストア
- [x] 選択は最長形式、出力時に最短符号へ縮小（RXMCInstLower の shrinkRXInst）
- [x] 検証: `scripts/run-codegen-test.sh` — llc → GCC ドライバ + libgcc とリンク → QEMU（gdbsim-r5f562n8, RXv1）で実行。
  4131 チェック全通過（-O0〜-O3、.s 経由・.o 直接とも）
- [ ] プロローグ最適化（pushm/popm/rtsd）、可変長引数の正確な規約、動的 alloca、FP、CFI
- [ ] RXv2/v3 命令を使う codegen の実行検証（QEMU は RXv1 のみ → GDB sim か実機）

### M3 拡張 — i64 / float / double は完了（詳細 `NOTES-codegen.md` の M3 節）
- [x] 引数配置を GCC の規則（累積バイト数、溢れたら以降すべてスタック、可変長は最後の名前付きからスタック）で実装
- [x] i64（add/adc, sub/sbb、乗除算・シフトは libgcc / 展開）
- [x] float / double はソフトフロート（libgcc soft-fp、-m64bit-doubles multilib）。RuntimeLibcalls.td に RX を登録
- [x] GCC との相互呼び出しテスト（m3.ll + m3_main.c、3964 チェック全通過、-O0〜-O3）
- [x] 構造体の値渡し（整数パーツ / byval）・戻り（レジスタ / R15 sret、R1 にアドレス返却）。struct.ll + struct_main.c 142 チェック全通過
- [ ] LLVM 側の可変長関数定義（va_start / va_arg）
- [ ] FPU 命令（fast-math 時のみ。GCC は FPU 使用時に -ffinite-math-only を強制する）、DFPU（RXv3）

### M4 rustc 統合 ✅ 2026-10-23（詳細 `NOTES-rustc.md`）
- [x] `rustc_llvm` に RX 初期化、bootstrap は外部 LLVM（build/llvm の llvm-config）
- [x] `rustc_target`: `Arch::Rx`、ターゲット仕様 `rx-none-elf`、callconv（`callconv/rx.rs`）
- [x] atomics: `max_atomic_width=32`, `atomic_cas=false`（load/store は MOV、fence はコンパイラバリア）
- [x] `-Zbuild-std=core` で `core` がビルドでき、テスト crate を GCC とリンクして QEMU で 80 チェック全通過（release/dev）
- [ ] インライン asm、alloc の確認、tidy/UI テスト/ドキュメント（上流化する場合）

### M5 実機
- [x] リンカスクリプト・起動コード（`rx-rt` クレート）、`asm!` 対応、Rust だけのプログラムを QEMU でリセット起動（NOTES-rt.md）
- [ ] RX72N 等で L チカ
