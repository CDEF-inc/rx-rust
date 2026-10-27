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
  `into_function(psel)` は MPC の PFS.PSEL を書き（PWPR の B0WI = 0、PFSWE = 1 で保護を外して戻す）、PMR = 1 にする。
  P35（NMI）は入力専用（PDR / PODR / PMR / PCR のビットがない）なので、`into_*` は `IoCapable`（P35 以外の
  136 本に実装）を持つ端子だけにあり、P35 は `InputPin` としてだけ使える（P35 の `into_*` はコンパイルエラー）。

確認: test/rust/hw-hal（ボード、rt グループ、17 チェック、release / dev とも通る）。
- PLL 240 MHz への切り替え後のレジスタ（CKSEL、MOOVF / PLOVF、MEMWAIT、STC、MODRV2、SCKCR）。
- ICLK: CMT0 の 1 周期（0.1 秒）の間のポーリングのループが 100 万回を超える（LOCO のままなら数千回）。
- PCLKB: CMT0（PCLKB / 512）の 20 周期 = 2.0 秒で LD1 / LD2 を点滅させ、hw-run.sh の実行時間が約 2.3 秒
  （PCLKB が違えば時間も変わる）。
- LD1 / LD2（PJ0 / PJ1）の出力と読み返し、PJ5（拡張コネクタだけにつながる）のプルアップ。
- PLL のまま終わったあとに書き込んだテスト（LOCO で動くもの）も通る（hw-run.sh の書き込み後のリセットで戻る）。

- `delay`: `Delay::new(p.cmt2, &clocks)`。CMT2（ユニット 1、MSTPCRA.MSTPA14）を PCLKB / 8 で回し続け
  （CMCOR = FFFFh）、CMCNT の差分で待つ `DelayNs`。CMT0 / CMT1 はアプリケーション用に空けておく。
- `serial`: `Serial::sci2(p.sci2, pins.p13, pins.p12, 115_200, &clocks)`。SCI2（MSTPCRB.MSTPB29）を CN8 の
  P13 = TXD2 / P12 = RXD2（PSEL 0Ah）で、8N1 の UART にする。ボーレートは SMR.CKS、SEMR.ABCS / BGDM、BRR から
  誤差の最も小さい組み合わせ（PCLKB 60 MHz で 115200 bps: ABCS = BGDM = 1、BRR = 64、115385 bps、+0.16%）。
  `embedded-io` の Read / Write / ReadReady / WriteReady。送受信の待ちは、Renesas の FIT ドライバと同じく SSR の
  bit 7 / 6 ではなく、TIE / RIE を立てて（ICU の割り込みは許可しない）IR063（TXI2）/ IR062（RXI2）を見る。
  RXD2 は内蔵プルアップを入れる（CN8 に何もつながっていないと RXD が浮いて FER が立つ。プルアップなしで SSR = 94h だった）。

確認: test/rust/hw-uart（ボード、rt グループ、15 チェック、release / dev とも通る）。
- ボーレートの選び方: PCLKB 60 MHz で 9600〜2000000 bps の 12 通りが、k・CKS・BRR の総当たりと同じ誤差になる
  （454546 bps は BRR = 16、441176 bps）。
- `delay_ms(5)` / `delay_us(100)` / `delay_ns(1000)` を CMT0（同じ PCLKB / 8）で計る: 37500 / 750 / 8 カウント以上で、
  余分はわずか。
- MPC: P12PFS / P13PFS の PSEL = 0Ah、PMR。TE と TIE を立てると IR063 だけが立つ（TXI2 はベクタ 63）。
- 1 行送って TEND。0 を送っている間に TXD（P13 の PIDR）が Low になり、終われば High（PSEL 0Ah で TXD2 が P13 に出る）。
- 受信エラーなし（RXD のプルアップ）。CN8 の 1・2 番（RXD・TXD）をつなぐと送った 6 バイトが戻ることを確かめる
  （つないでいないので `loopback=0`、受信と RXI2 = ベクタ 62 はまだ実機で確かめていない）。

### UART の未確認事項（2026-10-28 時点）

実機で確かめたのは送信側だけで、次は**未確認**（CN8 の 1・2 番をつないでいないため受信できない）。

- 受信そのもの: RXD2（P12、PSEL 0Ah）からの受信、RXI2 がベクタ 62（IR062）であること、`Read::read` /
  `ReadReady::read_ready` の動き。
- 受信エラーの扱い: ORER / FER / PER を検出して消し、エラーを返すこと。2 バイト目以降のエラーを保持して次の
  `read` で返すこと（rx-rust#38 のレビュー対応で入れた処理）。
- 他の機器との通信（USB シリアル変換などをつないで、実際に文字列が届くか）。送信は TXD 端子のレベルと TEND で
  確かめただけで、相手側で受け取ったことは見ていない。

確かめ方: CN8 の 1 番（RXD）と 2 番（TXD）をつないで `scripts/hw-build.sh rt` と `scripts/hw-run.sh
build/hw/rt/hw-uart-release.elf` を実行する。`loopback=1` になり、送った 6 バイトが戻ることを確かめる
（受信エラーの扱いは、このテストではまだ確かめない）。

## 例: examples/blinky（2026-10-28）

LD1 / LD2 を 2 Hz で交互に点滅させる。点滅の処理 `blink` は embedded-hal 1.0 の `StatefulOutputPin` /
`OutputPin` / `DelayNs` だけで書いた汎用の関数で、`main` が rx72m-hal の端子（PJ0 / PJ1）と CMT2 の `Delay`
を渡す（クロックは PLL 240 MHz）。コードフラッシュに置くので、デバッガを外したあともリセットや電源の入れ直しで動く。

- ビルド: `scripts/hw-build.sh examples`（`build/hw/examples/blinky.elf`、約 4.6 KB）
- 実行: `HW_TIMEOUT=10 scripts/hw-run.sh build/hw/examples/blinky.elf`。`_done` のないプログラムは
  HW_TIMEOUT 秒走らせ、最後の ELF ならデバッガを切り離して走らせたままにする（run.py）。
- ボードで確かめたこと: 30 秒で `BLINKS` = 121（250 ms ごと）、PJ の PODR が 01h / 02h と交互になり、LD1 / LD2 が
  交互に点滅するのを目でも確かめた。切り離したあとも点滅し続ける。

## 予定

1. ~~クロック設定~~、~~GPIO~~、~~MPC~~、~~遅延~~、~~UART（送信）~~（2026-10-28）。UART の受信は未確認（上記）。CN8 の折り返しで確かめる
2. SPI（RSPI）、I2C（RIIC）、PWM（MTU / GPTW）、ADC（S12AD）
3. 必要なら async（embassy の time driver を CMT で）、embedded-hal-nb の serial
