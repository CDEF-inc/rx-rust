# LLDB の RX 対応メモ（2026-11-03、issue #76）

LLDB で RX のプログラム（Rust、C）をデバッグする。QEMU の gdbstub には直接つなぎ、
実機（AP-RX72M-0A、E2 Lite）には e2-server-gdb との間に橋渡し（`scripts/hw/lldb-bridge.py`）を置く。

## LLDB 側（CDEF-inc/llvm-project）

- `ArchSpec`: `rx` コア（`llvm::Triple::rx`、`EM_RX`）。リトルエンディアン、アドレス 4 バイト、
  命令 1〜8 バイト。
- `ABISysV_rx`（`lldb/source/Plugins/ABI/RX`）:
  - **レジスタ:** 名前は GDB の `org.gnu.gdb.rx.core`（QEMU と e2-server-gdb が使う）と同じにする。
    サーバーのレジスタに、名前で DWARF 番号と汎用番号が付く。
    - DWARF 番号は RX バックエンドのもの: r0〜r15 = 0〜15、psw = 16、pc = 17、dr0〜dr15 = 32〜47。
    - 汎用番号: sp = r0、fp = r6、flags = psw、引数 = r1〜r4。
  - **関数入口と既定のアンワインド:** CFA = r0 + 4、PC は [CFA − 4]（呼び出しが戻り番地を積む）。
    実際のフレームはコンパイラの `.debug_frame` で解く。
- `EmulateInstructionRX`（`lldb/source/Plugins/Instruction/RX`、#85）: CFI のない関数のプロローグを命令の
  エミュレーションで解く（`UnwindAssemblyInstEmulation` が使う）。
  - 扱う命令: push.l / pushm / pop / popm / pushc / popc、SP の add / sub、フレームポインタ（`add #-n, r0, r6`、
    `mov.l r0, r6`、`mov.l r6, r0`）、分岐。
  - 分岐を扱うのは、関数の途中の rts / rtsd の後ろのブロックに、そこへ飛ぶ分岐の状態を渡すため。
  - `UnwindAssemblyInstEmulation` は、関数の入口で CFA のレジスタが CFA そのものだと仮定していた。RX では
    CFA = SP + 4 なので、入口の行のオフセットだけ下から始めるように直した（他のターゲットではオフセットが 0）。
  - テスト: `RXInstEmulationTests`（lldb/unittests/UnwindAssembly/RX）と check-lldb-c.sh の `DEBUG=0`。
  - **呼び出し先保存:** r0 と r6〜r13。
  - **戻り値（`finish`、#84）:**
    - 8 バイト以下のスカラー（整数、ポインタ、soft-float の float / double）は r1（上位は r2）。
    - 集約型は、Rust のもの（名前にパス `::` がある、またはタプルで、かつフレームの言語が Rust。
      `finish` では呼び出し元、`thread return` では関数自身のフレーム）なら Rust の ABI で読む。
      名前空間付きの C++ の型は、フレームが Rust でないので C の規則になる:
      - スカラー 2 つの組（rustc の ScalarPair）は、メモリ上で前にあるほうが r1、後ろのほうが次のレジスタ
        （rustc はフィールドを並べ替えることがあり、`{ a: u8, b: u32 }` では b が先）。
        (u8, u8) や (u16, u16) のように、2 つ目が 4 バイト目より前にあるときは、レジスタを分けて読む;
      - その他の 8 バイト以下は、整数として r1(:r2);
      - それより大きいものはメモリ経由（sret）なので、値は出ない。
    - 型が C のものなら GCC の rx-elf の ABI で読む: 1〜16 バイトでサイズが 4 の倍数なら r1〜r4、それ以外は
      メモリ経由（値は出ない）。
    - DWARF には関数の呼び出し規約がないので、ABI は型とフレームの言語で決める。そのため、Rust の構造体を返す
      `extern "C"` 関数を Rust から呼んだときは Rust の規則で読まれ、値が出ないか、サイズが 4 の倍数で
      ないときは誤った値になる（C から呼んだときは C の規則で正しく読む）。
  - **`thread return`:** 値を r1〜 に書く。対象は、レジスタで返るスカラーと集約型。
  - **引数（`GetArgumentValues`）:** 整数とポインタを、GCC の規則（r1〜r4 に 16 バイト、残りは
    [r0 + 4] からスタック）で読む。CLI からは使われないので、テストはしていない。
  - 式の評価で関数を呼ぶこと（`PrepareTrivialCall`）には対応しない。関数を呼ぶ式は、エラーになる（#79）。
- `GDBRemoteRegisterFallback`: `target.xml` を読めないとき（libxml2 なしの LLDB）の RX のレジスタ
  （`org.gnu.gdb.rx.core` の順）。
- ソフトウェアブレーク命令は `brk`（0x00）。
- 逆アセンブラは RX の MC 層を使う。CPU の指定がなければ `rxv3e`（RXv1〜RXv3e、FPU、DFPU すべて）で
  デコードする。
- clang の `RXTargetInfo`（`clang/lib/Basic/Targets/RX.*`）:
  - LLDB の型システム（clang の ASTContext）に必要。これがないと、構造体の表示で LLDB が落ちる。
  - 型は GCC の rx-elf と同じにする（`-m64bit-doubles`）: long は 32 ビット、long long / double /
    long double は 64 ビットで 4 バイト境界、size_t は unsigned long。
  - clang は RX のコードを生成しない。
- テスト:
  - `lldb/unittests/Utility/ArchSpecTest.cpp`;
  - `lldb/test/Shell/Disassemble/TestDisassembleRX.test`（ELF のアーキテクチャ認識と逆アセンブル）。

## ビルド

`scripts/build-lldb.sh` が build/lldb に clang + lldb をビルドする。

- LLDB は clang を必要とするので、build/llvm とは別のディレクトリにしている。テストのたびに
  build/llvm をビルドし直しても、clang までは作らずに済む。
- Python と Lua のスクリプト機能はなし。`JOBS` でコンパイルの並列数を変えられる（既定 6）。
- `LLDB_PYTHON=1` で Python を有効にする（rustc の表示整形のため、#83）。`python3-dev` と swig が要る。
  - その lldb はビルドしたマシンの libpython3.x が要るので、手元で使うだけにする。
  - `dist-toolchain.sh` は、Python 付きの build/lldb を詰めるのを断る。
  - 配布物と CI の LLDB は Python なしのまま。CI では表示整形のテストを飛ばす。
- libxml2（2.13.9）をソースから静的ライブラリとしてビルドし、LLDB にリンクする（#82）。
  - 目的: GDB サーバーの `target.xml` とメモリマップ（`qXfer:memory-map`）を読むため。
  - ICU・lzma・zlib なしでビルドするので、配布物の lldb が必要とするライブラリは増えない。
  - ソースは download.gnome.org から取得し、SHA-256 で確かめる。
- libxml2 なしの LLDB でも動くように、`GDBRemoteRegisterFallback` とブリッジの `qRegisterInfo` は残す。
- `scripts/dist-toolchain.sh` は、build/lldb があれば `bin/lldb` と `lib/liblldb.so` を配布物に入れる。

## QEMU

```sh
qemu-system-rx -M gdbsim-r5f562n8 -bios rom.bin -display none -S -gdb tcp:127.0.0.1:1234 &
lldb ELF -o 'gdb-remote 1234'
```

`scripts/check-lldb.sh` は test/rust/lldb-demo（dev プロファイル）を QEMU で動かし、次を確かめる:

- Rust の関数名でブレークし、`scale` → `middle` → `outer` → `main` → `rx_rt::start` →
  `_rx_reset` まで遡れる（rx-rt の `_rx_reset` に、PC を undefined にする CFI を入れて、そこで止める）;
- 引数と局所変数（構造体を含む）、`finish` の戻り値、`next`、`up`;
- 式（`expr k + 1`、構造体のフィールド）と、関数を呼ぶ式がクラッシュせずエラーになること;
- `src/rets.rs` の関数の `finish`: 2 / 3 / 8 バイトの構造体、フィールドが並べ替えられる構造体、
  (u16, u16)、(u8, u16)、`Option<u32>`、u64、f64。
  12 バイトの構造体（メモリ経由）では値が出ないこと。`thread return 7` で r1 = 7;
- レジスタと逆アセンブル;
- ウォッチポイント、グローバル変数;
- `src/types.rs` の局所変数で、Rust の型がどう出るか（下の「Rust の型」）;
- `src/user.rs` の割り込みハンドラから、ユーザーモードで割り込まれたコードまで遡れること（下の「割り込みをまたぐ
  バックトレース」）。lldb-demo は最後にこのループに入って戻らない。

### 割り込みをまたぐバックトレース（#85、2026-11-09）

rx-rt の `_rx_isr_common` の CFI で、LLDB はハンドラから割り込まれたコードまで遡れる。QEMU で次を確かめた:

- **ソフトウェア割り込み（isr-save、rxv1、dev）:**
  - `soft` → `_rx_isr_common` → `main`（`int #65` の行）→ `rx_rt::start` → `_rx_reset`;
  - `main` のフレームの PSW は、割り込まれたときの値（スタックに積まれた PSW）;
  - `finish` を繰り返すと、ハンドラから `_rx_isr_common`、そこから `rte` で `main` に戻る。
- **CMT の割り込み（hw-irq、rxv3、dev）:**
  - `cmi1` → `_rx_isr_common` → `main` の `interrupt::wait()`（インライン展開されたフレーム）。PSW は I = 1、
    IPL = 0;
  - **入れ子:** `cmi0` の中で割り込みを許可したところに `cmi1` が入ると、`cmi1` → `_rx_isr_common` → `cmi0` →
    `_rx_isr_common` → `main` → `_rx_reset` と遡る。`cmi0` のフレームの PSW は IPL = 1、I = 1。
- **ユーザーモード（lldb-demo の `user.rs`、check-lldb.sh で毎回確かめる）:**
  - USP に切り替えて `int #64` を繰り返すループで、ハンドラ `trap` → `_rx_isr_common` → `_lldb_user_loop`;
  - ループのフレームの r0 は USP の値、PSW は U = 1。CFI の式（PSW.U が 1 なら退避した USP、0 なら CFA）が、
    U = 1 の側で評価されている;
  - ループの戻り番地は ISP 側にあり、DWARF では ISP を指せない。そのため、ループの CFI は PC を undefined にして、
    そこでバックトレースを終える。
- 割り込まれたフレームの PC は、次に実行する命令の番地である。LLDB は呼び出し元のフレームの行を PC − 1 で引くので、
  直前の命令の行が出る。`int` なら `int` の行、`setpsw i` の直後に入った割り込みなら `interrupt::enable` の行になる。
- `_rx_isr_common` 自身のフレームの PSW は「unavailable」になる。どこにも退避していないため。
- 実機: EK-RX74M（2026-11-09、hw-lldb-check.sh）で、ユーザーモードの場合が QEMU と同じになる
  （r0 = USP、PSW.U = 1）。ソフトウェア割り込み・CMT・入れ子の場合は、実機ではまだ確かめていない。

### Rust の型（#83、2026-11-09）

LLDB には Rust のプラグインがない。Rust の DWARF を clang の型システム（C / C++）で読むので、次のように出る。
表は Python なしの LLDB でのもの。Python 付きの LLDB（`LLDB_PYTHON=1`）なら、rustc の表示整形（`src/etc` の
Python スクリプト。`rust-lldb` が読み込む）が使える（下の「表示整形」）。

| 型 | 表示 | 読み方 |
|---|---|---|
| 整数、`char`、`bool` | C の名前（`unsigned int`、`short`、`char32_t`、`bool`）。`u8` / `i8` は文字（`'\x02'`） | — |
| 構造体、タプル、ジェネリクス | `(lldb_demo::Point) point = (x = 11, y = -12)`、`((u8, i32)) tuple = (__0 = '\x01', __1 = -1)`。型引数も C の名前（`Wrapper<short>`） | — |
| C 風の enum | `(lldb_demo::types::Color) color = Green` | — |
| データ付きの enum（`Option`、`Result`、`Shape`） | すべての variant が `$variants$` の下に並び、それぞれに同じ `$discr$`（判別値）が付く | `$discr$` で有効な variant を選び、`frame variable some.$variants$.$variant$1.value.__0` |
| `&str` | `(data_ptr = "rx-rust", length = 7)` | — |
| スライス | `data_ptr` と `length` だけで、要素は出ない | `frame variable -Z 2 slice.data_ptr`、`parray 2 slice.data_ptr` |
| 配列 | `([0] = 10, ...)`。`[u8; N]` は文字列（`"RX72"`） | — |
| 参照 | ポインタ（`(lldb_demo::Point *) point_ref = 0x...`） | `frame variable *point_ref`、`point_ref->x` |
| クロージャ | 捕まえた変数への参照（`{closure_env#0}` の `_ref__k`） | `frame variable *add_k._ref__k` |

- `expr` で enum の variant の中を指すと（`expr some.$variants$.$variant$1.value.__0`）、LLDB が落ちる。clang の
  アサーション（`missing temporary materialization conversion`）による。
  - x86_64 の Rust のプログラムでも同じなので、RX 特有ではない。
  - build/lldb はアサーション付きでビルドしているので、落ちる。
  - `frame variable` は clang の式評価を通らないので、同じ式で読める。

#### 表示整形（2026-11-09）

Python 付きの LLDB で `command script import rust/src/etc/lldb_lookup.py`（`rust-lldb` がすること）を実行すると、
上の型が次のように出る（QEMU と EK-RX74M で同じ）:

| 型 | 表示 |
|---|---|
| `Option` / `Result` | `Some(7)`、`None`、`Ok(100)`、`Err(-3)` |
| データ付きの enum | `Circle(300)`、`Rect{w:4, h:9}`（フィールドも子として出る） |
| `&str` | `"rx-rust"`（文字も子として出る） |
| スライス | `size=2 { [0] = 20, [1] = 30 }` |
| 参照 | `0x... {x:11, y:-12}`（指す先の値） |
| 構造体、タプル | `{x:11, y:-12}`、`(1, -1)` |

- PSW などのレジスタのフィールドの表示も `{IPL:0, ..., U:1, ...}` の形に変わる。
- check-lldb.sh は、LLDB に Python があればこれも確かめる（10 項目）。なければ飛ばす。
- 配布物には `lib/rustlib/etc` に `lldb_lookup.py`、`lldb_providers.py`、`rust_types.py` を、`bin/` に
  `rust-lldb` を入れる。配布物の lldb には Python がないので、`rust-lldb` は Python 付きの lldb
  （`RUST_LLDB` か PATH）と組み合わせて使う。

`RX_CPU=rxv2|rxv3|rxv3e` で、その CPU 向けにビルドして CDEF-inc/qemu の同じコアで動かす（`scripts/cpu.sh`）。
4 つのコアすべてで同じ結果になる（2026-11-09）。ただし、QEMU の gdbstub には DFPU のレジスタがない（#81）。
そのため、DR レジスタに置かれた f64 の変数はまだ試していない。

`PROFILE=release`（opt-level "s"）は、最適化の後に残るものだけを短いセッションで確かめる:

- バックトレースは dev と同じく `_rx_reset` まで遡れる。ただし `outer` の引数は
  `a=<unavailable>, b=<unavailable>` になる。`a + b` を計算した後は、DWARF の位置リストが途切れているため。
- `finish` で戻った先は、インライン展開された `<u32>::wrapping_add` の中である。LLDB はそれを
  `[inlined]` の付いた別のフレームとして出し、その引数（`self=9, rhs=5`）も出す。
- `rets.rs` の関数のうち、`ret_pair16`、`ret_reordered`、`ret_pair_u8_u16`、`ret_s8` は、`#[inline(never)]` なのに
  呼び出し元（`run`）の中に展開されて残らない。DWARF にもその関数がないので、名前でブレークできない。
  そのため、release のセッションは `ret_s2`、`ret_u64`、`ret_f64` だけで `finish` する。

### GCC の C（`scripts/check-lldb-c.sh`）

test/rust/core-smoke の main.c を GCC（`-g -O0` または `-O2`、`-mcpu=rx600 -m64bit-doubles`）で、
Rust のライブラリを dev でビルドし、run-rust-test.sh と同じくリンクして、QEMU で動かす。
`OPT=0|2` で GCC の最適化を選ぶ。次を確かめる:

- **言語をまたぐバックトレース:** C の関数（`c_struct_mix`）→ それを呼ぶ Rust（`rs_call_c`）→ C の `main` →
  `_start`。`_start`（rx-start.s）には CFI がないが、そこでバックトレースは終わる。
- **C から呼ばれた Rust の関数（`rs_s3`）:** 3 バイトの構造体の引数を表示でき、C の呼び出し元での `finish`
  の戻り値が出る。
- **GCC の DWARF:** レジスタ番号は LLVM と同じ（r1〜r4 の引数、r2:r3 の構造体、r1 の戻り値が正しく出る）。
  GCC の `.debug_frame`（CIE バージョン 3）でも遡れる。
- **C の ABI の `finish`:** int（r1）と double（r1:r2）。

わかっている問題が 2 つある。どちらもテストでは避けていて、直ってはいない:

- **`-O0`: 1 行に書いた関数でブレークすると、プロローグの前で止まる。** たとえば
  `double c_double(double x) { return x * 0.5; }` である。行番号表に、プロローグの後の行が別にないので、
  LLDB はプロローグを飛ばせない。そこではまだ引数がスタックに書かれていないので、`x` はでたらめな値になる。
  避けるには、`next` か `si` でプロローグを越えてから見る。GDB は命令を解析してプロローグを飛ばす。
- **`-O2`: スタックで渡された構造体の引数の場所を、GCC が誤って出す。** たとえば `c_struct_mix` の `a`
  （3 バイト）である。
  - GCC は場所を `DW_OP_breg7`（r7 + 0）と書く。r7 は GCC の rx の引数ポインタ（`ARG_POINTER_REGNUM`）で、
    コードでは SP に置き換わっているが、デバッグ情報だけ置き換わっていない。
  - そのため LLDB は r7 の指す番地を読んで、でたらめな値を出す。GCC 14.2 の不具合である（GDB でも同じになるはずだが、確かめていない）。
  - レジスタで渡された引数は正しく出る。

run-all-tests.sh は、build/lldb があれば、check-lldb.sh（dev、release、rxv3）、check-lldb-c.sh（-O0、-O2）と
lit テストを実行する。

`DEBUG=0` は C を `-g` なしでビルドする。C の関数には CFI も DWARF もないので、LLDB は
`EmulateInstructionRX` でプロローグを解いて遡る（#85、2026-11-09、`OPT=2`）:

- **Rust から、CFI のない C の `main` へ:** `rs_s3` → `_main + 535` → `_start + 10` で終わる。
  エミュレータのない LLDB では、`_main` の次がでたらめな番地（`0x0001092c`）になっていた。
- **CFI のない C の関数の中から:** `_c_struct_mix` の `push.l r7; sub #4, r0` の後で止めると、
  `rs_call_c`（Rust）→ `_main` → `_start` と遡り、`finish` で `rs_call_c` に戻る。
- DWARF がないので、C の関数名はシンボルの名前（`_` 付き）で出る。ブレークポイントも `-n _c_struct_mix` で
  置き、入口で止まる（プロローグを飛ばさない）。

まだ試していないもの（#87）:

- DR レジスタに置かれた値（QEMU の gdbstub に DFPU のレジスタがない、#81）;
- LLDB の gdb-remote の API テスト（Python が要る、#83）。

（実機での LLDB のセッションは scripts/hw-lldb-check.sh が自動で回す。下の「実機」。）

## 実機（e2-server-gdb）

```sh
# WSL（hw.lldb は hw-connect = process connect --plugin gdb-remote listen://127.0.0.1:61235 を定める）
lldb ELF -s scripts/hw/hw.lldb -o hw-connect
# Windows の Git Bash（順序はどちらが先でもよい）
scripts/hw-lldb.sh ELF
```

### セッションの回し方（#86）

ブリッジとサーバーは、LLDB のセッションをまたいで動き続ける。サーバーの起動（約 10 秒）は最初の 1 回だけ。

| したいこと | LLDB で | ブリッジがすること |
|---|---|---|
| 最初から走らせ直す | `kill`、`hw-connect` | `kill` でプログラムを止め、LLDB のブレークポイントを外す。`hw-connect` で ELF をそのときのファイルから読んで書き込み直し、リセットする |
| ビルドし直した ELF で | `kill`、`target delete`、`target create ELF`、`hw-connect` | 同じ（書き込むのは新しい ELF） |
| 走らせたまま離れ、あとで止める | `process detach`、のちに `hw-connect` | ブレークポイントを外して走らせ、次の接続で止める |
| すべて終える | （LLDB を終える） | `hw-lldb.sh` の端末で ^C |

- LLDB は、ターゲットの ELF が書き換わっても `process connect` では読み直さない（QEMU で確かめた。
  ELF を差し替えても `main` は元の番地のまま）。新しいターゲットを作れば読み直す。
- LLDB のセッションの中でのリセット（`process plugin packet monitor reset`）は、LLDB が状態の変化を知らないので
  使わない。`kill` と `hw-connect` で走らせ直す。
- `hw-lldb.sh --sessions N` は N 回のセッションで終わる（hw-lldb-check.sh は 3）。
- 端末が 2 つ要る（Git Bash で `hw-lldb.sh`、WSL で LLDB）のは変わらない。WSL の interop が無効で、
  NAT モードの WSL から Windows の localhost に届かないため（#99）。

WSL（NAT モード）からは Windows の localhost に届かないが、Windows からは WSL の localhost に届く。
そこで、LLDB が待ち受け、ブリッジが接続しに行く（2 分間再試行する）。
この環境では WSL の interop（WSL から Windows の exe を起動する機能）が無効なので、WSL 側から
サーバーを起動することもできない。Windows の LLDB なら `hw-lldb.sh --listen ELF` と `gdb-remote 61235`。

実機用の lldb-demo は `scripts/hw-build.sh lldb` がビルドする（`build/hw/lldb/lldb-demo.elf`、
`HW_BOARD=rx74n|rx74m` なら `build/hw-rx74n/lldb/`）。コードフラッシュ（RX74N / RX74M は MRAM）に置き、
リセットベクタから起動する。EK-RX74N / EK-RX74M でも `HW_BOARD=rx74m scripts/hw-lldb.sh ELF` で同じように動く。

### 実機での自動テスト（`scripts/hw-lldb-check.sh`、#87）

Git Bash から、WSL の LLDB（build/lldb）をバッチで動かし、`hw-lldb.sh` のブリッジにつないで、出力を
check-lldb.sh と同じように照合する。リポジトリは WSL にあること（`LLDB_DISTRO`、既定 Ubuntu）。

- バックトレース（`scale` → … → `_rx_reset`）、構造体の引数;
- LLDB に Python があれば、rustc の表示整形（`Some(7)`、`Rect{w:4, h:9}`、`"rx-rust"`、スライスの要素）と ^C。
  ^C は LLDB の非同期モード（Python から切り替える）が要るので、Python がなければ走らせずに終える。
  Python の有無は、先に LLDB を 1 回動かして確かめる;
- ユーザーモードで割り込まれたコードへの巻き戻し（r0 = USP、PSW.U = 1）;
- `int #64` へのシングルステップ（ハンドラ `_rx_vector_64` で止まり、そこからループへ遡れる）;
- ウォッチポイント（#80）: ハンドラが書く `TRAPS` で 2 回続けて止まり（値 1 → 2）、2 回目のセッションでは、
  リセット後のブレークポイントで設定した `finished` が書き込み（0 → 1）で止まる;
- 2 回目のセッション（`kill` のあと。リセットベクタから始まり、ブレークポイントがまた効く）と、
  3 回目のセッション（`process detach` のあと。走っていたユーザーモードのループで止まる）。

`wsl.exe` は `bash -c` のスクリプトの後ろの引数を渡さないので、WSL 側で動かす部分はファイル
（`build/hw*/lldb/hw-lldb-check.run`）に書いて実行する。

2026-11-09、EK-RX74M（`HW_BOARD=rx74m`）で 31 項目すべて通過（43 秒）。Python なしにした場合も、表示整形と ^C を
飛ばして通る（26 項目、38 秒）。AP-RX72M-0A ではまだ回していない。

- LLDB が時間切れ（`LLDB_TIMEOUT`、既定 300 秒）になったら失敗にする。
- LLDB が途中で終わると、ブリッジは次のセッションを待ち続ける。LLDB が終わって 30 秒たってもブリッジが
  終わらなければ、止めて（サーバーも止まる）失敗にする。
- 3 回目のセッションの停止は、ループのどこか（`_rx_isr_common` の中のこともある）で止まったことだけを見る。
  `_rx_isr_common` の中で止まると、バックトレースが壊れる。LLDB の非同期停止での最初のフレームの巻き戻しは、
  命令のエミュレーション（EmulateInstructionRX）による。これは、割り込みで入ったときのスタック
  （CFA = SP + 36）も `dpushm` / `dpopm` も知らない。DWARF の CFI は正しいが、LLDB は x86 以外では CFI を
  エミュレーションで補う形を使わない（FuncUnwinders）。未解決。

### ブリッジがすること

e2-server-gdb の癖（docs/NOTES-hw.md）を LLDB から隠す:

- **起動:** 1 回接続し、ELF を書き込んで（`prg_download_start_on_connect` 〜 `prg_download_end`）、
  リセットし、e² studio の起動シーケンスを送る。LLDB にはリセットベクタで止まった状態を見せる。
- **停止:** LLDB の all-stop の `c` / `s` / `vCont` と ^C を、サーバーの non-stop の
  `vCont;c` / `vCont;s:p1.1` / `vCont;t:p1.1` にする。サーバーの `%Stop` 通知（と `vStopped`）を
  待って、停止の応答を返す。
- **レジスタ:**
  - `p` の値はビッグエンディアンなので、バイト順を逆にする。`g` は `p` を並べて作る（`g` そのものは
    サーバーが失敗する）。
  - **見せるレジスタ:** r0〜r15、usp、isp、psw、pc、intb、bpsw、bpc、fintv、fpsw、acc0 / acc1、extb。
    acc0 / acc1 は、サーバーの target.xml では 96 ビットだが、`p` は 128 ビットで返す。
  - DFPU のレジスタ（dr0〜、dpsw〜depc）は見せない。dr0 を `p` で読むとサーバーが落ち、
    dpsw 以降は長さのおかしい値を返す。
- **そのまま通すもの:**
  - メモリ（`m` / `M`）;
  - ブレークポイント（`Z0` / `Z1`）。サーバーは、コードフラッシュの `Z0` もハードウェアブレークにする。
    停止のあとに設定し直しても効く;
  - モニタコマンド（`qRcmd`。LLDB の `process plugin packet monitor ...`）。
- **ウォッチポイント（#80）:** `Z2`（書き込み）/ `Z3`（読み出し）/ `Z4`（アクセス）をサーバーに通す。
  - サーバーの停止通知には、どのウォッチポイントかが入らない（`watch:` がない）。そこでブリッジが
    `watch:` / `rwatch:` / `awatch:` を付ける。ウォッチポイントが 1 つならそれ、複数なら値が変わったもの
    （走らせる前に読んでおく）、どれも変わっていなければ最初のもの。止まった PC がブレークポイントなら付けない。
  - CPU はアクセスした命令の後で止まる。qHostInfo の `watchpoint_exceptions_received:after` で LLDB に伝える
    （RX の既定もそう）。
  - `qWatchpointSupportInfo` には、サーバーの `monitor get_no_hw_bkpts_available`（EK-RX74M では 8）を返す。
    ハードウェアブレークと共有する。
  - EK-RX74M（2026-11-09）では、最初の実行の前に設定しても、停止のあとでも、走っている間に設定しても、
    アクセスのたびに止まる。`Z3` / `Z4` も効く。以前 AP-RX72M-0A で見た「リセット後の最初の実行の前に設定した
    ときだけ、1 回だけ効く」は起きない（#98 と同じく、ボードか E2 Lite のファームウェアの違いかもしれない）。
    AP-RX72M-0A ではまだ試していない。
  - LLDB では `watchpoint set variable TRAPS` のように、その変数が見えるフレームで設定する。
    `lldb_demo::user::TRAPS` のような修飾した名前は見つからない（QEMU でも同じ）。
- **`int #n` と `brk` のステップ（#98）:** ステップの前に PC の命令を読み、`int #n` か `brk` なら、
  サーバーにシングルステップさせない。代わりに、ハンドラ（INTB のベクタ表から読む）と次の命令に
  一時的なハードウェアブレーク（`Z1`）を置いて走らせ、止まったら外す。LLDB にはステップとして答える。
  LLDB 自身がブレークポイントを置いている番地には、一時的なものを置かない（外すと LLDB のものまで消えるため）。
  サーバーが `Z1` を断ったら（ハードウェアブレークが足りないなど）、置けた分を外し、走らせずにその場で
  止まったと答え、理由を LLDB のコンソールに出す。ブレークポイントを減らしてからステップし直す。
- **セッションの終わり（#86）:**
  - LLDB が `kill` したり切断したりしたら、プログラムを止め、LLDB のブレークポイントを外して、次の接続を待つ。
    次の接続が来たときに ELF をファイルから読んで書き込み直し、リセットする（起動シーケンスは最初の 1 回だけ。
    scripts/hw/run.py と同じ）。その間にビルドし直した ELF が書き込まれる。
  - `detach` なら、ブレークポイントを外して走らせたまま次の接続を待ち、接続されたら止める（SIGINT として見せる）。
    待っている間に `hw-lldb.sh` を ^C で終えても、プログラムは走ったまま残る。
  - サーバーにはつないだままにする（サーバーはブリッジが切断すると終わる）。ブリッジが終わるとき、
    最後のセッションが `detach` ならプログラムは走ったまま残る。
- `BRIDGE_TRACE=1` で、LLDB とのパケットとサーバーの停止通知を表示する。

### 例外に入る命令をステップ実行しない（#98）

AP-RX72M-0A の e2-server-gdb では、例外に入る命令（`int #n`、`brk`、未定義命令、アクセス違反などの例外を起こす命令）を
シングルステップすると、PC がでたらめな番地になる（docs/NOTES-hw.md）。そこから続けて走らせても正しく動かない。
自由実行（`continue`）なら、例外は正しく処理される。

- EK-RX74M（2026-11-09）では起きない。元のブリッジ（サーバーにシングルステップさせる）でも、`int #64` の
  `si` はハンドラ `_rx_vector_64` で止まり、PSW も例外に入ったときの値（U = 0）になった。
  RX74N / RX74M 用の E2 Lite の設定か、11-07 に更新した E2 Lite のファームウェアの違いかは、わからない。
- いまのブリッジは `int #n` と `brk` をサーバーにステップさせず、ハンドラまで走らせる（上の「ブリッジがすること」）。
  EK-RX74M では、これでも同じくハンドラで止まる。AP-RX72M-0A ではまだ試していない。
- 未定義命令やアクセス違反のように、命令から行き先がわからない例外は避けられない。下の回避策による。

LLDB では次のときにシングルステップになる:

- `thread step-inst`（`si`）、`thread step-inst-over`（`ni`）;
- `step` / `next`。今の行の命令を 1 つずつステップするので、その行に例外に入る命令があると同じことになる。
  たとえば `asm!("int #64")` の行、未定義命令やアクセス違反を起こす行がそうである。
  ほかの関数の呼び出しは、`next` なら戻り番地にブレークを置いて走らせるので問題ない。
  `step` で入った先の関数の中で、上のどれかに当たれば同じである。

割り込みを受け付ける命令も、ステップすると例外に入ることがある。保留中の割り込みがあるときの `setpsw i`
（`rx_rt::interrupt::enable`、`interrupt::free` の終わり）と、`wait`（`interrupt::wait`）がそうである。
これらは `#[inline(always)]` なので、呼んだ行の中で 1 命令ずつステップすることになる。
ただし、実機で PC が壊れるのを確かめたのは例外に入る命令だけで、割り込みの場合はまだ確かめていない。

**回避策:** その命令を走らせるときは、行き先にブレークポイントを置いて `continue` する。
停止のあとに置いたブレークポイントも効く。

- **ハンドラで止める:** `breakpoint set -n <ハンドラの関数名>`、または
  `breakpoint set -a <番地>` で、ハンドラの入口に置く。
  - 番地は、割り込みならベクタ表（INTB + 4 × 番号）、例外なら固定ベクタ表または例外ベクタ表（EXTB）から読める。
    たとえば `int #64` なら `memory read -s4 -fx -c1 '$intb + 256'` である（QEMU で確かめた）。
  - rx-rt の入口は、ハンドラの登録のしかたで、通る場所が違う:
    - `rx_rt::interrupt!(n, handler)` のハンドラは、どれも `_rx_isr_common` を通る。そこに置けば、
      この形のどの割り込みでも止まる;
    - レジスタバンクを使うハンドラ（`interrupt!(n, handler, bank = b)`、RXv3）は `_rx_isr_common` を通らず、
      `_rx_isr_bank_call` を通る（test/rust/isr-bank のベクタ 65 と 66）。そちらに置く;
    - どちらの形でも、ベクタごとの入口 `_rx_vector_<番号>`（たとえば `_rx_vector_65`）に置けば、その割り込みで
      止まる。
- **命令の後で止める:** 1 回だけ効くブレークポイントを置いて `continue` する。ハンドラが戻ってきたところで止まる。
  - 次の行に置く: `breakpoint set -o true -l <行番号>`;
  - 次の命令に置く: `breakpoint set -o true -a <番地>`。番地は `disassemble -c 2 --pc` で見る。命令の途中の番地に
    置くと止まらない。
  - `thread until <行番号>` は、RX の Rust のプログラムでは「No line entries matching until target」で失敗する
    （QEMU の lldb-demo で確かめた）。
- 割り込みを受け付ける命令の場合は、その命令の行を `next` / `step` で通らず、上のどちらかで越える。

PC が壊れてしまったら、`kill` と `hw-connect` で走らせ直す（#86、上の「セッションの回し方」）。


### 実機で確かめたこと（2026-11-03）

lldb-demo（rxv3、dev、コードフラッシュ）で、次を確かめた:

- **起動とレジスタ:** リセットベクタで停止。レジスタの読み出し（pc、psw、extb、acc0）。
- **ブレークポイントとバックトレース:**
  - Rust の関数名で 2 か所にブレーク（コードフラッシュ）;
  - バックトレース（`_rx_reset` まで、6 フレーム）;
  - 引数・局所変数・構造体、逆アセンブル。
- **実行制御:** `finish` の戻り値、`next`、`up`、行番号でのブレーク、グローバル変数（`result = 28`）、
  ^C での停止（ブリッジへの直接のテスト）、`kill`。
- **ウォッチポイント:** LLDB に未対応と伝わる（「Watchpoint creation failed」）。

### レジスタのフィールド（2026-11-05、#82）

libxml2 付きの LLDB（#113）で、blinky（コードフラッシュ）のリセットベクタで確かめた:

- LLDB はブリッジの target.xml を読む。
- `register read psw` は値の下にフィールドを出す:
  `(IPL = 0, PM = 0, U = 0, I = 0, O = 0, S = 0, Z = 0, C = 0)`。
- `register read fpsw` も同じ。リセット直後の 0x00000100 は `DN = 1`、ほかは 0。
- `register info psw` はビットの配置表（IPL は 27〜24、PM は 20 など）を出す。

QEMU の target.xml には `<flags>` がないので、QEMU 相手では値だけが出る。
