# RX72M の PAC と embedded-hal 実装のメモ

周辺機能のドライバ（HAL）を embedded-hal 1.0 のトレイトで実装するための方針と、その土台の PAC。

## 方針（2026-10-28）

- **embedded-hal**: CDEF-inc/embedded-hal（rust-embedded/embedded-hal の fork、上流と同じ内容）。
  embedded-hal、-nb、-async、-bus、embedded-io、-async、embedded-can はそのまま rx-none-elf でビルドできる。
  トレイトの定義だけのクレートなので、RX 用のコードは入れない。RX で問題が出たときの修正先にする。
- **PAC**: `crates/rx72m-pac`。e² studio が入れる `RX72M.sfrx` からビルド時に生成する（下記）。
  生成物は Renesas の記述から作るので、リポジトリには入れない。
- **critical-section**: rx-rt の `critical-section` フィーチャー（PSW.I を保存して消し、戻す）。
- **HAL**: `crates/rx72m-hal`。embedded-hal 1.0 のトレイトを実装する（下記）。

## PAC（crates/rx72m-pac）

build.rs が次を行う。

1. `RX72M_SFRX`、なければ `~/.eclipse/com.renesas.platform_*/DebugComp/RX/IoFiles/RX72M.sfrx`
   （WSL では `/mnt/c/Users/*/.eclipse/...` も）を探す。見つからなければ、e² studio を入れるか
   `RX72M_SFRX` を指定するよう求めてビルドを止める。
2. sfrx（モジュール 156 個、入れ子のモジュール 208 個、レジスタ 8441 個、ビットフィールド付き）を SVD にする。
   トップレベルのモジュールを 1 つのペリフェラルにし、ベースアドレスは最小のレジスタアドレス。入れ子のモジュール
   （CAN0/MB0 など）のレジスタはトップレベルに入れる（名前に MB0_ などが付いている）。同じアドレスのレジスタ
   （CAN の MCTLn_TX / MCTLn_RX）は alternateRegister にする。
3. svd2rust 0.37（ライブラリ、`Target::None`）で `OUT_DIR/pac.rs` を生成し、lib.rs が `include!` する。

sfrx の制約: すべて読み書き可、リセット値なし（`reset()` / `write()` は 0 から）、説明なし、割り込みベクタ
なし（rx-rt の `interrupt!` にハードウェアマニュアルのベクタ番号を書く）。実機にしかない動き（CMCR の bit 7）も
載っていない。生成されるソースは約 23 MB。デバッグ情報付きでコンパイルすると 10 分以上かかるので、
使う側で `[profile.*.package.rx72m-pac] debug = false` にする（test/rust/hw-pac の Cargo.toml）。

確認: test/rust/hw-pac（ボード、scripts/hw-build.sh の rt グループ）が PAC で SYSTEM（PRCR、MSTPCRA）、
CMT、ICU を操作し、CMT0 の割り込みと critical_section::with を確かめる（11 チェック、release / dev とも通る）。

## AP-RX72M-0A の配線（ボードのハードウェアマニュアル v6.0 より）

| 用途 | 端子 | メモ |
|---|---|---|
| モニタ LED LD1 / LD2 | PJ0 / PJ1 | High で点灯 |
| シリアル（CN8） | P12 = RXD2、P13 = TXD2（SCI2） | CMOS レベル。CN8 の 3・4 番（RTS・CTS）は短絡。折り返し試験は CN8 の 1・2 番をつなぐ |
| メインクロック | EXTAL / XTAL | 24 MHz 水晶（PLL で ICLK 240 MHz）。サブクロック 32.768 kHz |
| スイッチ | SW1 | リセットのみ。ユーザー用のスイッチはない |

## HAL（crates/rx72m-hal）

- `clock`: `Config::pll_240mhz_from_24mhz().configure(&p.system)` でメインクロック（水晶 24 MHz、MOFCR.MODRV2 =
  00b、MOSCWTCR = 53h）を起動し、MOOVF を待って MEMWAIT = 1（ICLK 120 MHz 超）、PLL x10（PLLCR.STC = 19）、
  PLOVF を待って SCKCR（ICLK 240 / PCLKA・BCLK 120 / PCLKB・PCLKC・PCLKD・FCLK 60 MHz）、SCKCR3.CKSEL = 100b。
  各クロックの周波数を `Clocks` で返す。上限を超える設定は panic。
- `gpio`: `Parts::take()` が 176 ピン版の全ポート端子（ボードのマニュアルの I/O 表の 137 本、ポート 0〜9、A〜G、J）
  を `Pin<PORT, BIT, MODE>` で 1 回だけ渡す。`into_push_pull_output` / `into_output(high)` / `into_input` /
  `into_pull_up_input`。`InputPin`（PIDR、どのモードでも）、`OutputPin` / `StatefulOutputPin`（PODR）。
  ポートのレジスタは全ポート同じ並びなので、アドレスで直接扱う。同じレジスタの読み書きは critical-section の中。
  周辺機能の端子設定（MPC の PFS）は、それを使うドライバで扱う。

確認: test/rust/hw-hal（ボード、rt グループ、17 チェック、release / dev とも通る）。
- PLL 240 MHz への切り替え後のレジスタ（CKSEL、MOOVF / PLOVF、MEMWAIT、STC、MODRV2、SCKCR）。
- ICLK: CMT0 の 1 周期（0.1 秒）の間のポーリングのループが 100 万回を超える（LOCO のままなら数千回）。
- PCLKB: CMT0（PCLKB / 512）の 20 周期 = 2.0 秒で LD1 / LD2 を点滅させ、hw-run.sh の実行時間が約 2.3 秒
  （PCLKB が違えば時間も変わる）。
- LD1 / LD2（PJ0 / PJ1）の出力と読み返し、PJ5（拡張コネクタだけにつながる）のプルアップ。
- PLL のまま終わったあとに書き込んだテスト（LOCO で動くもの）も通る（hw-run.sh の書き込み後のリセットで戻る）。

## 予定

1. ~~クロック設定~~、~~GPIO~~（2026-10-28）
2. MPC（PFS）の端子機能の設定
3. 遅延（`DelayNs`、CMT または CMTW）
4. UART（SCI2、`embedded-io` の Read / Write、`embedded-hal-nb` の serial）。CN8 の折り返しで確かめる
5. SPI（RSPI）、I2C（RIIC）、PWM（MTU / GPTW）、ADC（S12AD）
6. 必要なら async（embassy の time driver を CMT で）
