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
- linker `rust-lld`（`Gnu(Cc::No, Lld::Yes)`。LLVM フォークの lld の RX 対応、NOTES-lld.md。`-C linker=rx-elf-ld` で GNU ld）、panic=abort、relocation-model=static、eh_frame_header なし
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
  （2026-11-03: `-C target-feature=+flush-denormals` で通常の演算も FPU / DFPU を使う。下記「FPU / DFPU」）
- RX の target feature（rxv2 / rxv3 / rxv3e / fpu / dfpu）を `rustc_target/src/target_features.rs` に追加
  （unstable `rx_target_feature`。`rustc_feature` と `rustc_span` にも登録、`tests/ui/check-cfg/target_feature.stderr` も更新）。
  `-C target-feature=±fpu` の警告が出なくなり、`cfg(target_feature = "dfpu")` などで分岐できる（rx-rt が使う）。
  `--print cfg`: rxv1 → なし、rxv1 + fpu → fpu、rxv2 → fpu / rxv2、rxv3 → dfpu / fpu / rxv2 / rxv3。
  rustc_span のシンボルが変わると古い rustc でビルドした core のメタデータが合わず ICE になるので、`build/rust-*` を消す。
- サイズ: text 18 KB（release, opt-level=s, gc-sections）。gc-sections なしだと 277 KB で内蔵 RAM に入らなかった。
- 既存の LLVM レベルのテスト（i32 4131 / m3 3957 / struct 142）も全通過。

## FPU / DFPU（2026-11-03、rx-rust #70）
- target feature `flush-denormals`（unstable `rx_target_feature`。`target_features.rs` と
  `tests/ui/check-cfg/target_feature.stderr` に追加）: `-C target-feature=+flush-denormals` で、Rust の通常の f32 演算・
  比較・`as` 変換が FPU、rxv3 / rxv3e では f64 も DFPU を使う（LLVM の同名のサブターゲット機能。NOTES-codegen.md の
  「IEEE の FPU / DFPU」）。既定は無効で、そのときは従来どおり compiler-builtins のソフトフロート（完全に IEEE）。
- 有効にしたときの意味: NaN・無限大・符号付きゼロ・丸めは IEEE どおり（`float-smoke` が compiler-builtins とビットで比較）。
  **非正規化数だけは 0 になる**（入力も結果も、同符号の 0。FPSW.DN / DPSW.DDN = 1 の動作）。DN / DDN = 0 だと未実装処理
  例外になるので、rx-rt を使わない場合は FPSW / DPSW を 0x100 にしておく。漸近アンダーフローが要るコードでは有効にしない。
- NaN の結果のビット列（符号・ペイロード）は IEEE でも Rust でも決まっていない。RX の FPU は NaN を入力にしない無効演算で
  7FFFFFFFh（f64 は 7FFFFFFFFFFFFFFFh）を返す。
- `cfg(target_feature = "flush-denormals")` で分岐できる。
- `test/rust/float-smoke`: 3,988 チェック、QEMU の 4 コア × release / dev × 有無、実機 RX72M（release / dev × 有無）で全通過。
  `scripts/run-rust-rom.sh` に `RUST_EXTRA_FLAGS`（RX_CPU と一緒に使う追加の rustc フラグ。別の target ディレクトリ）を足した。
- 速度: ベンチマーク 1 周がソフトフロート約 0.82 秒 → FPU / DFPU 約 0.029 秒（実機、LOCO クロック）。

## alloc（2026-11-02、rx-rust #52）
- `-Zbuild-std=core,alloc`（.cargo/config.toml の `build-std = ["core", "alloc"]`）で rxv1〜rxv3e のどれでもビルドできる。
  RX 向けの変更は要らなかった。std は対象外。
- `test/rust/alloc-smoke`: グローバルアロケータは embedded-alloc 0.6 の `LlffHeap`（linked_list_allocator）を 16 KB の
  static 配列に置く。Box（u64、16 バイト揃えの型、スライス、`dyn Fn`）、Vec（1000 要素の push / sort / retain /
  shrink、入れ子）、String / format!（整数・16 進・Debug・幅指定・f64 / f32）、BTreeMap（200 個の挿入・削除・range）、
  VecDeque、BinaryHeap、Rc、Arc を 3 周まわし、毎周 `HEAP.used() == 0`（すべて解放された）を確かめる。115 チェック。
  QEMU の rx62n / rxv2 / rxv3 / rxv3e の release / dev と、実機の RX72M（rxv3、release / dev）で全通過。
- アラインメント: u64 / f64 は 4 バイト境界（GCC と同じ、DataLayout の `i64:32-f64:32`）。それより大きい
  `#[repr(align(16))]` などはアロケータが Layout どおりに揃える（16 バイト境界になることを確かめた）。
- アトミック: Arc の参照カウントは AtomicUsize の CAS（RXv2 以降は movli / movco のループ、RXv1 は割り込みを止めた
  列。NOTES-codegen.md）。embedded-alloc は `critical-section` で排他するので、rx-rt の `critical-section` feature が要る。
- **リンク順の問題（rx-rt で修正）**: critical-section の実装（`__critical_section_1_0_acquire` / `_release`）は
  rx-rt にあり、使うのが rx-rt より後ろにリンクされる crate（embedded-alloc）だけだと、ld が rx-rt のアーカイブを
  見た時点ではまだ未定義参照がないので、実装のオブジェクトが取り込まれず未定義になる。dev（オブジェクトが細かく
  分かれる）で起き、release では他のコードと同じオブジェクトに入っていたので起きなかった。rx-rt の build.rs が
  `critical-section` feature のとき link.x に `EXTERN(__critical_section_1_0_acquire)` / `release` を足して
  常に取り込むようにした（RX のシンボルは先頭に `_` が付くので、Rust の `_critical_section_1_0_acquire` は `__` になる）。
- サイズ（rx62n、release、opt-level = "s"）: .text 39,806 バイト、.rodata 16,372 バイト（dev は 72,502 / 61,788）。
  大きいのは書式化と浮動小数点の文字列化（約 15.7 KB。f64 / f32 の `{}` / `{:.3}` を使っているため）、BTreeMap
  （約 5.9 KB）、u32 の sort（2.1 KB）、`to_uppercase`（Unicode 表を含む）。アロケータ自体は約 1.3 KB。

## 可変長関数（2026-11-09）
- Rust で定義する可変長関数（`unsafe extern "C" fn f(n: c_int, mut ap: ...)`）、`VaList` の `next_arg` / `clone`
  （va_copy）、C の `va_list` を受け取る Rust の関数、Rust から C の可変長関数を呼ぶことが、RX 向けの変更なしで動く。
  - rustc は RX の `va_arg` を LLVM の既定の展開に任せる（rustc_codegen_llvm の va_arg.rs）。
  - `VaList` は次の名前なし引数を指すポインタ（core の既定の `VaListInner`）で、GCC の `va_list` と同じ。
  - 引数の置き場所（最後の名前付き引数と名前なしの引数はスタック）は LLVM バックエンドが決める（NOTES-codegen.md の
    「フレームポインタ・動的 alloca・可変長関数」）。
- 可変長関数の定義には `#![feature(c_variadic_experimental_arch)]` が要る（rustc_target の
  `supports_c_variadic_definitions` で RX は Unstable）。呼ぶ側と `VaList` は安定機能。
- `next_arg` で読める型は core の `VaArgSafe`（i32 / u32 / i64 / u64 / isize / usize / f64 / ポインタ）。
  i8 / i16 / f32 は C の既定の昇格で int / double になるので読めない（GCC と同じ）。構造体も読めない。
- `test/rust/variadic-smoke`（lib.rs + main.c）: 文字列の書式で型を指定し、Rust が読んだ値を GCC の `va_arg` が
  読んだ値と比べる。
  - 読む側は 4 通り: 名前付き 2 個、名前付き 6 個（R1〜R4 からはみ出す）、va_copy で 2 回読む、C の `va_list` を
    受け取る関数。
  - 呼ぶ側では、int / long / long long / f64 / size_t / ptrdiff_t / ポインタの混在、名前付き 1 個の和、`VaList` を
    C の関数に渡す。
  - Rust から GCC の可変長関数を呼び、i64・f64・ポインタ・構造体（S8 と 3 バイトの S3）を渡す。
  - 379 チェック。QEMU の rx62n / rxv2 / rxv3 / rxv3e の release / dev と、rxv3 の `+fpu,+dfpu` で全通過。
  - i64 を 2 つの int として逆順に読む版と、va_copy せずに元を 2 回読む版は失敗する（テストが間違いを見つけることの確認）。
- `scripts/run-rust-test.sh` は、共有の target ディレクトリにある最初の `lib*.a` をリンクしていた。
  そのため、core-smoke の後では別のクレートでも core-smoke を走らせていた。いまは Cargo.toml のクレート名で選ぶ。

## coretests（2026-11-11）
`library/coretests`（core のテスト、約 2,800 件）を rx-none-elf でビルドし、QEMU で走らせる
（`scripts/run-coretests.sh`、`test/rust/coretests`）。coretests のソースには手を入れない。
- rx-none-elf には std も libtest も無いので、どちらも小さな代わりのクレートを使う（Cargo で `std` / `test` という名前の依存にする）。
  - `std/`: core と alloc を std と同じ形で再エクスポートし、出力は QEMU の SCI0。rustc は std の prelude を入れるだけで
    `#[macro_use]` はしないので、`println!` などのマクロも prelude に置く。std にしか無い型（`LazyLock`、`thread_local!`、
    `RandomState`、`HashSet` を `BTreeSet` で、`catch_unwind` は閉包を呼ぶだけ）は最小限の代わり。
    std にしか無い f16 / f32 / f64 のメソッド（`floor`、`powf`、`gamma` など）は std と同じ中身をトレイトにして prelude に置く
    （定数式で使われる `floor` などは const trait）。C の数学関数は compiler_builtins の libm。
  - `test/`: rustc のテストハーネスが作る `TestDescAndFn` などの型と、呼ばれる `test_main_env_args` を持つ。テストを 1 本ずつ
    呼び、結果を libtest の形式で出す。パニックは巻き戻せないので、パニックハンドラがそのテストの結果を記録し
    （`#[should_panic]` なら成功）、スタックを `__stack` に戻して次のテストから再開する（確保したメモリは捨てる）。
    起動コード、グローバルアロケータ（2 のべき乗の大きさごとの空きリスト、8 MB）もここ。
  - `features/`: coretests が有効にしている std だけの feature（`float_gamma`）を `staged_api` で宣言するだけのクレート。
    無いと「未知の feature」でコンパイルエラーになる。
- プログラムは約 1.9 MB なので 512 KB の ROM には入らない。QEMU の SDRAM（16 MB）に ELF として読み込む（`test/coretests.x`）。
- 結果（rx62n / rxv2 / rxv3 / rxv3e の release）: 2815 件すべて通過、5 件は coretests 自身が `panic = "abort"` で無視。
  QEMU で約 70 秒。
  - 当初は `atomic::ptr_bitops_tagging` が失敗した。RX バックエンドにスタックの再整列が無く、`#[repr(align(16))]` の
    ローカル変数が 4 バイト整列にしかならないのに、LLVM はその番地の下位ビットを 0 として畳み込んでいた。
    llvm-project#37 でプロローグが SP を揃え直すようにして通った（NOTES-codegen.md「スタックの再整列」）。
  - 失敗してよいテストは `test/rust/coretests/expected-failures` に書き、それ以外の失敗と、書いてあるのに通ったテスト（XPASS）を
    エラーにする。
- 実機（EK-RX74M）でも全件通る（NOTES-hw.md の「coretests」）。コード MRAM に入らないので `num` と残りの 2 本に分ける。
  `PART=+num` / `PART=-num` で QEMU でも同じ分け方で走らせられる。ヒープはリンカスクリプトが決める
  （`coretests_heap_start` / `_end`。使うのは約 470 KB）。

## 注意・未対応
- `scripts/run-rust-test.sh` は、`RX_CPU` が無ければ `-C target-cpu=rxv1` でビルドして QEMU の rx62n で走らせる
  （qemu-demo も .cargo/config.toml で rxv1 を指定）。`RX_CPU=rxv2|rxv3|rxv3e` なら、その CPU 向けにビルドして
  CDEF-inc/qemu の同じコアで走らせる（`scripts/cpu.sh`）。`scripts/run-all-tests.sh` は 4 つのコアすべてで実行し、
  core-smoke は実機の RX72M（`-C target-cpu=rxv3`）でも通る（NOTES-hw.md）。
- インライン asm（`asm!`）は M5 で対応（NOTES-rt.md）。alloc は上記。std は対象外。
- tidy・UI テスト・platform-support のドキュメントは未対応（上流化する場合に必要、rx-rust #53）。
