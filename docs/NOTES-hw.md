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
全 26 本で約 1 分 20 秒（書き込みを含む。実行時間は 1 本 5 秒以内）。失敗した命令テストは、
`build/hw/insns/NAME.fail` に失敗したチェックの番号・値・`bsr.a _check` のアドレスを最大 64 件書く。

| グループ | 内容 | 配置 |
|---|---|---|
| insns | test/qemu の命令テスト（cpu-features、fcmp-flags、fpu-status、rxv2-fpu、dsp-v2、dfpu-insns、rxv3-insns、mem-insns、cmt）。V2 = V3 = FPU = DFPU = 1、V3E = 0 でアセンブル | RAM（0x4000〜）、EXTB を RAM の固定ベクタテーブルに向ける |
| rom | rxv2-insns（EXTB を書き換えるので RAM 版の起動処理と両立しない） | コードフラッシュ、リセットベクタから起動 |
| codegen | test/codegen の i32、m3、struct、atomic、fpu、dfpu、mem（`llc -mcpu=rxv3`、-O0 と -O2）と GCC でコンパイルした C ドライバ | RAM |
| rust | test/rust/core-smoke（`-C target-cpu=rxv3`、release と dev） | RAM |

dsp-v1（RXv1 の DSP）と rxv3e-insns は RX72M の ISA と合わないので実行しない。

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

2026-10-28、rx-rust main（llvm-project ced046c、rust d482ce7）に test/qemu/cmt.s を足したもの。
hw-run.sh で 26 本、計 35,416 チェックがすべて失敗 0。
QEMU（CMT を直した CDEF-inc/qemu）の run-all-tests.sh も 116 / 116。

| グループ | テスト | チェック数 |
|---|---|---|
| insns | cpu-features 59、fcmp-flags 9、fpu-status 29、rxv2-fpu 40、dsp-v2 1512、dfpu-insns 128、rxv3-insns 64、mem-insns 84、cmt 13 | 1938 |
| rom | rxv2-insns | 16 |
| codegen（-O0 と -O2 のそれぞれ） | i32 4131、m3 3957、struct 142、atomic 1080、fpu 3110、dfpu 4000、mem 78 | 32996（2 回分） |
| rust | core-smoke release 233、dev 233 | 466 |

codegen と core-smoke のチェック数は QEMU のものと同じ。
