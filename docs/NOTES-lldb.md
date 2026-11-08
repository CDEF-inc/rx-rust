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
- ウォッチポイント、グローバル変数。

run-all-tests.sh は、build/lldb があればこれと lit テストも実行する。
release（opt-level "s"）では引数や局所変数が最適化で消える。
LLDB には Rust のプラグインがないので、型は C の名前で出る（`unsigned int` など）。

## 実機（e2-server-gdb）

```sh
# WSL
lldb ELF -o 'process connect --plugin gdb-remote listen://127.0.0.1:61235'
# Windows の Git Bash（順序はどちらが先でもよい）
scripts/hw-lldb.sh ELF
```

WSL（NAT モード）からは Windows の localhost に届かないが、Windows からは WSL の localhost に届く。
そこで、LLDB が待ち受け、ブリッジが接続しに行く（2 分間再試行する）。
この環境では WSL の interop（WSL から Windows の exe を起動する機能）が無効なので、WSL 側から
サーバーを起動することもできない。Windows の LLDB なら `hw-lldb.sh --listen ELF` と `gdb-remote 61235`。

実機用の lldb-demo:
`RUSTFLAGS="-C link-arg=-Tlink.x -C target-cpu=rxv3" RX_MEMORY_X=$PWD/../../hw/rx72m-memory.x cargo build`
（test/rust/lldb-demo で。build.rs は RX_MEMORY_X をそのまま開くので、ディレクトリによらない
パスにする。コードフラッシュに置き、リセットベクタから起動する）。

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
- **ウォッチポイントは「未対応」と答える:**
  - サーバーは `Z2`〜`Z4` を受け付けるが、ウォッチポイントが効くのは、リセット後に最初に走らせる前に
    設定したときだけで、効くのも 1 回だけ。
  - 停止のあとに設定したものは効かない。
  - LLDB は停止のたびにウォッチポイントを外して付け直すので、効かないまま止まらなくなる。
- **終了:**
  - LLDB が `kill` したり切断したりしたら、ブリッジはプログラムを止めて終わり、サーバーも終わる。
  - `detach` ならプログラムは走ったまま残る。
- `BRIDGE_TRACE=1` で、LLDB とのパケットとサーバーの停止通知を表示する。

### 例外に入る命令をステップ実行しない（#98）

e2-server-gdb では、例外に入る命令（`int #n`、`brk`、未定義命令、アクセス違反などの例外を起こす命令）を
シングルステップすると、PC がでたらめな番地になる（docs/NOTES-hw.md）。そこから続けて走らせても正しく動かない。
自由実行（`continue`）なら、例外は正しく処理される。

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

PC が壊れてしまったら、そのセッションは捨てて `hw-lldb.sh` を起動し直す（#86）。
LLDB の `process plugin packet monitor reset` でもリセットはできるが、LLDB は状態が変わったことを知らない。

ブリッジで自動的に避けることもできる。ステップの前に PC の命令を読み、例外に入る命令なら、ハンドラと次の命令に
一時的なハードウェアブレークを置いて `vCont;c` にする。ただし、これは実機で試す必要があり、まだやっていない（#98）。

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
