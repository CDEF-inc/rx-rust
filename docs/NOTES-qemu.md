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

## RXv2 の DSP（Q3）

- アキュムレータ: RXv2 以降は A0 / A1 の 2 本で 72bit（読むときは bit 71 を 96bit に符号拡張）、RXv1 は A0 だけで 64bit。
  QEMU では Int128 に符号拡張して持ち、命令ごとに 72 / 64bit に切り詰める。
- 符号（GNU as）: RXv1 の DSP 命令の 0 だったビットが A0 / A1 の選択で、RXv1 の形式は A0 の形式。
  `mvfac*` のシフト量は 2 バイト目の bit 0（反転）と 3 バイト目の bit 6 に分かれる。予約の 01b は
  実機（RX72M）では #2 と同じ。
- RXv2 で追加: emaca / emsba / emula、maclh / msbhi / msblh / msblo / mullh、mvfacgu / mvfaclo / mvtacgu、
  racl / rdacl / rdacw。拡張（A1、mvfac* のシフト）: machi / maclo / mulhi / mullo、mvfachi / mvfacmi、
  mvtachi / mvtaclo、racw。
- 演算はマニュアルの Operation のとおり:
  - mul*/mac*/msb*: 16bit × 16bit の積を bit 16 に置いて代入・加算・減算。em*a は 32bit × 32bit（シフトなし）。
  - mvfac*: (A << #imm) を 72bit で切り詰め、bit 95/63/47/31 から 32bit を読む。
  - mvtachi / mvtacgu: Operation は「該当フィールドを消してから、72bit に符号拡張した src をシフトして OR」だが、
    実機（RX72M）では mvtachi はガードビットを変えない（Function の「bit 63〜32 に転送」のとおり）。
    mvtacgu は bit 71〜64 に符号拡張して書く。mvtaclo はゼロ拡張。
  - racw / racl: (A << #imm) + 80000000h を語 / 長語に飽和し、下位 32bit を 0 にする。rdacw / rdacl は丸めの加算なし。
    マニュアルの Operation では最後に丸める前の tmp をマスクしていて、そのままだと丸めは飽和の判定にしか効かない。
    命令名（round）と RXv1 の racw に合わせて、丸めた値をマスクする実装にした（実機の RX72M と一致）。
- 既存の QEMU の誤り: racw のシフト量が 1 多かった（translate で imm + 1 にした値をヘルパーでさらに +1）。
  QEMU 8.2.2 で dsp-v1 を動かすと racw の最初のケースから失敗する。

テスト: `test/qemu/gen-dsp.py` がマニュアルの Operation を Python で書いたモデルから期待値を作り、
`dsp-v1.s`（RXv1: A0、64bit）と `dsp-v2.s`（A0 / A1、72bit）を生成する（生成物もコミットしている）。

```
python3 test/qemu/gen-dsp.py test/qemu                    # 生成し直すとき
scripts/run-qemu-test.sh dsp-v1 rx62n rxv1                # 266 チェック
scripts/run-qemu-test.sh dsp-v2 rxv2 rxv3 rxv3e           # 1512 チェック
```

- 積・積和・積差（12 命令 × 8 組の値、A0 / A1 交互、もう一方のアキュムレータが変わらないこと）、
  mvfac* のシフト 0〜2、mvtac*（ガードビットが立った状態に正負の値を書く）、rac* の飽和の境界（シフト 1 / 2）。
- dsp-v1 は RXv1 のコアでだけ実行する。RXv2 のコアでは 64bit を超える値の扱い（飽和・ガードビット）が違うので一致しない。
- `cpu-features` に、RXv1 のコアで A1・シフト付きの mvfac*・RXv2 の DSP 命令がトラップすることを追加した。

rx-rt の割り込み入口（`save-acc`、RXv2 以降）は mvtaclo → mvtachi → mvtacgu の順に戻すので、
ガードビットは最後の mvtacgu で戻る（mvtachi がガードビットを変えるかどうかに関係しない）。

## RXv3 の命令（Q4）

- `bfmov` / `bfmovz #slsb, #dlsb, #width, Rs, Rd`（`fc 5e` / `fc 5a` + #imm16）: Rs の bit slsb から width ビットを
  Rd の bit dlsb へ。bfmov は Rd の他のビットを残し、bfmovz は 0 にする。#imm16 は
  `((dlsb + width) & 1Fh) << 10 | dlsb << 5 | ((dlsb - slsb) & 1Fh)`（マニュアル 4 章の符号の注記。LLVM も同じ）。
  width は 1〜31、(slsb + width) と (dlsb + width) がともに 32 を超えると結果は未定義。
- `xor Rs, Rs2, Rd`（`ff 6x`）: RXv3 で 3 オペランドになった。Z / S を設定。
- レジスタ退避バンク（1.9 節）: `save` / `rstr`（#uimm8 または Rs でバンク番号）が R1〜R15、USP、FPSW、ACC0、ACC1 を
  バンクに保存・バンクから復帰する。特権命令。バンク数は製品ごと（0〜256）で、範囲外のバンクは未定義。
  - QEMU では CPU のプロパティ `banks`（既定は rxv3 / rxv3e で 16、RX72N・RX66N と同じ）。`banks=0` では
    save / rstr は未定義命令。範囲外のバンクは何もしない（ゲストエラーとしてログ）。バンクの内容はリセットで変えない
    （マニュアルではリセット後不定）。
  - rstr は FPSW の丸めモードも反映する。

テスト: `scripts/run-qemu-test.sh rxv3-insns rxv3 rxv3e`（62 チェック）

- bfmov / bfmovz の 14 通り（境界の bit 0 / 31、width 1 / 31、Rs = Rd）、xor の結果と Z / S、
- save / rstr: R1〜R15、USP、FPSW、ACC0 / ACC1（ガードビットを含む）を壊してから戻すこと、Rs で選んだ別のバンク、
  戻した FPSW の丸めモード（0 方向）で fdiv の結果が変わること、ユーザーモードで特権命令例外（ベクタ 20）になること。
- harness に特権命令例外のハンドラを追加（r13 = 4）。`cpu-features` に RXv3 の命令が RXv2 以前でトラップすることを追加。

## 倍精度 FPU（Q5）

- `dfpu`（rxv3 / rxv3e の既定）のコア: DR0〜DR15（64bit）、制御レジスタ DPSW / DCMR / DECNT / DEPC（DCR0〜3）、21 命令。
- 符号はマニュアル 4 章と LLVM（GNU as は ftod / itod / utod / mvtdc のレジスタ欄を入れ替えて出す）:
  演算 `76 90`、汎用レジスタとの転送 `fd 77` / `fd 75`、メモリ `fc 78〜7a` / `fc c8〜ca`（dsp の後に DR 番号のバイト、
  dsp は 8 倍）、即値 `f9 03`（`mov #imm, dsp8[rd]` のサイズ 3 の位置。QEMU の mov はサイズ 3 を 64bit の
  ストアとして実行していたので未定義にした）、dpushm / dpopm `75 a0〜b8`、mvfdr `75 90 1b`。
- 演算はマニュアルの Operation のとおり（dest = src2 op src、DCMPcm は RES = src2 cm src、MVFDR は Z = RES）。
  DPSW は FPSW と同じ配置で、例外の扱いも同じ（DDN、cause / flag、無効演算の結果 7FFFFFFFFFFFFFFFh、
  NaN の整数変換は符号で決まる）。NaN の伝搬は表 1.14（SNaN を優先、次に src2）。
- 例外: DEj = 1 の例外と未実装処理（DDN = 0 の非正規化数・アンダーフロー）では書き込み先を変えず、
  DEPC に命令のアドレス、DECNT.EHM なら EHS = 1。EHM と EHS がともに 1 の間は DPSW / DEPC を更新しない。
  マニュアルでは倍精度の例外は割り込みコントローラへの割り込み要求になるが、RX62N のボードには
  その要求先がないので割り込みは起きない（ログだけ）。
- dmov.l で上位 / 下位 32bit、dmov.d で上位に書くと下位は 0。dtoi / dtou / dround / dtof の上位 32bit は
  マニュアルでは不定、QEMU では変えない。dpopm.l はそのまま書き込み（cause フラグも立つ）、
  mvtdc は FPSW と同じく cause フラグは 0 にしかできず、DEPC は読み出し専用。

テスト:

```
scripts/run-qemu-test.sh dfpu-insns rxv3 rxv3e                       # 119 チェック
MCPU=rxv3 QEMU_CPU=rxv3 scripts/run-codegen-test.sh test/codegen/dfpu.ll test/codegen/dfpu_main.c
```

- dfpu-insns: dmov の全形式（メモリの dsp16 を含む）、四則・dsqrt、丸めモード（+ 方向、- 方向の x - x = -0）、
  NaN の規則、dcmp と mvfdr、変換、dabs / dneg がフラグを変えないこと、DDN = 0 の未実装処理と DEPC / DECNT
  （EHS 保持中は更新されない）、有効にした無効演算で書き込み先が変わらないこと、mvtdc / dpushm / dpopm。
- dfpu.ll（LLVM が出す DFPU の列）を dfpu_main.c（GCC のソフトウェア浮動小数点の double と比較）で実行:
  4000 チェックが -O0〜-O3 で通る。GNU as 経由のビルドでは ftod / itod / utod のレジスタを入れ替えて渡す
  （check-dfpu.sh と同じ）。DFPU のないコア（rx62n、`-cpu rxv3,dfpu=off`）では DFPU の命令がトラップする。

## RXv3e（Q6）

参照: RXv3e ソフトウェアマニュアル（R01US0745EJ0101、Rev.1.01）。`pdftotext -layout` で
`../ref/rxv3esm.txt` に変換して読んだ。NOTES-rxv3e.md の符号（未照合だったもの）はマニュアルの 4 章と一致した。

当初は「root ワールドだけ」の予定だったが、root で WID.BWID を書いて trrte を実行すると secure / non-secure に
入れる（ソフトウェアだけで他のワールドに行ける）ので、CPU 側のワールドを一通り実装した:

- WID（cr 15）: WID[1:0] = 現在のワールド、BWID、BITW、BWI。書けるのは root のスーパーバイザで BWID だけ。
  root 以外では BWID が 0 に読める。
- ワールドごとに 3 本ある制御レジスタ（ISP、INTB、BPC、BPSW、EXTB、IVC）: env の値が現在のワールドのもの、
  他のワールドの分は `world_cr` に置き、ワールドが変わるときに入れ替える。ワールドをまたがない例外・割り込みは
  現在のワールドの EXTB / INTB / ISP を使う（表 5.4）。
- 制御レジスタのアクセス権（表 1.1）: mvtc / mvfc / pushc / popc では FINTV は root だけ、WPRI は root と secure、
  IVC と WID の書き込みは root のスーパーバイザだけ（アクセスできないときは書かず、読むと 0）。
- IVC（cr 5）、WPRI（cr 14）、TRCTB と WINTB（trmvfc / trmvtc の cr 14 / 15、リセット値 80000000h）。
- 命令（符号はマニュアル 4 章）:
  - `trcall #imm`（`fd 75 00 imm`）: WID.BWI = PSW.I、BWID = WID、root へ。PSW と次の PC を ISP_TR に積み、
    TRCTB + imm × 4 へ。特権命令。どのワールドからでも実行できる。
  - `trrte`（`7f 97`）: SP_TR から PC と PSW を戻し、BWID のワールドへ（non-secure へは PSW の bit 28 を 0 に）。
    root 専用（他のワールドでは未定義命令）、特権命令。
  - `trmvfc cr, rd`（`fd 5a`）/ `trmvtc rs, cr`（`fd 58`）/ `trmvtc #imm, cr`（`fd 7x 2c`）: WID.BWID のワールドの
    ISP / INTB / BPC / BPSW / EXTB / IVC、または TRCTB / WINTB。root 専用、特権命令。BPSW_NS の bit 28 は常に 0、
    IVC_S / IVC_NS の NSIC / EXCSE は 0。
  - `int Rs`（`fd 75 1r 00`）: 下位 8 ビットが番号。
  - `mvtipl #imm5`（`75 70 imm5`。RXv1 の 4 ビット形式と互換）。
- PSW.IPL は 5 ビット（RXv3e 以外のコアと non-secure ワールドでは IPL[4] は 0。mvtc / popc / mvtipl /
  rte / rtfi / trrte とも）。
- IVC.UIR: ユーザーモードの int を制限（01 なら int #0〜#15 だけ、int Rs は不可。違反は特権命令例外）。

扱っていないもの（RX62N のボードに割り込みコントローラ・メモリ保護の対応がない）: ワールドをまたぐ割り込み
（WINTB、BITW、BWINM）、secure 例外、IVC.NSIC（non-secure で優先度 16 以上の割り込みを I = 0 でも受ける。
RX62N の ICU は優先度 15 まで）、WPRI の割り込み判定への反映。

RXv3e の命令（trcall / trrte / trmvfc / trmvtc / int Rs / 5 ビットの mvtipl）と IVC / WID / WPRI / TRCTB / WINTB は
llvm-mc（`-mcpu=rxv3e`）でアセンブルする。

テスト: `scripts/run-qemu-test.sh rxv3e-insns rxv3e`（43 チェック）

- リセット値、WPRI / IVC / WID の書ける範囲、5 ビットの IPL、
- trmvtc / trmvfc: BWID = 0 では root のレジスタ、2 では non-secure（root の値は変わらない）、1 では secure、
- int Rs、root からの trcall（ハンドラから trrte で戻る）、
- trrte で non-secure ワールドへ: WID = 2、R0 = ISP_NS、INTB_NS、FINTV / WPRI は 0 に読める、WID は書けない、
  root 専用命令は未定義命令例外（EXTB_NS・ISP_NS で処理）、IPL[4] が 0、int は INTB_NS、
- non-secure から trcall で root へ: WID = 80000200h（BWI、BWID）、ISP_TR・INTB・FINTV が戻ること、
- root のユーザーモード: trcall / trmvfc は特権命令例外、IVC.UIR = 01 で int Rs と int #20 は例外、int #3 は通る。
- `cpu-features`: RXv3e の命令が他のコアでトラップし、PSW.IPL[4] は RXv3e だけ。

## rx-rust の全テストを各 CPU で（Q7）

- `RX_CPU`（`scripts/cpu.sh`）で CPU モデルを 1 つ選ぶと、llc / GNU as の -mcpu、rustc の target-cpu、QEMU の -cpu が
  そろう。`rx62n` は RXv1 + FPU（GCC のテストドライバが FPU を使うため、RXv1 の実行はこのコア）、
  `rxv2` / `rxv3` / `rxv3e` はそのまま。RX_CPU を指定しなければ従来どおり（RXv1 のコード、QEMU の rx62n）。
- `run-codegen-test.sh` / `run-rust-test.sh` / `run-rust-rom.sh` が RX_CPU を受け付ける。Rust のビルドは
  `build/rust-test/<CPU>`、`build/rust-rom/<CPU>` に分けた（core を CPU ごとにビルドするため）。
  run-rust-rom.sh では RX_CPU のとき RUSTFLAGS が crate の `.cargo/config.toml` の rustflags を置き換えるので、
  リンカスクリプトの指定も渡す。
- **run-rust-rom.sh はこれまで PATH の qemu-system-rx（Ubuntu の 8.2.2）を使っていた**（qemu-run.sh と別に
  QEMU を起動していたため Q1 で切り替わっていなかった）。qemu-run.sh と同じく `../qemu/build/qemu-system-rx`
  を使い、QEMU 11 のモニタの 8 桁のアドレスも読めるようにした。
- `scripts/run-all-tests.sh [CPU...]`: アセンブラ（mc-diff の全ファイル）・check-dfpu・QEMU の命令テストを 1 回、
  そのあと CPU ごとに codegen（i32 / m3 / struct / atomic を -O0 / -O2、fpu を -O0〜-O3、rxv3 / rxv3e では
  dfpu も -O0〜-O3）と Rust（core-smoke、qemu-demo、isr-save を release / dev）。ログは `build/test-logs`。
- isr-save を広げた: RXv2 以降では A1（ガードビットを含む）、DFPU があれば DR0 / DR1・DPSW・DCMR を割り込みハンドラで
  壊し、rx-rt の割り込み入口が戻すことと、ハンドラが DPSW = 0x100 で始まることを確かめる
  （rx62n 19、rxv2 22、rxv3 29、rxv3e 34 チェック）。rxv3e では `asm!` から int Rs・5 ビットの mvtipl・
  WID / TRCTB / WPRI も使う（rustc の LLVM が RXv3e の命令を知っている必要がある）。

結果（2026-10-27）: 102 項目すべて通過。主なもの:

| | rx62n | rxv2 | rxv3 | rxv3e |
|---|---|---|---|---|
| i32 / m3 / struct / atomic（-O0, -O2） | 4131 / 3957 / 142 / 1080 | 同 | 同 | 同 |
| fpu.ll（-O0〜-O3） | 3110 | 3110 | 3110 | 3110 |
| dfpu.ll（-O0〜-O3） | — | — | 4000 | 4000 |
| core-smoke（release / dev） | 233 | 233 | 233 | 233 |
| qemu-demo | 8 | 8 | 8 | 8 |
| isr-save | 19 | 22 | 29 | 34 |

- rxv2 以降の Rust の core-smoke では atomic の CAS / fetch_* が movli / movco（14 か所）で実行される（rx62n では 0）。
- rxv3 / rxv3e では rx-rt の割り込み入口の dpushm / dpopm と A1・ガードビットの退避が実際に動く。

## 実機（AP-RX72M-0A）との比較

`test/qemu` の命令テストを、RX72M（RXv3 + DFPU）の評価ボード AP-RX72M-0A で E2 Lite と
e² studio の GDB サーバーを使って実行し、QEMU と違った点を直した（2026-10-27〜）。
テストは RAM（0x4000〜）にリンクし、EXTB を RAM の固定ベクタテーブルに向けて自由実行した。
実行方法（`scripts/hw-build.sh`、`scripts/hw-run.sh`）と最新の結果は [NOTES-hw.md](NOTES-hw.md)。

| 項目 | 実機の動作 | 対応 |
|---|---|---|
| RTE / RTFI でユーザーモードへ | PSW.U が 1 になり USP に切り替わる（マニュアル 1.4.2 のとおり） | QEMU は退避した PSW の U のままだった。修正。テストは `USER_MODE` / `SUPERVISOR_MODE`（harness.inc）で USP を用意し、スーパーバイザモードに戻ってから main を返す |
| MVTIPL の予約ビット | 即値バイトの bit 7〜4 は無視（14h で IPL 4、20h・80h で IPL 0）。例外にならない | QEMU は RXv3e 以外で 16 以上を未定義命令にしていた。RXv3e 以外は下位 4 ビット、RXv3e は下位 5 ビットを使うよう修正 |
| mvfac* の #imm の予約値（01b） | 例外にならず #2 と同じ | QEMU は未定義命令にしていた。#2 として扱うよう修正 |
| mvtachi | ガードビット（bit 71〜64）は変わらない（マニュアルの Function の記述のとおり） | QEMU と gen-dsp.py は Operation の式どおり、負の値でガードビットに OR していた。修正 |
| DN = 0 のアンダーフロー | 非実装処理（CE）に加えて CU・FU・FS も立つ（マニュアル 1.3.6「アンダーフロー例外が発生したとき」）。オペランドの非正規化数は CE だけ | QEMU は CE だけだった。修正 |
| ftou の負の数 | 負の正規化数はすべて無効演算（-0.5 も。マニュアルの補足説明の表のとおり）。結果は 0 | QEMU は -1 より大きい負の数を不正確（結果 0）にしていた。修正 |
| dmov.d の dsp | 4 倍（8[R7] は dsp 2、dsp:8 は 0〜1020、dsp:16 は 0〜262140、4 の倍数）。Renesas CC-RX V3.08 も同じ | マニュアルの注記は 8 倍で、GNU as・LLVM・QEMU は 8 倍だった。LLVM（llvm-project）と QEMU を修正。test/mc は CC-RX のバイト列と比べる。コード生成は dmov.d のメモリ形を使わないので、生成コードへの影響はない |
| DFPU 例外の保留中の DPSW | DECNT.EHM と EHS が 1 の間は mvtdc でも DPSW が書けない（マニュアル「DPSW is not updated」） | QEMU は mvtdc の書き込みを通していた。修正 |
| dpopm.l の DPSW | 原因フラグは mvtdc と同じく 0 にできるだけ（マニュアルは mvtdc についてしか書いていない） | QEMU はそのまま書いていた。修正（保留中は書かないのも同じ） |
| suntil / swhile のフラグ | C は比較 *R1 - R2 が符号なしで 0 以上（借りなし）、swhile の Z は全要素の一致（マニュアルどおり） | QEMU（上流由来）は C を *R1 <= R2、swhile の Z を R3 = 0 にしていた。修正。test/qemu/mem-insns.s で、コード生成が使い始めた [Rn+]・[Ri, Rb]・#imm からメモリ・メモリ間転送・ストリング命令を確かめる |
| racw / racl の丸め | 丸めた値の下位 32 ビットを 0 にする（dsp-v2 の rac* の 1512 チェック中の該当分がすべて一致） | 現在の実装のまま（前に「実機で確認したい点」としていた解釈が正しかった） |

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
