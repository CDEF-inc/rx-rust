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
- 持っていない ISA・ユニットの命令は未定義命令例外（ベクタ 23）。Q1 の時点では FPU 命令を `fpu` で判定する。
  RXv2 以降の命令は Q2 以降で追加し、同じ仕組みで判定する。

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
| Q1 | CPU モデルと ISA 判定、fcmp の回帰テスト（このメモ） |
| Q2 | RXv2 の基本: movli/movco、stz/stnz Rs、3 オペランドの fadd/fsub/fmul、fsqrt、ftou、utof、FPSW.DN、EXTB |
| Q3 | RXv2 の DSP: A0 / A1（72bit）と各命令 |
| Q4 | RXv3: bfmov/bfmovz、save/rstr（レジスタバンク） |
| Q5 | DFPU: DR0〜DR15、DPSW/DCMR/DECNT/DEPC、21 命令 |
| Q6 | RXv3e: root ワールドだけ（Protect Zone の分離はしない）、新しい制御レジスタ、trcall/trmvfc/trmvtc/trrte |
| Q7 | rx-rust の全テストを各 CPU で実行（`target-cpu=rxv1` の制限を外す） |
