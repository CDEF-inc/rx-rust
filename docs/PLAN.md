# RX (RXv3) 向け Rust ツールチェイン — LLVM RX バックエンド自作計画

## 前提
- upstream LLVM / Rust に RX ターゲットは無い。Renesas/OSTS の LLVM も RL78・RISC-V のみ（RX は GCC のみ）。
- 方針: rustc 同梱の LLVM フォーク (`rust/src/llvm-project`) に `lib/Target/RX` を新規実装し、rustc に `rx-none-elf` ターゲットを追加する。
- 対象コア: RXv3（RX66N/RX72N 等）と RXv3e（RX74M/RX74N）、リトルエンディアン、倍精度 FPU あり。
  RXv3e のコード生成は RXv3 + DFPU と同一（`NOTES-rxv3e.md`）。
- ABI は GNURX（Renesas 版 GCC、`-misa=v3 -mdfpu -m64bit-doubles`）と互換にし、libgcc / newlib / Renesas FIT(C) とリンク可能にする。
  上流 GCC 14.2 は `-mcpu=rx100|rx200|rx600|rx610` のみで RXv2/v3/DFPU を知らない。ABI の基本（引数・アラインメント）は共通。
- C のシンボルには `_` が前置される（GCC rx: `USER_LABEL_PREFIX "_"`）。LLVM の DataLayout は `m:o` にして
  C シンボルに `_` を付ける（M2、NOTES-codegen.md の「シンボル名」）。
- リポジトリ: https://github.com/CDEF-inc/rx-rust（作業場所: WSL Ubuntu `/work/rx_rust_toolchain/rx-rust`）

## 残っていること（2026-11-01 時点）
各マイルストーンの `[ ]` のまとめ。2026-10-31 に llc とテストで確かめた。issue #1（RXv2 / RXv3 対応）は
2026-11-01 に閉じ、残りはそれぞれ別の issue にした。

| 項目 | いまの動き | 影響 | issue |
|---|---|---|---|
| フレームポインタ | 使わない（`-frame-pointer=all` も効かない） | 動的 alloca の前提 | #51 |
| 動的 alloca | llc が `RX: dynamic stack allocation is not supported yet` で止まる | Rust では普通は出ない | #51 |
| 可変長関数の定義（va_start / va_arg） | llc が `Cannot select: vastart` で止まる。呼ぶ側（printf など）は動く | Rust では C 用の `...` 関数の定義だけ | #51 |
| DFPU: dtoi / dtof / ftod、f64 の DR への常駐 | libcall、演算ごとに GPR を経由 | 倍精度の速度 | #49 |
| FPU のメモリオペランド、RXv2 の 3 オペランド形式 | レジスタ形式だけを選ぶ | 単精度の命令数 | #50 |
| `alloc` | 未試験 | ヒープを使う crate | #52 |
| tidy・UI テスト・platform-support ドキュメント | 未対応 | 上流化する場合だけ必要 | #53 |
| 周辺機能（UART 受信、SPI / I2C / PWM / ADC） | rx72m-hal に未実装 | NOTES-hal.md の「予定」 | #54 |

## ディレクトリ
- `rust/` — サブモジュール CDEF-inc/rust `rx-none-elf`（rust-lang/rust `7189d39d9ce` + RX）。
  その中の `src/llvm-project` は CDEF-inc/llvm-project `rx/23.1`（LLVM 23.1.4 `f738f3b0` + RX バックエンド）
- `crates/rx-rt/` — 起動コード・ベクタ表・リンカスクリプト（M5）
- `crates/rx72m-pac/`、`crates/rx72m-hal/` — RX72M の PAC（ビルド時に生成）と embedded-hal 実装（NOTES-hal.md）
- `examples/blinky/` — AP-RX72M-0A の LD1 / LD2 点滅
- `scripts/`, `test/` — ビルド・検証スクリプトとテスト（`test/hw` は実機用）、`docs/` — このファイルと NOTES-*.md
- `gnu/install/`（`scripts/build-gnu.sh`、リポジトリ外）— 参照用 rx-elf binutils 2.43 / 上流 GCC 14.2 + libgcc（エンコーディング検証・ABI 比較・リンカ）。
  binutils には `scripts/patches/` を当てる（ld が e_flags の V2 / V3 を落とす不具合の修正）
- `../qemu/` — CDEF-inc/qemu `rx/v11.1`（RXv2 / RXv3 / RXv3e の CPU モデル、NOTES-qemu.md）
- GNURX 14.2 のソース（RXv2/v3/DFPU の ABI 参照）とコンパイラは、最初の作業環境の `~/work/gnurx/` と `~/opt/gnurx-14.2` にあった。
  いまの `/work` には無い
- `build/llvm/` — LLVM ビルド（Release + assertions、X86 + RX、ccache）
- `build/dist/` — `scripts/dist-toolchain.sh` が作る配布用ツールチェイン（CI 用、README の「Prebuilt toolchain」）
- `ref/` — RXv3e ISA マニュアル（PDF とテキスト版）。Renesas の著作物なのでコミットしない（.gitignore）

## マイルストーン
### M0 環境 ✅ 2026-10-23
- [x] rust / llvm-project 取得、LLVM 単体を `-DLLVM_EXPERIMENTAL_TARGETS_TO_BUILD=RX` でビルドできる骨組み
  - Triple に `rx` 追加（Triple.h/.cpp, TargetDataLayout.cpp, ELFObjectFile.h）
  - `lib/Target/RX`: TargetInfo, MCTargetDesc（AsmInfo/InstPrinter）, 空の TargetMachine, TableGen（レジスタ, brk/rts/nop）
  - CPU: generic/rxv1/rxv2/rxv3/rxv3e、機能: rxv2/rxv3/rxv3e/dfpu
  - 確認: `llc --version` に `rx - Renesas RX`、`llc -mtriple=rx-none-elf -mcpu=help`
- [x] 参照 rx-elf-gcc / objdump（`gnu/install/bin`）
- [x] binutils 2.43 の ld が、入力の e_flags が 1 ビットでも違うと出力から V2 / V3 を落とす不具合を、パッチで直した（rx-rust #42）
- 実測（rx-elf-gcc -mcpu=rx600 -m64bit-doubles）: i64/f64/long double は size 8・align 4、char は unsigned、BIGGEST_ALIGNMENT=4
  → DataLayout `e-m:o-p:32:32-i64:32-f64:32-n8:16:32-S32`（M2 で `m:o` に変更し C シンボルに `_` を付与）
- GNURX 由来の事実: DWARF 番号 R0-15=0-15, PSW=16, PC=17, DRn=32+n。`-mdfpu` でも f64 引数/戻り値は整数レジスタ。DR0-15 は全て caller-saved
- ビルド手順: `scripts/build-llvm.sh`（GCC は `scripts/build-gnu.sh`。`cmake -G Ninja ... -DLLVM_PARALLEL_COMPILE_JOBS=6 -DLLVM_PARALLEL_LINK_JOBS=1`）。
  スクリプトの並列度は WSL のメモリが 7GB だったころの値。いまのマシン（28 コア・31GB）なら上げてよい。GCC ビルドと同時に走らせない

### M1 MC 層（アセンブラ・オブジェクト出力）✅（詳細 `NOTES-mc.md`）
- [x] `RXRegisterInfo.td`（R0=SP〜R15, PSW, FPSW, ACC0/1, DR0〜DR15、GPRLow=R0〜R7）
- [x] `RXInstrFormats.td` / `RXInstrInfo.td`（可変長 1〜8 バイト命令）— 整数・シフト・スタック・分岐
- [x] AsmParser / InstPrinter / MCCodeEmitter / AsmBackend / ELFObjectWriter（EM_RX=173, R_RX_*）/ e_flags
- [x] 検証: `scripts/mc-diff.sh` で GNU as と .text・e_flags・リロケーションを比較（basic 210/210、branch、symbol 一致）
- [x] 単精度 FPU、DSP（A0/A1）、stz/stnz、RXv2 の追加命令（fsqrt/ftou/utof、FPU 3 オペランド、movli/movco、DSP 拡張）。
  fpu 55/55、dsp 51/51、v2 19/19 一致（issue #1）
- [x] RXv3 の追加命令（bfmov/bfmovz、save/rstr）と DFPU 全命令（DR / DRH / DRL / DCR レジスタ）。
  dfpu 57/57、v3 17/17 一致（issue #1）
- [x] MOV の残りのアドレッシング形式と string 命令（llvm-project #9、`test/mc/mov.s`、`string.s`）
- [x] memex（.B / .W / .UW / .L のメモリオペランド）と 06 プレフィクス形式（itof/utof のメモリ形式を含む）、
  メモリからの push、ビット操作（bset/bclr/bnot/btst/bm*cnd）、xchg（llvm-project #15、`test/mc/memex.s`）
- [x] `sc<cond>.l`（M2 で追加、gas と 14/14 一致）
- [x] サイズ指定なしの分岐と、シンボルの差（`sym1 - sym2`）を GNU as と同じに受け付ける（llvm-project #14、`test/mc/sizeless.s`）。
  codegen はサイズ付き分岐 + BranchRelaxation（M2）
- [x] RXv3e の Protect Zone 命令（trcall/trmvfc/trmvtc/trrte）と mvfc/mvtc の追加レジスタ。binutils 未対応なのでマニュアルの符号表で検証
- [x] 逆アセンブラ（TableGen の表、`llvm-objdump` / `llvm-mc --disassemble`）。test/mc の全行が往復で一致、
  ランダムなバイト列で GNU objdump と命令長・命令名が一致（`scripts/mc-dis.sh`、`scripts/dis-fuzz.py`）。
  objdump は分岐先にシンボル名を付ける（llvm-project #16）

### M2 コード生成（i32 整数のみ）✅ 2026-10-23（詳細 `NOTES-codegen.md`）
- [x] CallingConv.td（GNURX 互換。引数 R1〜R4、戻り値 R1、callee-saved R6〜R13、char/short は int に昇格）
- [x] FrameLowering（SP 相対、FP なし）、ISelDAGToDAG / ISelLowering、分岐・比較・ロード/ストア
- [x] 選択は最長形式、出力時に最短符号へ縮小（RXMCInstLower の shrinkRXInst）
- [x] 検証: `scripts/run-codegen-test.sh` — llc → GCC ドライバ + libgcc とリンク → QEMU（gdbsim-r5f562n8, RXv1）で実行。
  4131 チェック全通過（-O0〜-O3、.s 経由・.o 直接とも）
- [x] メモリアクセスの形式: 定数のストア（`mov.sz #imm, dsp[Rd]`）、レジスタ間接 + インデックス（`[Ri, Rb]`）、
  ポストインクリメント / プリデクリメント、memcpy / memset を smovf / sstr に（llvm-project #10〜#13）。
  命令列を `mem.expected` と照合し（`scripts/check-mem.sh`）、mem.ll 78 チェックを実行
- [x] RXv2/v3 命令を使う codegen の実行検証: CDEF-inc/qemu の rxv2 / rxv3 / rxv3e コアで全テスト（`scripts/run-all-tests.sh`、NOTES-qemu.md）。
  実機（AP-RX72M-0A）でも codegen テストが -O0 / -O2 で通過（M5）
- [x] プロローグ・エピローグ: callee-saved を 1 つの範囲にして pushm（1 本なら push.l）で保存し、rtsd で解放・復帰・戻り
  （2026-11-01、`test/codegen/frame.ll`、`scripts/check-frame.sh`、NOTES-codegen.md）
- [x] CFI: デバッグ情報があれば .debug_frame（uwtable なら .eh_frame）にプロローグを記述し、gdb がスタックをたどれる
  （2026-11-01、`scripts/check-frame.sh`、NOTES-codegen.md の「CFI」）
- [ ] フレームポインタと動的 alloca

### M3 拡張 — i64 / float / double は完了（詳細 `NOTES-codegen.md` の M3 節）
- [x] 引数配置を GCC の規則（累積バイト数、溢れたら以降すべてスタック、可変長は最後の名前付きからスタック）で実装
- [x] i64（add/adc, sub/sbb、乗除算・シフトは libgcc / 展開）
- [x] float / double はソフトフロート（libgcc soft-fp、-m64bit-doubles multilib）。RuntimeLibcalls.td に RX を登録
- [x] GCC との相互呼び出しテスト（m3.ll + m3_main.c、3957 チェック全通過、-O0〜-O3）
- [x] 構造体の値渡し（整数パーツ / byval）・戻り（レジスタ / R15 sret、R1 にアドレス返却）。struct.ll + struct_main.c 142 チェック全通過
- [ ] LLVM 側の可変長関数定義（va_start / va_arg）。可変長関数を呼ぶ側は上の規則で動く
- [x] 単精度 FPU（`+fpu`、rxv2 以上の既定）: nnan+ninf かつ非正規化数環境が preserve-sign の演算だけ
  fadd/fsub/fmul/fdiv/fsqrt/fcmp、int→float は itof/utof、それ以外はソフトフロート（issue #1）。
  fpu.ll 3110 チェック、core-smoke も `+fpu` で通過
- [x] DFPU（RXv3）による f64 演算: nnan+ninf かつ非正規化数環境が preserve-sign の四則・平方根・比較と
  int→double を DR0/DR1 経由の列に（issue #1）。命令列を dfpu.expected と照合し、CDEF-inc/qemu の rxv3 で dfpu_main.c と実行（4000 チェック）

### M4 rustc 統合 ✅ 2026-10-23（詳細 `NOTES-rustc.md`）
- [x] `rustc_llvm` に RX 初期化、bootstrap は外部 LLVM（build/llvm の llvm-config）
- [x] `rustc_target`: `Arch::Rx`、ターゲット仕様 `rx-none-elf`、callconv（`callconv/rx.rs`）
- [x] atomics: `max_atomic_width=32`（load/store は MOV、fence はコンパイラバリア）
- [x] CAS / RMW: `atomic_cas=true`。RXv2+ は movli/movco ループ、RXv1 は割り込みを止めた列（CMPXCHG32、issue #1）。
  atomic.ll 1080 チェック、core-smoke の CAS / fetch_* も QEMU（RXv1）で通過。RXv2 経路も CDEF-inc/qemu の rxv2 以降で実行
- [x] `-Zbuild-std=core` で `core` がビルドでき、テスト crate を GCC とリンクして QEMU で 80 チェック全通過（release/dev）
- [x] RX の target feature（rxv2 / rxv3 / rxv3e / fpu / dfpu、rust #4）とインライン asm（`asm!`、M5、`test/codegen/inline-asm.ll`）
- [x] 配布用ツールチェイン（`scripts/dist-toolchain.sh`、rx-rust #40）。CI の RX ジョブで使う
- [ ] `alloc` の確認
- [ ] tidy / UI テスト / platform-support のドキュメント（上流化する場合）

### M5 実機 ✅ 2026-10-28（詳細 `NOTES-rt.md`、`NOTES-hw.md`、`NOTES-hal.md`）
- [x] リンカスクリプト・起動コード（`rx-rt` クレート）、`asm!` 対応、Rust だけのプログラムを QEMU でリセット起動（NOTES-rt.md）
- [x] 割り込み入口で FPSW（fpu）、DR0/DR1/DPSW/DCMR（dfpu）、ACC（`save-acc`）を退避、起動時に FPSW/DPSW = 0x100。
  rustc に RX の target feature を追加して cfg で切り替え（issue #1）。isr-save 19 チェック通過（rxv1+fpu）
- [x] AP-RX72M-0A（RX72M、RXv3 + DFPU）を E2 Lite で動かすテスト環境（`scripts/hw-build.sh`、`scripts/hw-run.sh`）。
  命令・codegen・core-smoke・割り込みの 31 本、35,560 チェックがすべて実機で通過（NOTES-hw.md）。
  実機と QEMU の違いは CDEF-inc/qemu で直した（NOTES-qemu.md）
- [x] rx72m-pac と rx72m-hal: クロック（PLL 240 MHz）、GPIO、MPC、CMT の遅延、SCI2 の UART 送信、rx-rt の critical-section
- [x] L チカ: `examples/blinky`（embedded-hal で LD1 / LD2 を点滅、コードフラッシュから起動）
- [x] Rust で書いた RTOS が QEMU と RX72M で動く
- [ ] UART の受信、SPI / I2C / PWM / ADC（NOTES-hal.md の「予定」）
