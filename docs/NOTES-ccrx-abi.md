# CC-RX の呼び出し規約（2026-11-12）

RX の ABI を文書で定めているのは CC-RX のマニュアル（R20UT3248JJ0115 Rev.1.15、「9.1 関数呼び出しインタフェース」）だけで、
命令セットのマニュアル（RXv3e, R01US0745JJ0101）には無い。clang と rustc はこれまで GNURX（rx-elf-gcc）に合わせていたが、
CC-RX と比べると関数呼び出しで 2 点違っていたので、CC-RX に合わせたものを既定にし、GNURX のものも選べるようにした
（CDEF-inc/llvm-project#45）。

## 比べたもの
同じプローブ（char / short の引数、64 ビットの引数、構造体の大きさ別、戻り値、可変長、ビットフィールド、型の大きさ）を
次の 3 つでコンパイルして比べた。CC-RX は自社のマニュアルとも照らした。
- マニュアル 9.1（表 9.1〜9.3、例 1〜5）と 10 章の処理系定義（(29) ビットフィールド、wchar_t）
- CC-RX V3.08.00（`C:\Program Files (x86)\Renesas\RX\3_8_0`、`-isa=rxv3 -optimize=2`、`-dbl_size=8` でも）
- GNURX 14.2（既定の `-mrx-abi`。GNURX には `-mgcc-abi` が無い）、上流 GCC 14.2（gnu/install）、変更前の clang

| 項目 | マニュアル 9.1 | CC-RX 3.08 | GNURX / GCC / 変更前の clang |
|---|---|---|---|
| char / short / _Bool の引数（レジスタ） | 呼ぶ側が符号 / ゼロ拡張して渡す（表 9.2） | 呼ぶ側が拡張し、呼ばれた側は拡張しない（拡張済みと見なす） | 呼ぶ側は拡張しない（上位ビットは不定）、呼ばれた側が拡張する |
| 収まらずスタックに行った引数の後の引数 | 空いているレジスタに入る（例 1: `f(uchar, ll, ll, short, …)` の short は R4） | 同じ（long long・double・16 バイトの構造体・`_Complex double` の後でも） | 以後はすべてスタック（カウンタはどの引数でも進む） |
| いつもスタックに行く構造体（大きさが 4 の倍数でない、16 超） | レジスタを使わない | 同じ（`f(struct S6, int, int)` の int は R1, R2） | 大きさを 4 に切り上げた分カウンタが進む（R3, R4） |
| 符号を書かない `int` のビットフィールド | unsigned（(29)） | unsigned | signed |
| wchar_t | unsigned short（10 章） | 2 バイト | long（4 バイト） |

同じだったもの: R1〜R4 が引数、R6〜R13 が保存される、R15 が構造体の戻り値の置き場所、16 バイト以下で 4 の倍数の構造体は
R1〜R4 で渡して R1〜R4 で返す（16 バイトでも）、そうでない構造体の戻り値はメモリ（R15）、可変長では最後の名前付き引数も
スタック、スタック上の引数は自然境界（char は 1 バイト）、long long のアラインメントは 4、plain char は unsigned、enum は 4 バイト、
戻り値の char / short は呼ばれた側が拡張（CC-RX の呼ぶ側はそれを当てにする）、`_Complex float` は R1:R2。
CC-RX はマニュアルどおりで、食い違いは見つからなかった。

double の大きさは別の話: CC-RX（`-dbl_size=4`）も GNURX（`-m32bit-doubles`）も既定は 4 バイトだが、clang と rustc の
rx-none-elf は 8 バイト（Rust の f64 = c_double のため）。CC-RX と組み合わせるときは CC-RX 側に `-dbl_size=8` を付ける。

## 分類と対応
- **呼び出しに効く 2 点（直した）**: char / short の拡張と、収まらなかった引数の後の置き方。
  - 拡張: 呼ぶ側も呼ばれた側も拡張する（呼ぶ側の拡張は GNURX の呼ばれた側には無害、呼ばれた側の拡張は GNURX の呼ぶ側の
    ため）。これは ABI を選ばない。clang / rustc は引数に signext / zeroext を付け、バックエンドの LowerFormalArguments は
    signext / zeroext の引数を信用せず sext_inreg / zext_inreg し直す（呼ばれた側の命令が 1 つ増える）。
  - 置き方: ABI で選ぶ。**既定は CC-RX**（レジスタに置いた引数だけカウンタを進める）、`gnu` で GNURX（全部で進める）。
    両方を同時には満たせない（`test/clang` を CC-RX の規約で GCC と組ませると 1097 中 244 が失敗する）。
- **関数の外から見える型の違い 2 点**:
  - wchar_t: 構造体のレイアウトや L"" に効くので ABI に合わせた（CC-RX: unsigned short、gnu: long）。newlib（GNURX）の
    wchar_t 関数と組むときは `-mabi=gnu` を使う。
  - `int` のビットフィールドの符号: レイアウトは同じで、読んだ値の解釈だけが違う。clang にはビットフィールドの符号を
    切り替える仕組みが無い（`-funsigned-bitfields` は無視される）ので、記録だけ。移植するコードは `signed` / `unsigned` を書く。
  - Rust にはどちらも関係しない。

## 切り替え
- clang: `-mabi=ccrx`（既定）/ `-mabi=gnu`（cc1 の `-target-abi`、バックエンドの `TargetOptions::MCOptions.ABIName`）。
  llc は `-target-abi=gnu`。それ以外の名前はエラー。LTO では ABI がモジュールに残らないので、リンク時にも
  `-Wl,-mllvm,-target-abi=gnu` を渡す（未検証）。
- マクロ: どちらも `__RX_ABI__`（スタックの引数が自然境界、GCC の `-mrx-abi` の意味）を定義し、加えて `__RX_CCRX_ABI__` /
  `__RX_GNU_ABI__`。GCC の `__RX_GCC_ABI__` は `-mgcc-abi`（スタックの引数を 4 境界にする古い ABI）のマクロなので定義しない。
  名前も GCC の `-mgcc-abi` / `-mrx-abi` を使わなかったのは同じ理由（GCC の `-mrx-abi` は GNURX の規約そのもので、CC-RX とは
  上の 2 点が違う）。
- ELF の e_flags: binutils の `E_FLAG_RX_ABI`（0x8）は「スタックの引数が自然境界」の印で、`-mgcc-abi` と区別するためのもの。
  CC-RX と GNURX はどちらも自然境界なので、ここに今回の違いを載せると意味が変わる。だから使わない（clang はどちらでも立てる）。
  CC-RX のオブジェクトはこのビットを立てず、Renesas 独自のビットを立てる（例: 0x10802、`-dbl_size=8` で 0x20802）が、
  ld.lld はそのままリンクする。
- rustc: `llvm_abiname` に `ccrx` / `gnu` を足した（CDEF-inc/rust#45）。rx-none-elf は既定（CC-RX）。
  GNURX の規約はターゲット JSON の `"llvm-abiname": "gnu"` で。

## オブジェクトに記録する規約とリンク時の検査（2026-11-13）
規約を CC-RX のものに変えたが、オブジェクトからはどちらの規約で作ったか分からず、混ぜてリンクしても何も言われなかった
（11-12 版以前の LLVM で作ったオブジェクトは GNURX の規約）。CDEF-inc/llvm-project#46 で、規約をオブジェクトに記録し、
ld.lld が食い違いをエラーにするようにした。

- 記録: バックエンドがすべてのオブジェクトに ELF の note を書く（セクション `.note.rx.abi`、SHT_NOTE、メモリに置かない。
  owner `"RX"`、type 1、記述子は規約と版（1））。規約の値は
  - 1（CC-RX）/ 2（GNURX）: そのオブジェクトが規約に依存する。定義した関数か、オブジェクトの外の関数を呼ぶところのどれかが、
    もう一方の規約では引数の置き場所が変わる（LowerFormalArguments と LowerCall で両方の規約の割り当てを比べる）。
    ローカルな関数とその直接の呼び出しは数えない。
  - 3（どちらでもない）: どの関数も両方の規約で同じ。違いはスタックに行った引数の後にしか出ないので、ほとんどのコード
    （実行時ルーチンなど）はこれ。
- 検査（ld.lld）: CC-RX に依存するオブジェクトと、GNURX に依存するオブジェクト、または古い LLVM のオブジェクト（note が無く
  `.comment` が `clang version` / `rustc version`）を一緒にリンクするとエラー。note の無いほかのオブジェクト（GCC、CC-RX、
  アセンブラ）は検査しない。`ld -r` の出力は note を全部持ち、まとめて検査される。
  - `-z rx-abi-check=warning` で警告に、`-z rx-abi-check=none` で検査なし（既定は `error`）。rustc からは
    `-C link-arg=-zrx-abi-check=warning`。
- 限界: GNU ld と CC-RX の rlink は検査しない（rlink は知らないセクションとして無視するか警告する）。GCC のオブジェクトとの
  食い違いは見つけられない（GCC のオブジェクトに note が無い）。
- テスト: llvm-project の `llvm/test/CodeGen/RX/abi-note.ll` と `lld/test/ELF/rx-abi-note.s`、rx-rust の
  `scripts/check-abi-note.sh`（`test/abi-note`。llc と rustc のオブジェクトで、印の値、混ぜたときのエラー、古い rustc の
  オブジェクト（note を消したもの）、`-z rx-abi-check=warning`、GCC のオブジェクトを確かめる。run-all-tests.sh から）。

## テスト
- llvm-project: llvm/test/CodeGen/RX/arg-placement-ccrx.ll（マニュアルの例 1 と CC-RX 3.08 の出力どおりの置き方、
  拡張）、arg-placement.ll と struct.ll は `-target-abi=gnu` で GNURX の置き方、clang/test/CodeGen/RX/abi.c（3 通りの
  `-target-abi`）、Preprocessor/init-rx.c（マクロと wchar_t）、Driver/rx-toolchain.c（`-mabi=`）。
- `scripts/run-ccrx-test.sh`（`test/ccrx`）: caller.c と callee.c を clang（既定の `-mabi=ccrx`）と CC-RX でコンパイルし、
  4 通りに組み合わせて ld.lld でリンクし QEMU で走らせる。マニュアルの規則ごとに: char / short / _Bool の拡張（上位ビットが
  拡張になっていない値から作る）、戻り値の拡張、64 ビットのレジスタ対、マニュアルの例 1、収まらなかった long long / double の
  後の int / float、12 種の構造体を先頭・int 3 個の後・long long 2 個の後に置いた引数と戻り値（R1〜R4 と R15）、いつもスタックの
  構造体の後の int、sret と R1〜R4、可変長（構造体を含む）、コールバック、構造体の大きさとオフセット。1 通りあたり 537 チェック。
  - CC-RX は Windows でしか動かないので、CC-RX 側は Git Bash で `scripts/build-ccrx-objs.sh DIR` が作る
    （`-isa=rxv1 -lang=c99 -dbl_size=8 -optimize=2`。RXv1 なのでどのコアでも走る）。CC-RX のライブラリはリンクしないので、
    テストは CC-RX の実行時ルーチンを呼ばない C99 だけで書いた（浮動小数点の演算と 64 ビットの乗除算なし）。
    ```
    # Windows（Git Bash）
    bash scripts/build-ccrx-objs.sh /c/tmp/ccrx
    # WSL
    RX_CPU=rxv3 bash scripts/run-ccrx-test.sh /mnt/c/tmp/ccrx
    ```
  - run-all-tests.sh は `CCRX_OBJS=DIR` があれば各 CPU の -O0 / -O2 で走らせる（CI には CC-RX が無いので走らない）。
  - 結果（2026-11-12）: rx62n / rxv3 で 4 通りとも全通過。clang に `-mabi=gnu` を付けると CC-RX との 2 通りが 537 中 175 失敗
    （テストが違いを見つけることの確認）。
- `scripts/run-clang-test.sh`（GCC と）は clang に `-mabi=gnu` を付けるようにした（全通過。呼ぶ側の拡張は GCC に無害）。

## Rust と GCC の C（rx-rust のテスト）
rustc の rx-none-elf も CC-RX の規約になった（rust サブモジュールが CDEF-inc/rust#45 を含む）。rx-rust の Rust のテストは
GCC でコンパイルした C と組むので、いつもスタックに行く構造体の後に引数がある関数は、Rust（CC-RX）と GCC（GNURX）で
置き場所が食い違う。test/rust/cabi-smoke の `rs_after_s3(struct s3, u8)` と core-smoke の `rs_s3(struct s3, int)` /
`c_struct_mix(struct s3, struct s8, int)` がそれで（新しい rustc で cabi-smoke 1 件、core-smoke 10 件が失敗）、構造体を
最後に回した（`rs_before_s3(u8, struct s3)`、`rs_s3(int, struct s3)`、`c_struct_mix(struct s8, int, struct s3)`）。
どちらの rustc でも通る。GCC の C と組む Rust のコードで同じ形の関数を使うなら、引数の順を変えるか、ターゲット JSON に
`"llvm-abiname": "gnu"` を書く。

## 残っていること
- Rust と CC-RX の組み合わせの実行テスト（test/ccrx の Rust 版）。
- `_Complex char` など GNU 拡張の型は CC-RX に無い（または試していない）ので、GNURX の扱い（収まればレジスタ）のまま。
