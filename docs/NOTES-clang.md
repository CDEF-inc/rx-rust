# clang の RX 対応（2026-11-12）

LLVM Discourse で「バックエンドは Rust 以外のフロントエンド、少なくとも clang でも使えるべき」との指摘を受け、clang が
`--target=rx-none-elf` で C をコンパイルできるようにした（CDEF-inc/llvm-project#41、`rx/clang`）。ABI・型・定義済みマクロは
GNURX 14.2（Windows の `C:\ProgramData\GCC for Renesas RX 14.2.0.202607-GNURX-ELF`）と上流 GCC 14.2（gnu/install、既定の
`-mrx-abi`）の `-m64bit-doubles` と同じで、どちらのオブジェクトとも双方向に呼び合える。

## 使い方
```
clang --target=rx-none-elf -mcpu=rxv3 -O2 -c foo.c
clang --target=rx-none-elf --gcc-toolchain=gnu/install -T my.ld foo.o bar_gcc.o -o foo.elf
```
- 既定は GCC と同じ `-mcpu=rx600`（RXv1 + 単精度 FPU）、`-m64bit-doubles`、`char` は unsigned、最適化時はフレームポインタなし。
- `-mcpu=` はバックエンドの rxv1 / rxv2 / rxv3 / rxv3e と GNURX の rx100 / rx13t / rx140 / rx200 / rx230 / rx600 / rx610 /
  rx64m / rx66t / rx71m / rx72t（ISA と FPU は GNURX と同じ。DFPU はどれも `-mdfpu` のときだけ）。`-mcpu=` が無ければ GNURX の
  `-misa=v1|v2|v3|v3e` も使え、GNURX と同じく FPU あり・DFPU は `-mdfpu` のときだけ（バックエンドの rxv1 は FPU なし、rxv3 は
  DFPU あり）。`-mdfpu` は RXv3 より前の CPU と `-m32bit-doubles` ではエラー（GNURX と同じ）。ほかに `-fpu` / `-mnofpu` /
  `-nofpu`、`-mdfpu` / `-mno-dfpu`、`-m64bit-doubles` /
  `-m32bit-doubles`（double と long double が float になる。バックエンドの `64bit-doubles` 機能を外し、e_flags も GCC と同じ）、
  `-mrelax`。
- FPU / DFPU の命令は IEEE の結果が変わらないときだけ使う（NOTES-codegen.md の「IEEE の FPU / DFPU」）。GCC の `-fpu` のように
  非正規化数を 0 にしてよければ `-fdenormal-fp-math=preserve-sign`。
- ドライバ（clang の `ToolChains/RX.cpp`）: rx-elf の GCC（clang の隣、または `--gcc-toolchain`）から libgcc、crtbegin.o /
  crtend.o、newlib を探し、GCC のマルチライブ 64-bit-double / no-fpu-libs を選ぶ。リンクは ld.lld（`-fuse-ld=bfd` で GCC の
  GNU ld）、GCC の specs と同じ並び（crt0.o、`-lgcc --start-group -lc -lnosys --end-group -lgcc`、`-msim` で `-lsim`、`-T` が
  無ければ libgloss の rx.ld / rx-sim.ld）。アセンブラは統合アセンブラ（`-fno-integrated-as` なら GNU as に `-mcpu` と
  `-m64bit-doubles` を渡す）。GCC が無ければ compiler-rt の builtins をリンクする（RX 用はまだ無い）。

## GCC と同じにしたこと（実測。clang/test/CodeGen/RX、Preprocessor/init-rx.c）
- 定義済みマクロ: GNURX の `-dM -E` と、CPU と `-misa` の 14 通りで RX のマクロが全部一致（`__RX__`、`__RX600__` / `__RX64M__`
  など `-mcpu` の名前、`__RXv1__` / `__RXv2__` / `__RXv3__`（RXv3e も v3）、`__RX_LITTLE_ENDIAN__`、`__RX_64BIT_DOUBLES__` /
  `__RX_32BIT_DOUBLES__`、`__RX_FPU_INSNS__`、`__RX_DFPU_INSNS__`、`__RX_GAS_SYNTAX__`、`__RX_ABI__`、
  `__RX_ALLOW_STRING_INSNS__`）。`-misa=` だけなら CPU 名のマクロは無い。以前の `RXTargetInfo`（LLDB 用）は `__RX_GCC_ABI__` を
  定義していたが、これは `-mgcc-abi`（スタック上の引数を 4 境界にする古い ABI）のマクロで、バックエンドの ABI とは違う。
  - 上流 GCC 14.2 は `__RXv1__` を定義しない（GNURX の追加）。それ以外は上流とも一致。
  - GNURX の .exe は WSL から動かない（NOTES の環境メモ）ので、Windows 側の Git Bash で `-dM` を取って比べた。
- 型: `-dM` の `_TYPE__` は GCC と全部一致。int32_t / int_least32_t は long、int_fast8/16/32_t は int、char32_t は
  unsigned long（newlib の型）。clang は int_fastN_t を int_leastN_t と同じにしていたので、TargetInfo に
  `getFastIntTypeByWidth` を足した。
  - clang 自身の stdint.h（`-ffreestanding` や newlib が無いとき）も、RX ではこれらのマクロで int_fastN_t とその上限・幅を
    定義する（以前は int_leastN_t と同じにしていて、int_fast8/16_t が 1 / 2 バイトだった。#41 のレビューで指摘。
    clang/test/Headers/rx-stdint.c）。
- アラインメント: long long / double は 4。`__alignof__` も 4（clang は既定で 8 にする「優先アラインメント」を持つので
  `allowsLargerPreferedTypeAlignment` を false に）。
- ビットフィールド: GCC は RX で MS のレイアウト（`TARGET_MS_BITFIELD_LAYOUT_P`。型の違うビットフィールドは新しい
  記憶単位から）を使う。`__attribute__((packed))` の構造体では使わない。clang の ms_struct は基本型のフィールドと記憶単位を
  型の大きさに揃える（i386 の真似）が、GCC の RX は型のアラインメント（long long でも 4）のままなので、TargetInfo の
  `msStructAlignsFieldsToTypeSize` で切る。`-mno-ms-bitfields` で通常のレイアウト。
  - 例: `struct { char a; int b : 3; }` は 8 バイト、`struct { char c; long long x : 3; }` は 12 バイト（アラインメント 4）。

## 呼び出し規約（clang の `CodeGen/Targets/RX.cpp`）
GCC の `rx_function_arg` などに合わせた。構造体の扱いは rustc の callconv/rx.rs と同じ。
- 引数は R1〜R4 に累積バイト数で詰め、各引数は 4 に切り上げて数える。収まらない引数はスタックへ、カウンタはどの引数でも進む。
- 構造体: サイズが 4 の倍数でレジスタに収まるなら `[N x i32]`（4 バイトなら i32）、それ以外とスタックに行くものは byval
  （型の自然境界。char だけの構造体は 1 境界で詰まる）。空の構造体は場所を取らない。
- 戻り値: サイズ 1〜16 で 4 の倍数の構造体は R1〜R4、それ以外（空の構造体も）は sret（R15。引数に数えない）。
- `_Complex` は GCC では集約型ではない: 収まればそのモードのままレジスタ（`_Complex char` は R1 の下位 16 ビット）、
  戻り値はサイズによらずレジスタ。
- **char / short / _Bool の引数は拡張しない**（GCC は呼ぶ側で拡張せず、呼ばれた側が `movu.b r1, r1` などで拡張する。
  `rx_promote_function_mode` は戻り値だけ int にする）。clang は引数に zeroext / signext を付けず、_Bool は i8。
  戻り値には付ける（呼ばれた側が拡張する）。
- **スタック上の char / short は自分の大きさと境界**: `f(int, int, int, int, char e, char f, short h, int i)` は e が +0、f が +1、
  h が +2、i が +4。バックエンドはどれも 4 バイトにしていたので直した（RXISelLowering の analyzeRXArguments。
  llvm/test/CodeGen/RX/stack-small-args.ll）。rustc の u8 / u16 のスタック引数もこれで GCC と同じ置き方になる。
- 可変長: 最後の名前付き引数と名前なしの引数はスタック（バックエンドが決める）。名前なしの構造体は byval なので、3 バイトの
  構造体の次の char[4] の構造体は +3 から詰まる（GCC と同じ）。va_arg は型の境界に揃えてから型の大きさだけ進める
  （GCC の PARM_BOUNDARY は 8 ビットなので 4 には切り上げない）。

## テスト
- llvm-project: clang/test/CodeGen/RX（abi.c、varargs.c、layout.c、32bit-doubles.c）、clang/test/Driver/rx-toolchain.c
  （Inputs/basic_rx_tree が GCC の木）、clang/test/Preprocessor/init-rx.c、llvm/test/CodeGen/RX/stack-small-args.ll。
- `scripts/run-clang-test.sh`（`test/clang`）: caller.c と callee.c を clang と rx-elf-gcc でコンパイルし、4 通りに組み合わせて
  （GCC→clang、clang→GCC、clang 同士、GCC 同士）clang のドライバでリンクし QEMU で走らせる。スカラー（char / short の
  レジスタ・スタック、戻り値の拡張）、i64 / double / float、25 種の構造体をそれぞれ先頭・int 3 個の後・long long 2 個の後
  （スタック）に置いた引数と戻り値、小さな構造体を詰めたスタック、sret と R1〜R4、`_Complex`、可変長（構造体を含む）、
  va_copy、va_list の受け渡し、コールバック（呼ばれた側から呼んだ側の関数を呼ぶ）、ビットフィールドのレイアウトと
  sizeof / _Alignof。1 通りあたり 1097 チェック。
  - 上流 GCC と: rx62n / rxv2 / rxv3 / rxv3e の -O0 / -O2 / -Os、GNU ld でも全通過。上流 GCC は RXv2 以降を持たないので
    GCC 側は常に rx600 のコード。
  - GNURX と: `GCC_OBJS=dir` で GNURX が Windows で作った caller_gcc.o / callee_gcc.o を使う。`-misa=v1`（rx62n）、
    `-misa=v2`（rxv2）、`-misa=v3`（rxv3）、`-misa=v3 -mdfpu`（DFPU 命令を使う。rxv3 / rxv3e）の -O0 / -O2 で全通過。
    ```
    # Windows（Git Bash）、test/clang で
    rx-elf-gcc.exe -misa=v3 -mdfpu -m64bit-doubles -O2 -std=gnu11 -fwrapv -c caller.c -o $D/caller_gcc.o  # callee.c も
    # WSL
    RX_CPU=rxv3 OPT=2 GCC_OBJS=$D bash scripts/run-clang-test.sh
    ```
  - 両方で作る値（構造体の float など）はビット列から作る: GCC の `-fpu` は `-ffinite-math-only` を含み、GNURX の
    `-mdfpu -O2` は `-(float)0` をある翻訳単位では +0.0、別の単位では -0.0 にした（GNURX 同士でも食い違った）。
  - clang に `-mno-ms-bitfields` や `-m32bit-doubles` を付けると（`CLANG_FLAGS`）GCC との組み合わせだけ失敗する
    （テストが違いを見つけることの確認）。
  - 値の一部は int をあふれさせるので `-fwrapv`（無いと clang の最適化で参照値と食い違った）。
  - 起動コード（scripts/rx-start.s）は clang の統合アセンブラで組み立てる。
  - run-all-tests.sh は build/lldb の clang（scripts/build-lldb.sh）があるときに各 CPU の -O0 / -O2 で走らせる。

## llvm-test-suite（2026-11-12）
`scripts/run-test-suite.sh` が llvm-test-suite（既定は SingleSource）を clang でビルドし、QEMU（gdbsim-r5f562n8）で走らせる。
```
GNURX=/path/to/GNURX/rx-elf/rx-elf CLANG=build/lldb/bin/clang scripts/run-test-suite.sh   # WSL
```
- C / C++ のライブラリは GNURX の newlib と libstdc++（`--gcc-toolchain`。GNURX のインストール先 `rx-elf/rx-elf`。
  WSL からは /mnt/c/ProgramData/... で読める）。GNURX の newlib はシステムコールを `write` / `read` / `sbrk` / `fstat` /
  `isatty`（下線なし）と `_exit` の名前で呼ぶ。
- test/test-suite: 起動コード（start.s）、システムコール（syscalls.c）、リンカスクリプト（rx-qemu-libc.ld、すべて SDRAM 16 MB）。
  - stdout / stderr は SCI0。`_exit` は印（`\4\4RX-EXIT n`）のあとに終了コードを送る。
  - 引数と stdin は、scripts/test-suite-run.py が QEMU のローダで `__args`（0x01fb0000、64 KB）に置く。
  - 時計: CMT0（PCLK / 512 = 93750 Hz）と CMI0 の割り込みで数え、`times()`（`clock()`）と `gettimeofday()` が読む。
    QEMU は `-icount shift=3,sleep=off` なので、時間は実行した命令数（1 命令 8 ns）で進み、何度走らせても同じ値になる。
    `clock()` を 0 や固定の進み方にすると、経過時間で割るテスト（stepanov_v1p2）が参照出力と合わない。
  - GCC の crtbegin.o は使わない: `__dso_handle` と `.eh_frame` の登録（`__register_frame_info`）は syscalls.c が持つ。
    crtbegin の `.init_array` / `.fini_array` は .ctors 形式（-1 ... 0）で、newlib の `__libc_init_array` が呼べない。
- test/test-suite/host-tools.cmake: test-suite が target 用にもビルドする timeit-target と not（newlib ではビルドできない）を
  imported にし、起動コード・システムコール・リンカスクリプトを全プログラムのリンクの依存に加える（変えると再リンクされる）。
- `-DSMALL_PROBLEM_SIZE=ON`。テストごとの上限は 1800 秒（`TEST_TIMEOUT`）。24 並列で約 1 時間。

結果（rx62n、-O2、SingleSource 411 本）: 387 通過、失敗 15、ビルドできない 9。
- C++ の例外（11）: RX バックエンドは例外を扱わない（`ExceptionsType = None`、CFI / LSDA を出さない）。throw は terminate。
- RAM（3）: Shootout-strcat と Shootout-C++-strcat（60 MB のバッファ）、smallpt（18 MB の画像）。gdbsim の SDRAM は 16 MB。
- 時間切れ（1）: himenobmtxpa（QEMU と soft-float の double で 30 分を超える）。
- ビルドできない（9）: TLS（2）、pthread（spirit）、`std::thread`、`err.h`（fasta）、fenv の `FE_TONEAREST`、64 ビットの atomic
  （`__atomic_fetch_add_8`）、newlib の `infinity()` と名前が衝突（ray、sphereflake）。
- 見つけて直したもの（CDEF-inc/llvm-project#42、rust#43）: ベクタの比較で clang がアサート（`getSetCCResultType`）、
  未定義の weak への分岐がアドレス 0 に届かず lld がエラー、64 KB を超えるフレームで dsp:16 に収まらないスタック位置が
  fatal error。

## 残っていること
- C++ の例外（上記、#170）: RXMCAsmInfo の `ExceptionsType`、フレームの CFI、例外ポインタ / セレクタのレジスタ。
- TLS（#171）: clang が RX で受け付けない。GCC と同じく emulated TLS（libgcc の `__emutls_get_address`）にする。
- 64 ビットの atomic（#172）: `__atomic_*_8` の実装が無い（GNURX に libatomic が無い）。
- test-suite の環境の制約（#173）: RAM 16 MB、30 分の上限、newlib に無いもの（pthread、`err.h`、fenv、`infinity()`）。
- rustc の callconv/rx.rs も GCC に合わせた（CDEF-inc/rust#42、rx-rust#161）: char / short の引数を拡張せず、bool は i8、
  可変長の位置の構造体は byval（NOTES-rustc.md、test/rust/cabi-smoke）。
- `__attribute__((interrupt))` / `fast_interrupt` は未対応（バックエンドにも無い。Rust は rx-rt の asm で入口を作る）。
- `-mbig-endian-data`、`-mpid`、`-mno-allow-string-insns`、`-msmall-data-limit`、`-mint-register`、`-mgcc-abi` は無い
  （バックエンドが対応していない）。
- RX 用の compiler-rt builtins は無い（GCC が無いときのリンク先）。
- llvm-test-suite の MultiSource は未実施（入力ファイルをパスで読むものが多く、ローダで渡す仕組みが要る）。
