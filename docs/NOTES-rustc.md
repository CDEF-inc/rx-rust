# M4 rustc 統合メモ（2026-10-23）

## 変更点（rust/ 配下）
| ファイル | 内容 |
|---|---|
| compiler/rustc_target/src/spec/targets/rx_none_elf.rs | ターゲット仕様（新規） |
| compiler/rustc_target/src/spec/mod.rs | `Arch::Rx`、ターゲット一覧に `rx-none-elf`、網羅 match（scalable vector / c-variadic(不安定) / object の Architecture なし） |
| compiler/rustc_target/src/callconv/rx.rs | C ABI（新規、下記） |
| compiler/rustc_target/src/callconv/mod.rs | `Arch::Rx => rx::compute_abi_info` |
| compiler/rustc_target/src/target_features.rs, asm/mod.rs | `Arch::Rx`（M4 の時点では機能一覧とインライン asm なし。どちらも後で追加、下記と NOTES-rt.md） |
| compiler/rustc_codegen_llvm/src/va_arg.rs | `Arch::Rx` は LLVM の va_arg に任せる |
| compiler/rustc_span/src/symbol.rs | `sym::rx` |
| compiler/rustc_llvm/build.rs, src/lib.rs | LLVM コンポーネント `rx` と `LLVMInitializeRX*` |
| src/bootstrap/src/core/sanity.rs | `STAGE0_MISSING_TARGETS` に `rx-none-elf` |
| bootstrap.toml | 外部 LLVM（build/llvm の llvm-config）、submodules=false、llvm-tools=false、jobs=4 |

### ターゲット仕様（rx-none-elf）
- llvm_target `rx-none-elf`、pointer_width 32、data_layout `e-m:o-p:32:32-i64:32-f64:32-n8:16:32-S32`（LLVM と一致必須）
- cpu `rxv3`（既定）、features `+64bit-doubles`（e_flags を GCC -m64bit-doubles と揃える）
- linker `rx-elf-gcc`（LLD は RX 非対応）、panic=abort、relocation-model=static、eh_frame_header なし
- アトミック: `max_atomic_width = 32`、`atomic_cas = true` → `target_has_atomic` = 8/16/32/ptr（CAS・fetch_* あり）
  - LLVM 側: 32bit 以下の atomic load/store は通常の MOV（メモリオペランドに atomic 情報が残るので巻き上げられない）、
    fence は MEMBARRIER（コンパイラバリア）。RX はシングルコア前提。
  - CAS は RXv2 以上で movli/movco ループ、RXv1 では割り込みを止めた load/compare/store（NOTES-codegen.md の「アトミック」）。
    RXv1 の方式はスーパーバイザモードでのみアトミック。`-C target-cpu=rxv1` でも CAS を使うコードはコンパイルできる。
  - `singlethread` は使わない（LLVM の ThreadModel::Single は一部ターゲットしか見ておらず、また
    LowerAtomic で通常ロードにすると割り込み待ちループが巻き上げで壊れる）

### callconv/rx.rs（GCC 互換。NOTES-codegen.md の「フロントエンドとの約束」の実装）
- 戻り値: 構造体はサイズが 4 の倍数かつ 16 以下なら `[N x i32]`（R1-R4）、それ以外は indirect（sret → R15）。整数は 32bit に拡張。
- 引数: 累積バイト数を数え、構造体はサイズが 4 の倍数で 16 バイト以内に収まるなら `[N x i32]`、それ以外は byval（自然境界）。
  スカラーは 32bit に拡張。カウンタは各引数をサイズ 4 切り上げで加算（バックエンドと同じ規則）。

### LLVM 側で M4 中に追加したもの
- RuntimeLibcalls.td の RX に `Int128RTLibcalls`（__multi3, __udivti3 …）、`CompilerRTOnlyInt64/Int128Libcalls`（__mulodi4,
  __muloti4）。i128 と checked_mul が core で使われるため。実体は compiler_builtins。
  - `__mulodi4` が無いと SMULO i64 の展開で LegalizeTypes のアサーション失敗（"Invalid type for expanded integer"）になった。
- ISD::TRAP / DEBUGTRAP を Legal にして `brk`（既定だと `abort()` 呼び出しになり、libc がないとリンクできない）。
- atomic load/store のパターン、ATOMIC_FENCE → MEMBARRIER。CAS / RMW は issue #1 で追加（CMPXCHG32）。

## ビルド手順
```
bash scripts/build-llvm-libs.sh   # build/llvm に全 LLVM ライブラリ + llvm-config（初回 ~20 分）
bash scripts/build-rustc.sh       # stage1 rustc + ホスト std（初回 ~15 分、ログ build/rustc.log）
bash scripts/run-rust-test.sh     # -Zbuild-std=core でテスト crate をビルド → GCC とリンク → QEMU
```
- LLVM を変更したら rustc_llvm を作り直させる: `touch rust/compiler/rustc_llvm/build.rs rust/compiler/rustc_llvm/src/lib.rs`
  してから build-rustc.sh（静的リンクなので）。古い core の成果物が残るので `rm -rf build/rust-test` も。
- `-Zbuild-std` は stage1 の sysroot（`lib/rustlib/src/rust` → rust/ へのシンボリックリンク）のソースを使う。
  cargo は stage0 のもの + `RUSTC_BOOTSTRAP=1`。`-Zbuild-std-features=compiler-builtins-mem` で memcpy 等を compiler_builtins から得る。
- サブモジュールは llvm-project と library/backtrace のみ取得済み（backtrace は std のビルドに必要）。

## 検証
`test/rust/core-smoke`（lib.rs + main.c）: `#![no_std]` staticlib を -Zbuild-std=core でビルドし、GCC のドライバ・libgcc
（64-bit-double multilib）と `--gc-sections` でリンク、QEMU の SDRAM（0x01000000）で実行。
- **233 チェック全通過（release / dev とも、`-C target-cpu=rxv1`）**。うち 153 は CAS / RMW（issue #1 で追加）
- 内容: i32/i64 演算、u128 の乗除算（__multi3/__udivti3）、f64/f32、構造体の値渡し（S3 byval + 後続 R2、S8 レジスタ）、
  構造体戻り値（S20 sret / S8 レジスタ）、Rust → C の構造体付き呼び出し、core::fmt による整形（u128 を含む）、
  slice::sort_unstable、AtomicU32 の load/store、checked_mul（__mulodi4）、
  AtomicU32 の compare_exchange / fetch_{add,sub,and,or,xor,nand,max,min} / swap / try_update、
  隣接する AtomicU8 ×4 の fetch_add（隣のバイトが変わらないこと）、AtomicU16 の fetch_max と compare_exchange_weak ループ
- 既定 CPU（rxv3）でビルドすると同じ crate に movli/movco ループが 14 個、rxv1 では clrpsw の列が 14 個入ることを確認
- `-C target-cpu=rxv1 -C target-feature=+fpu`（f32 が合法型になる経路）でも 233 チェック全通過（itof 10 箇所）。
  rxv3（既定で fpu あり）でも core / compiler_builtins がビルドできる。Rust の通常の f32 演算は fast-math フラグが
  付かないので libcall のまま（FPU を使うのは int→float と `core::intrinsics::fadd_fast` 等）。
- RX の target feature（rxv2 / rxv3 / rxv3e / fpu / dfpu）を `rustc_target/src/target_features.rs` に追加
  （unstable `rx_target_feature`。`rustc_feature` と `rustc_span` にも登録、`tests/ui/check-cfg/target_feature.stderr` も更新）。
  `-C target-feature=±fpu` の警告が出なくなり、`cfg(target_feature = "dfpu")` などで分岐できる（rx-rt が使う）。
  `--print cfg`: rxv1 → なし、rxv1 + fpu → fpu、rxv2 → fpu / rxv2、rxv3 → dfpu / fpu / rxv2 / rxv3。
  rustc_span のシンボルが変わると古い rustc でビルドした core のメタデータが合わず ICE になるので、`build/rust-*` を消す。
- サイズ: text 18 KB（release, opt-level=s, gc-sections）。gc-sections なしだと 277 KB で内蔵 RAM に入らなかった。
- 既存の LLVM レベルのテスト（i32 4131 / m3 3957 / struct 142）も全通過。

## 注意・未対応
- `scripts/run-rust-test.sh` は、`RX_CPU` が無ければ `-C target-cpu=rxv1` でビルドして QEMU の rx62n で走らせる
  （qemu-demo も .cargo/config.toml で rxv1 を指定）。`RX_CPU=rxv2|rxv3|rxv3e` なら、その CPU 向けにビルドして
  CDEF-inc/qemu の同じコアで走らせる（`scripts/cpu.sh`）。`scripts/run-all-tests.sh` は 4 つのコアすべてで実行し、
  core-smoke は実機の RX72M（`-C target-cpu=rxv3`）でも通る（NOTES-hw.md）。
- インライン asm（`asm!`）は M5 で対応（NOTES-rt.md）。alloc は未試験（rx-rust #52）、std は対象外。
- tidy・UI テスト・platform-support のドキュメントは未対応（上流化する場合に必要、rx-rust #53）。
