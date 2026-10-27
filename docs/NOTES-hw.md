# 実機（AP-RX72M-0A）でのテスト

QEMU の CPU モデルとコンパイラの出力を、実際の RXv3 で確かめるためのメモ。
2026-10-27〜28 に、ここにあるテストがすべて実機で通った（「結果」の表）。
実機と QEMU が違った点と、その修正は [NOTES-qemu.md](NOTES-qemu.md) の「実機（AP-RX72M-0A）との比較」にある。

## 機材

| | |
|---|---|
| ボード | Alpha Project AP-RX72M-0A（R5F572MNDDFC: RX72M、RXv3 + FPU + 倍精度 FPU、コードフラッシュ 4 MB、EXTAL 12 MHz） |
| デバッガ | Renesas E2 Lite（JTAG） |
| ホスト | Windows 11 の e² studio 2026-10（e2-server-gdb と同梱の Python 3 を使う）。テストのビルドは WSL2 の Ubuntu |

ボードのジャンパ（Alpha Project のアプリケーションノート AN1526 による）:

- JSW3 = SGL（シングルチップモード）
- JSW2 = SCI。工場出荷時の USB のままでは、デバッガが接続できない（電源の検出に失敗する）

e² studio で接続するときの設定（hw-run.sh も同じ値でサーバーを起動する）:
JTAG 6 MHz、EXTAL 12 MHz、レジスタ設定「On-chip ROM enabled extended mode」(1)、ID コードはすべて F。

## 使い方

```sh
# WSL: テストをビルドして build/hw に置く（insns、rom、codegen、rust、またはその一部）
scripts/hw-build.sh

# Windows の Git Bash: e2-server-gdb を起動して全部実行する（e² studio は切断しておく）
cd //wsl.localhost/Ubuntu/<rx-rust のパス>
scripts/hw-run.sh                                  # build/hw の全 ELF
scripts/hw-run.sh build/hw/insns/dsp-v2.elf        # 1 本だけ
```

出力は 1 テスト 1 行で、QEMU の `qemu-run.sh` と同じ変数（checks、failures、失敗があれば
first_failure、got、want）と実行時間を表示し、最後に `== N passed, M failed` を出す。
全 31 本で約 1 分 30 秒（書き込みを含む。実行時間は 1 本 5 秒以内）。失敗した命令テストは、
`build/hw/insns/NAME.fail` に失敗したチェックの番号・値・`bsr.a _check` のアドレスを最大 64 件書く。

| グループ | 内容 | 配置 |
|---|---|---|
| insns | test/qemu の命令テスト（cpu-features、fcmp-flags、fpu-status、rxv2-fpu、dsp-v2、dfpu-insns、rxv3-insns、mem-insns、cmt、memex-insns）。V2 = V3 = FPU = DFPU = 1、V3E = 0 でアセンブル | RAM（0x4000〜）、EXTB を RAM の固定ベクタテーブルに向ける |
| rom | rxv2-insns（EXTB を書き換えるので RAM 版の起動処理と両立しない） | コードフラッシュ、リセットベクタから起動 |
| codegen | test/codegen の i32、m3、struct、atomic、fpu、dfpu、mem（`llc -mcpu=rxv3`、-O0 と -O2）と GCC でコンパイルした C ドライバ | RAM |
| rust | test/rust/core-smoke（`-C target-cpu=rxv3`、release と dev） | RAM |
| rt | rx-rt のアプリケーション: test/rust/isr-save（ソフトウェア割り込みで FPSW・A0/A1・DR0/DR1・DPSW/DCMR の退避を確かめる）と test/rust/hw-irq（CMT0/CMT1 の割り込み、`interrupt::free`、IPL によるマスク、多重割り込み）、test/rust/hw-pac（rx72m-pac と rx-rt の critical-section、docs/NOTES-hal.md）。`-C target-cpu=rxv3`、release と dev | コードフラッシュ（test/hw/rx72m-memory.x）、rx-rt の `_rx_reset` からリセット起動 |

dsp-v1（RXv1 の DSP）と rxv3e-insns は RX72M の ISA と合わないので実行しない。

rx-rt のアプリケーションは `_start` を持たないので、run.py はリセットベクタから走らせ、`main` の最後で
呼ぶ `done` で止める（isr-save と hw-irq に `done` を置いた）。hw-irq は QEMU でも動く（CMT と ICU の該当
レジスタのアドレスが RX62N と同じ。run-all-tests.sh は QEMU の memory.x を RX_MEMORY_X で渡す）。

RX72M の割り込みで分かったこと:
- CMI0 / CMI1（ベクタ 28 / 29）の優先度は IPR004 / IPR005（0x87304 / 0x87305）。IPR はベクタ番号ごと
  （IPR028 = 0x8731C）ではなく、低いベクタでは周辺ごとに並ぶ（RX62N と同じ）。IPR が 0 だと IR が立っても
  受け付けない。
- CMT はリセット後モジュールストップ（MSTPCRA.MSTPA15）で、MSTPCRA は PRCR（A502h）で書き込みを許可してから書く。
- CMCR の bit 7 はコンペアマッチで 1 になり（CMIE が 0 でも）、CMCR を書くまで残る（止めても残る）。1 は書けない。
  QEMU にも入れた（test/qemu/cmt.s で確かめる）。

## ファイル

- `test/hw/start-ram.s`、`start-rom.s`: 起動処理。main の戻り値を `exit_code` に、終わったら
  `finished` に 1 を書いて `_done` でループする
- `test/hw/harness.s`: test/qemu/harness.s と同じ `_check` と例外の入口に、失敗の記録（`_faillog`）を足したもの
- `test/hw/rx72m-ram.ld`、`rx72m-rom.ld`: リンカスクリプト。デバッガが作業領域に 0x1000 付近と
  0x7fb40〜0x7ffff を使うので、RAM 版は 0x4000 から置き、スタックは 0x7f000 から
- `scripts/hw-build.sh`: ビルド（WSL）
- `scripts/hw-run.sh`: サーバーの起動と停止（Windows の Git Bash）
- `scripts/hw/run.py`、`scripts/hw/rsp.py`: GDB リモートプロトコルで ELF を書き込み、実行し、結果を読む

## e2-server-gdb の使い方で分かったこと

GDB は使わず、Python から直接リモートプロトコルを話す。e² studio と同じ手順でないと CPU が走らない。

- e² studio が渡すオプション（`-uSyncMode= async -uFirstGDB= main --gdbVersion= 16.2` など）が必要。
- `g` パケットは DFPU のレジスタ（DR0 以降）で失敗するので、レジスタは `p` で 1 つずつ読む。
  `p` の値はビッグエンディアンの 16 進、`P` と `c ADDR` はリトルエンディアンで渡す。
- クライアントが切断するとサーバーは終了する。
- 普通の `c` / `vCont;c` は、e² studio の起動シーケンスを送るまで何もしない。動いた順序（non-stop モード、QNonStop:1）:
  1. `monitor configuration_complete`
  2. `prg_download_start_on_connect` と `prg_download_end` の間でメモリに書き込む（コードフラッシュもこれで書ける）
  3. `monitor reset`
  4. `enable_stopped_notify_on_connect`、`enable_execute_on_connect`、`ignore_continue_during_connect`
  5. `vCont;c` を 1 回（無視される）
  6. `launch_complete`、`all_launches_complete`
  7. 以後は `vCont;c` で走る。止めるのは `_done` のハードウェアブレーク（Z1）か、`vCont;t:p1.1`。
     停止は `%Stop` 通知で届き、`vStopped` で応える
- 例外に入る命令をシングルステップすると PC がおかしくなる。自由実行なら例外は正しく処理される。
- クロックはリセット後のまま（テストはクロックの設定をしない）。
- e² studio もサーバーの起動前にポートを調べに接続する。プロキシでパケットを記録するときは、データのない接続を無視すること。

## 結果

2026-10-28、rx-rust main（llvm-project 417e313、rust 534e54e）に rt グループと cmt の CMCR bit 7 の
チェックを足したもの。hw-run.sh で 31 本、計 35,560 チェックがすべて失敗 0。
QEMU（CMCR bit 7 を入れた CDEF-inc/qemu）の run-all-tests.sh も 126 / 126（hw-irq を含む）。

| グループ | テスト | チェック数 |
|---|---|---|
| insns | cpu-features 59、fcmp-flags 9、fpu-status 29、rxv2-fpu 40、dsp-v2 1512、dfpu-insns 128、rxv3-insns 64、mem-insns 84、cmt 18、memex-insns 59 | 2002 |
| rom | rxv2-insns | 16 |
| codegen（-O0 と -O2 のそれぞれ） | i32 4131、m3 3957、struct 142、atomic 1080、fpu 3110、dfpu 4000、mem 78 | 32996（2 回分） |
| rust | core-smoke release 233、dev 233 | 466 |
| rt | isr-save release 29、dev 29、hw-irq release 11、dev 11 | 80 |

codegen と core-smoke のチェック数は QEMU のものと同じ。
