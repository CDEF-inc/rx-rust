# QEMU の RXv2 / RXv3 / RXv3e 対応メモ

上流 QEMU の RX は RXv1（RX62N）のみ。RXv2 以降の命令を実行して検証するため、
[CDEF-inc/qemu](https://github.com/CDEF-inc/qemu)（qemu/qemu の fork、`rx/v11.1` ブランチ、v11.1.2 起点）で対応する。

- 上流 QEMU は AI が生成したコードを含むパッチを受け付けない（`docs/devel/code-provenance.rst`）。
  この fork は rx-rust のテスト用の社内版で、上流には出さない。
- ステップごとに PR を積む（Q1〜Q7、issue #1 と同じ進め方）。

## ビルド

```
sudo apt install python3-venv libglib2.0-dev libpixman-1-dev zlib1g-dev libfdt-dev
git clone https://github.com/CDEF-inc/qemu -b rx/v11.1 ../qemu   # rx-rust の隣
cd ../qemu && mkdir build && cd build
../configure --target-list=rx-softmmu --disable-docs
ninja qemu-system-rx
```

`scripts/qemu-run.sh` は `../qemu/build/qemu-system-rx` があればそれを使い、なければ PATH の
`qemu-system-rx` を使う（`QEMU=` で指定も可）。QEMU 11 のモニタはアドレスを 8 桁で表示する
（8.2 は 16 桁）ので、どちらも読めるようにした。

## CPU モデル（Q1）

gdbsim ボード（`-M gdbsim-r5f562n8`）で `-cpu` でコアを選ぶ。周辺機能は RX62N のまま。

| `-cpu` | ISA | FPU | DFPU |
|---|---|---|---|
| `rx62n`（既定） | RXv1 | あり | — |
| `rxv1` | RXv1 | なし | — |
| `rxv2` | RXv2 | あり | — |
| `rxv3` | RXv3 | あり | あり |
| `rxv3e` | RXv3e | あり | あり |

- 既定のユニットは LLVM の同名 `-mcpu` に合わせた。`fpu=on|off`、`dfpu=on|off` で変えられる
  （`dfpu=on` は RXv3 以降のみ。違反は起動時エラー）。
- 持っていない ISA・ユニットの命令は未定義命令例外（ベクタ 23）。FPU 命令は `fpu` で、
  RXv2 の命令は RXv2 以降かどうかで判定する（RXv2 の FPU 命令は両方）。

## RXv2 の基本命令（Q2）

マニュアル（RXv3、R01US0316EJ0100。`pdftotext -layout` でテキスト化して参照）で確認した仕様:

- `movli [Rs], Rd`: LI = 1 にしてロード。`movco Rs, [Rd]`: LI = 1 ならストアして Rs = 0、LI = 0 なら
  ストアせず Rs = 1。どちらの場合も LI = 0。LI を 0 にするのは movco / rte / rtfi だけ（例外の受付では変わらない）。
  符号 `fd 2f`（movli）/ `fd 27`（movco）は mov.size の post-increment 形式のサイズ 3 にあたる。サイズ 3 は
  RXv1 では未定義なので、デコーダのグループで movli / movco を先に置き、mov 側はサイズ 3 を未定義にした
  （以前の QEMU は 64bit アクセスとして実行していた）。
- `stz` / `stnz Rs, Rd`（`fc 4b` / `fc 4f`）: Z（stnz は Z でない）のとき Rd = Rs。
- 3 オペランドの `fadd` / `fsub` / `fmul rs, rs2, rd`: rd = rs2 op rs（`ff a/8/b`）。
- `fsqrt`、`ftou`（.L のメモリも可）、`utof`（itof と同じ形式。メモリは memex で拡張してから符号なしとして変換）。
- EXTB（制御レジスタ 13、リセット値 FFFFFF80h）: 固定ベクタテーブル（リセット以外）の先頭。RXv2 以降のみ。

FPU については既存の QEMU の誤りも直した（rx62n にも効く）:

| 項目 | 以前 | マニュアル |
|---|---|---|
| 浮動小数点例外のベクタ | 21（アクセス例外） | 25 |
| FPSW のリセット値 | 0 | 00000100h（DN = 1） |
| cause フラグ | softfloat のフラグをクリアしないので、一度 inexact になると以後ずっと CX | 命令ごと |
| DN = 1 のアンダーフロー | 0 にするがフラグなし | 0、CU / FU を立てる（CX は立てない） |
| 例外発生時の Z / S | 更新していた | 変えない |
| 整数変換の NaN | 符号によらず最大値 | 符号で 7FFFFFFFh / 80000000h（ftou は FFFFFFFFh / 0） |
| NaN を入力にしない無効演算の結果 | 7FC00000h | 7FFFFFFFh |
| MVTC での FS | 計算結果を捨てていた | FU/FZ/FO/FV の OR |

DN = 0 では非正規化数の入力とアンダーフローが未実装処理（CE、マスク不可）。QEMU は常に flush して計算し、
flush が起きたら DN を見て CE の例外にするか、DN = 1 の結果として扱う。

RXv1 のマニュアルは手元にないので、FPSW のリセット値と NaN の扱いは RXv3 マニュアルの記述を全コアに使っている。

テスト:

```
scripts/run-qemu-test.sh fpu-status rx62n rxv2 rxv3 rxv3e     # FPSW と例外（上の表）
scripts/run-qemu-test.sh rxv2-fpu rxv2 rxv3 rxv3e             # 3 オペランド、fsqrt、ftou、utof
scripts/run-qemu-test.sh rxv2-insns rxv2 rxv3 rxv3e           # movli/movco、stz/stnz、EXTB
scripts/run-qemu-test.sh cpu-features rx62n rxv1 rxv2 rxv3 rxv3e   # RXv2 の命令が RXv1 でトラップ
```

`run-codegen-test.sh` は `MCPU`（llc と GNU as の -mcpu）を受け付ける。LLVM が出す RXv2 のコードを実行できる:

```
MCPU=rxv2 QEMU_CPU=rxv2 scripts/run-codegen-test.sh test/codegen/atomic.ll test/codegen/atomic_main.c
MCPU=rxv2 QEMU_CPU=rxv2 scripts/run-codegen-test.sh test/codegen/fpu.ll test/codegen/fpu_main.c
```

atomic.ll（movli / movco の CAS ループ）は 1080 チェック、fpu.ll は 3110 チェックがともに -O0 / -O2 で通る。
同じオブジェクトを rx62n で動かすと movli がトラップする。

## テスト（`test/qemu`、`scripts/run-qemu-test.sh`）

命令単位のテストをアセンブリで書き、llvm-mc（`-mcpu=rxv3e`）でアセンブルして CPU モデルごとに実行する。

```
scripts/run-qemu-test.sh fcmp-flags rx62n rxv2 rxv3 rxv3e
scripts/run-qemu-test.sh cpu-features rx62n rxv1 rxv2 rxv3 rxv3e
```

- `harness.s`: `_check`（r1 = 結果、r2 = 期待値）と、固定ベクタテーブル（0xffffff80、`rx-qemu.ld` の `.fixedvec`）。
  未定義命令例外は r13 = 1 にして `_resume` のアドレスへ戻るので、命令がトラップするかを調べられる。
- テストは CPU ごとに `--defsym` で `V2` `V3` `V3E` `FPU` `DFPU`（0 / 1）を与えてアセンブルする。
  期待値はこれらから書く（例: `TRAPS 1 - FPU, fadd r4, r5`）。
- `fcmp-flags`: `fcmp` が Z / S / O を比較結果で設定し C を変えないこと（直前の命令のフラグに関係なく）。
  PSW を `mvfc` で読む 8 通りと、問題を見つけた `bne` の列。
- `cpu-features`: FPU 命令が FPU なしのコア（`rxv1`）でだけトラップし、整数命令はトラップしないこと。

期待値はマニュアルから書く。QEMU と LLVM を同じマニュアルの読みから作るので、読み違いは両方に入り
テストでは見つからない。実機での確認が最終的な裏付けになる。

## fcmp の後の分岐が古いフラグを見る問題

QEMU 8.2.2（Ubuntu の qemu-system-rx）では、同じ変換ブロック内で `fcmp` の前にフラグを変える命令があると、
`fcmp` の後の条件分岐が古いフラグを見た（NOTES-codegen.md）。原因は `fcmp` などのヘルパーが
`TCG_CALL_NO_WG`（グローバルを書かない）で宣言されていたことで、ヘルパーが書いた `psw_*` の代わりに
TCG がキャッシュした値で分岐していた。`mvfc psw` で読むと正しい値が見えるのはこのため。

上流のコミット 0ce0739d46（"target/rx: Remove TCG_CALL_NO_WG from helpers which write env"）で修正済みで、
v11.1.2 では起きない。Q1 では回帰テスト（`fcmp-flags`）だけ加えた。fpu.ll も -O0 で通るようになった。
このバグは GCC がコンパイルしたテストドライバの float 比較の分岐も狂わせていたので、
codegen テストのチェック数が変わる（m3 3964 → 3957、fpu 3141 → 3110。新しい値が手で数えた数と一致）。

## 予定

| ステップ | 内容 |
|---|---|
| Q1 | CPU モデルと ISA 判定、fcmp の回帰テスト |
| Q2 | RXv2 の基本: movli/movco、stz/stnz Rs、3 オペランドの fadd/fsub/fmul、fsqrt、ftou、utof、FPSW.DN、EXTB |
| Q3 | RXv2 の DSP: A0 / A1（72bit）と各命令 |
| Q4 | RXv3: bfmov/bfmovz、save/rstr（レジスタバンク） |
| Q5 | DFPU: DR0〜DR15、DPSW/DCMR/DECNT/DEPC、21 命令 |
| Q6 | RXv3e: root ワールドだけ（Protect Zone の分離はしない）、新しい制御レジスタ、trcall/trmvfc/trmvtc/trrte |
| Q7 | rx-rust の全テストを各 CPU で実行（`target-cpu=rxv1` の制限を外す） |
