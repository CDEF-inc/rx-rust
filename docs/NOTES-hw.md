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
| codegen | test/codegen の i32、m3、struct、atomic、fpu、dfpu、mem、mulh、frame、jt（`llc -mcpu=rxv3`、-O0 と -O2）と GCC でコンパイルした C ドライバ | RAM |
| rust | test/rust/core-smoke（`-C target-cpu=rxv3`、release と dev） | RAM |
| rt | rx-rt のアプリケーション: test/rust/isr-save（ソフトウェア割り込みで FPSW・A0/A1・DR0/DR1・DPSW/DCMR の退避を確かめる）、test/rust/isr-bank（レジスタバンクの割り込み入口）と test/rust/hw-irq（CMT0/CMT1 の割り込み、`interrupt::free`、IPL によるマスク、多重割り込み）、test/rust/hw-pac（rx72m-pac と rx-rt の critical-section）、test/rust/hw-hal（rx72m-hal のクロックと GPIO、LED が点滅する）、test/rust/hw-uart（DelayNs、MPC、SCI2。docs/NOTES-hal.md）、test/rust/hw-spi（RSPI2 の SPI、内部ループバック）、test/rust/hw-adc（A/D 変換器の自己診断、内部基準電圧、温度センサ）、test/rust/hw-pwm（MTU6 の PWM を LD2 の端子で測る）。`-C target-cpu=rxv3`、release と dev | コードフラッシュ（test/hw/rx72m-memory.x）、rx-rt の `_rx_reset` からリセット起動 |

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
- `test/hw/rx74n-ram.ld`、`rx74n-rom.ld`、`rx74n-memory.x`: EK-RX74N のリンカスクリプトと rx-rt の `memory.x`
- `test/hw/rx72m-ram.ld`、`rx72m-rom.ld`: リンカスクリプト。デバッガが作業領域に 0x1000 付近と
  0x7fb40〜0x7ffff を使うので、RAM 版は 0x4000 から置き、スタックは 0x7f000 から
- `scripts/hw-build.sh`: ビルド（WSL）
- `scripts/hw-run.sh`: サーバーの起動と停止（Windows の Git Bash）
- `scripts/hw/run.py`、`scripts/hw/rsp.py`: GDB リモートプロトコルで ELF を書き込み、実行し、結果を読む

## EK-RX74N（RXv3e）

insns、rom、codegen、rust と rt のテスト（isr-save、isr-bank、alloc-smoke、float-smoke、hw-pac-rx74n、hw-hal-rx74n、hw-clock-rx74n）を実機の RXv3e で確かめられる。2026-11-07 に全 67 本が通った。examples の rx74n-serial-echo も J27 の USB シリアルで確かめた。

| | |
|---|---|
| ボード | Renesas EK-RX74N（R5K574NCB4AE、水晶 24 MHz） |
| デバッガ | E2 Lite を J9（14 ピン）につなぎ、SW6 は Pin1 ON / Pin2 OFF（Debug in）。電源は J27 の USB から取る（E2 Lite からは給電できない） |
| シリアル | J27（FT234XD、既定は RSCI12） |

e² studio 2026-10 の E2 用デバイス一覧に RX74N はなく、最初は `No matching device found` になる。
[RX FSP](https://github.com/renesas/rx-fsp/releases) の `RX_FSP_Packs_v1.0.0.zip` を e² studio のサポートフォルダ
（`~/.eclipse/com.renesas.platform_<番号>`）に展開し、e² studio を再起動して RX の新規プロジェクト画面を開くと、
`DebugComp/RX/RX/E2_v3.3.5` に配置される。再起動だけでは配置されない。`E2DeviceList.xml` は変わらないが、接続できるようになる。

```sh
HW_BOARD=rx74n scripts/hw-build.sh     # WSL: build/hw-rx74n に置く
HW_BOARD=rx74n scripts/hw-run.sh       # Windows の Git Bash
```

RX72M との違い:

- アドレスの bit 31 でセキュア側（`0x80000000` 以上）と非セキュア側に分かれ、同じ資源が両方に見える。リセット後の RAM はセキュアなので、
  デバッガからは `0x82000000` では読めるが `0x02000000` では `The specified address is in the secure area` で失敗する
  （ハードウェアマニュアル R01UH1056EJ0110 の 4.1、56.3、56.4）。リセット後は root ワールド（Flat-Project の動作）で全空間にアクセスできるので、
  Protect Zone の設定は要らない。`test/hw/rx74n-ram.ld` は On-chip RAM のセキュア側（`0x82000000`〜）に置く。e² studio のリンカスクリプトも同じ。
- `rxv3e-insns` は `ROOT_ONLY` でアセンブルし、root ワールドの部分だけを実行する（27 チェック）。`trrte` で non-secure ワールドへ移ると、
  セキュアなメモリのコードを実行できず止まる。QEMU にはこの分離がないので全部通る。
- コード MRAM は R5K574NC で 1 MB（`0xFFF00000`〜）。RAM と同じ手順で書き込める。`test/hw/rx74n-rom.ld`、`rx74n-memory.x` の変数は On-chip RAM のセキュア側に置く。
- 周辺のアドレスが違うので、`cmt` と rt の hw-irq、hw-hal、hw-uart、hw-spi、hw-adc、hw-pwm、RX72M の examples は実行しない。
  hw-pac の代わりに、CMT0 の割り込みと critical-section を rx74n-pac で確かめる test/rust/hw-pac-rx74n がある（11 チェック）。
  CMI0 はベクタ 28、IPR4、IER3.IEN4 で RX72M と同じだが、CMT のモジュールストップは MSTPCRF.MSTPF15、保護は PRCR_S。
- 一度、`FFWRXCmd_AuthKey` の JTAG 通信エラーで接続できなくなった（JTAG クロックを下げても同じ）。SW6 を Pin1 ON / Pin2 OFF にして
  J27 を抜き差しすると戻った。どちらが効いたかは切り分けていない。

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

2026-11-03、rx-rust#72 のブランチ（lld の RX 対応）で、`RX_LD=build/llvm/bin/ld.lld scripts/hw-build.sh`
（insns rom codegen rust rt を lld でリンク）の 57 本、計 170,024 チェックがすべて失敗 0。GNU ld でリンクした
rom（rxv2-insns）と hw-irq も、rx72m-rom.ld の変更後に通ることを確かめた。

2026-11-02、rx-rust#66 のブランチ（llvm-project d1b19fc、rust f2be09e。epilogue の CFI、jump table、emul / emulu、
FPU のメモリオペランドと 3 オペランド形式、DFPU の DR 常駐と変換を含む）に、codegen の mulh / frame / jt を足したもの
（rx-rust #57）。hw-run.sh で 43 本、計 97,650 チェックが 2 回ともすべて失敗 0。

| グループ | テスト | チェック数 |
|---|---|---|
| insns | 下の 2026-10-28 と同じ 10 本 | 2002 |
| rom | rxv2-insns | 16 |
| codegen（-O0 と -O2 のそれぞれ） | i32 4131、m3 3957、struct 142、atomic 1080、fpu 4244、dfpu 7762、mem 78、mulh 23438、frame 104、jt 2564 | 95000（2 回分） |
| rust | core-smoke release 233、dev 233 | 466 |
| rt | isr-save 29、hw-irq 11、hw-pac 11、hw-hal 17、hw-uart 15（それぞれ release と dev） | 166 |

- mulh は emul / emulu（64 ビット積、mulhu / mulhs、定数除算の掛け算列）を含み、QEMU と同じ 23,438 チェック。
- hw-hal（release）のチェック 14（PJ5 のプルアップを有効にした直後に読むと high）が毎回失敗した。release では PCR の
  書き込みから PIDR の読み出しまで数命令（240 MHz で十数 ns）しかなく、プルアップがピンを充電する前に読んでいた
  （dev は遅いので通る）。テストに 1000 回の volatile 読み出しの待ちを入れて、3 回とも通ることを確かめた。

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
