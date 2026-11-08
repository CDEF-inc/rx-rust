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
   トップレベルのモジュールを 1 つのペリフェラルにし、ベースアドレスは最小のレジスタアドレスを 4 の倍数に切り下げたもの。入れ子のモジュール
   （CAN0/MB0 など）のレジスタはトップレベルに入れる（名前に MB0_ などが付いている）。同じアドレスのレジスタ
   （CAN の MCTLn_TX / MCTLn_RX）は alternateRegister にする。
3. svd2rust 0.37（ライブラリ、`Target::None`）で `OUT_DIR/pac.rs` を生成し、lib.rs が `include!` する。

ベースを切り下げるのは、svd2rust のレジスタブロックが `#[repr(C)]` の構造体だから。ベースが 4 の倍数でないと、
レジスタのオフセットがそのサイズの倍数にならず、整列の詰め物でそのレジスタ以降が後ろにずれる。
切り下げる前は RX72M の MTU1 / MTU4 / MTU5 / MTU7 / TPU1 / TPU3 / TPU5（ベースが奇数アドレス）と
RX74N の SYSC / BUS（ベース 8701_9006h / 8701_D002h）がずれていた。RX74N の SYSC では SCKDIVCR 以降の
レジスタが 2 バイト後ろを指し、PRCR_S は 8701_93FCh になっていた（PLL の設定で見つけた）。

sfrx の制約: すべて読み書き可、リセット値なし（`reset()` / `write()` は 0 から）、説明なし、割り込みベクタ
なし（rx-rt の `interrupt!` にハードウェアマニュアルのベクタ番号を書く）。実機にしかない動き（CMCR の bit 7）も
載っていない。生成されるソースは約 23 MB。デバッグ情報付きでコンパイルすると 10 分以上かかるので、
使う側で `[profile.*.package.rx72m-pac] debug = false` にする（test/rust/hw-pac の Cargo.toml）。

確認: test/rust/hw-pac（ボード、scripts/hw-build.sh の rt グループ）が PAC で SYSTEM（PRCR、MSTPCRA）、
CMT、ICU を操作し、CMT0 の割り込みと critical_section::with を確かめる（11 チェック、release / dev とも通る）。

## PAC（crates/rx74n-pac）

rx72m-pac と同じ作りで、`RX74N.sfrx` から生成する。生成処理は `crates/sfrx-pac` に共通化した。
`RX74N.sfrx` は RX FSP のパックを e² studio に入れると `IoFiles` に置かれる（NOTES-hw.md）。`RX74N_SFRX` でも指定できる。

sfrx にはモジュール 283 個、レジスタ 33,277 個があり、半分は非セキュア側の別名（`_NS`）。リセット後はセキュアなので、`_NS` のモジュールは生成しない。
それでも生成されるソースは約 47 MB（CANFD が半分以上）で、ビルドに 3 分ほどかかる。

RX72M と名前が違う: SYSTEM は SYSC と MSTPREG に分かれ（`p.sysc.prcr_s()`、`p.mstpreg.mstpcrf()`）、ICU の IER は `ier3` のように 0 埋めがない。

確認: test/rust/hw-pac-rx74n（EK-RX74N、rt グループ、11 チェック、release / dev とも通る）。

## EK-RX74N の配線（ボードのマニュアル R20UT5626EG0100 より）

| 用途 | 端子 | メモ |
|---|---|---|
| ユーザー LED | LED1（青）= PF05、LED2（緑）= P807、LED3（赤）= P604 | |
| ユーザースイッチ | SW1 = P414、SW2 = P109 | 押すと Low。SW3 はリセット |
| USB シリアル（J27） | P114 = TXD012-B、P115 = RXD012-B（RSCI12） | FT234XD。PC からは COM ポートに見える |
| メインクロック | 水晶 24 MHz（X1、EXTAL / XTAL） | MOMCR.MOSEL = 0（水晶）。外部クロック入力（MOSEL = 1）では発振しない |

ボードのマニュアルのジャンパ表では XTAL と X1 の間（E25）が Open と読めたが、水晶として動く。Renesas の RX FSP の
ボード設定（Renesas.RX_board_rx74n_ek パック）もメインクロックを水晶、PLL を 24 MHz / 3 x150 / 2 = 600 MHz にしている。

## HAL（crates/rx74n-hal）

リセット後は MOCO 8 MHz で、分周はすべて 1、SCICLK も MOCO（`Clocks::after_reset()`）。全部 root ワールド（Flat-Project）で
セキュア側のアドレスを使う。

- `clock`: `Config::configure` がメインクロック（水晶）、PLL1、待ちサイクルを設定し、システムクロックを PLL1P にする。
  手順はハードウェアマニュアルの表 9.7 と図 65.14 のとおり: PRCR_S の PRC0 / PRC1 → PLL1LDOCR（リセット後は有効）→
  MOMCR / MOSCWTCR / MOSCCR と OSCSF.MOSCSF → PLLCCR / PLLCCR2 / PLLCR と OSCSF.PLLSF → MRCFREQ（MRICLK の MHz、
  切り上げ）と MREFREQ（MRPCLK）を書いて読み戻す → ICLK 150 MHz 超なら SRAMWTSC.WTEN → 600 MHz なら VSCR を VSCR_1 →
  SCKDIVCR、SCKDIVCR2 → SCKSCR = PLL1P → 30 us 待つ（DCDC モードの場合、図 9.14）。SCICLK は MOCO のまま。
  - `pll_480mhz_from_24mhz()`: PLL1 x40（VCO 960 MHz）/ 2。CPUCLK0 480 MHz、ICLK / MRICLK / PCLKE 240 MHz、
    MRPCLK / PCLKA / PCLKD / BCLK 120 MHz、PCLKB / PCLKC 60 MHz。リセット後の VSCR_2 のまま使える上限。
  - `pll_600mhz_from_24mhz()`: PLL1 / 3 x150（VCO 1200 MHz）/ 2。CPUCLK0 600 MHz、ICLK / MRICLK / PCLKE 300 MHz、
    MRPCLK / PCLKA / PCLKD / BCLK 150 MHz、PCLKB / PCLKC 75 MHz。DCDC モード（ボードは UMRECR = FFFF_FFFFh で
    DCDCCTL = 1）で VSCR_1 にする（表 67.52）。R5K574NxB4xx の最大。
  - `configure` は設定値をハードウェアマニュアルの制限（PLL の入力 8〜24 MHz、VCO 960〜2400 MHz、各クロックの最大、
    クロック間の N:1、3 の倍数の分周と 2 の累乗の分周を混ぜない）で検査し、外れていれば panic する。

- `gpio`: 320 ピン版の全ポート端子を `Parts::take()` で渡す（`pf05` のようにポートは 16 ビット、ビットは 2 桁）。端子のモードは RX72M と違い
  PmnPFS（`0x87C00800 + 0x40 * ポート + 4 * ビット`）にまとまっていて、PDR、PCR、PMR、PSEL をここで書く。保護は PWPR_S。
  データは `0x87C00000 + 0x20 * ポート`（PODR + 2、PIDR + 4、16 ビット）。
- `delay`: CMT2、PCLKB / 8。モジュールストップは MSTPCRF.MSTPF14。
- `serial`: RSCI12。RX72M の SCI とは別の周辺で、レジスタは 32 ビット。SSR の RDRF / TDRE をそのままポーリングできる
  （RX72M のように IR を見なくてよい）。エラーは SSCR で消す。FIFO は使わない。
  8 MHz で 115200 bps を出すにはビットレート補正が要る（ABCS = BGDM = 1、BRR = 7、MDDR = 236、115234 bps、+0.03%）。
- `spi`: `Spi::rspia0(p.rspia0, pins.pa05, pins.pa06, pins.pa07, MODE_0, 1_000_000, &clocks)`。RSPIA0
  （MSTPCRB.MSTPB19）を Arduino コネクタ J13 の PA05 = RSPCK0-B（D13）/ PA06 = MOSI0-B（D11）/ PA07 = MISO0-B（D12）
  でマスタにし、`SpiBus<u8>` を実装する。PSEL は 10h（ハードウェアマニュアル表 22.20。テキストに起こした表の段がずれて
  いて、最初 1Fh（CLKOUT の行）と読み違えた）。API とテストは rx72m-hal の `spi` と同じ（クロック同期式の 3 線、SPCMD0、
  8 ビット、MSB ファースト、`set_loopback`、`free`）。RX72M の RSPI との違い:
  - 制御レジスタは 32 ビット（SPCR、SPCMDm）、送受信は 4 段の FIFO。SPSR.SPTEF / SPRF は FIFO のしきい値で立ち、
    SPSCLR に 1 を書く（か DTC / DMAC）まで消えないので、FIFO の段数（SPTFSR.FREE、SPRFSR.FILL）を見る。
  - 動作クロック（TCLK）は SPICLK か PCLKA。`clock` は SPICLK を設定しないので、SPCR.SYNDIS = 1 で PCLKA にして
    同期化回路を飛ばす（47.3.19。8 ビットのフレームは PCLKA : TCLK = 1:1 で使える、表 47.17）。
  - SPDR はどの幅でも LSB 詰めで読み書きできるので、バイトで扱う。次アクセス遅延の既定は 1 RSPCK + 5 TCLK。

確認: test/rust/hw-spi-rx74n（rt グループ、29 チェック、release / dev とも通る。2026-11-08）。PLL1 480 MHz
（PCLKA 120 MHz）で、内容は RX72M の hw-spi と同じ（ビットレートの選び方、PmnPFS、SPCR = C100_0001h、
SPLP2 / SPLP のループバック、4 モードの 1 MHz と 30 MHz、アイドル時の RSPCK と MOSI、転送時間）。64 バイトは 1 MHz で
5046 カウント（673 us）、500 kHz で 10068 カウント（比 2.00）。D11 と D12 をつなぐと外部ループバックも確かめる。

- `pwm`: `Pwm::gptw8(p.gptw8, pins.pf05, 1_000, &clocks)`。GPTW8 をノコギリ波 PWM モード 1 にし、PF05 = GTIOC8B
  （PSEL 03h、表 22.25）から出す。PF05 はユーザー LED の LED1（緑）なので、RX72M の LD2 と同じく端子で測れる。
  `SetDutyCycle`。RX74N に MTU はなく、32 ビットの GPTW（GPTW0〜15）を使う。
  - カウントの基準（GTCLK）は PCLKD。リセット後の GTCLKCR.BPEN = 0 は非同期の GPTCLK（`clock` は設定しない）を
    選ぶので、GPTW のモジュールストップ（MSTPCRE.MSTPE31、全チャネル共通）を解除する前に BPEN = 1 にする（解除の
    あとは変えられない）。プリスケーラは GTCR.TPCS（GTCLK / 2^0〜2^10）のうち、周期が 65535 カウント以下に収まる最小のもの
    （`SetDutyCycle` の u16 に合わせる）。
  - GTCNT は 0 から GTPR まで数えて戻るので周期は GTPR + 1。GTIOR.GTIOB = 01001b（初期は Low、周期の終わりで High、
    GTCCRB のマッチで Low、表 24.4）で、周期の初めから GTCCRB カウント High。デューティ 0 と最大は
    GTUDDTYC.OBDTY（10b / 11b）で出す（RX72M の MTU のような TGR の工夫は要らない）。新しいデューティは GTCCRB の
    バッファの GTCCRE に書き（GTBER.CCRB = 01b）、周期の終わりに移るので半端なパルスは出ない。

確認: test/rust/hw-pwm-rx74n（rt グループ、21 チェック、release / dev とも通る。2026-11-08）。PLL1 480 MHz（PCLKD
120 MHz）で、内容は RX72M の hw-pwm と同じ（PF05 を PIDR で読み、CMT0 の時間で重みを付けて測る）。1 kHz の 25 / 50 /
75 % は 25.0 / 50.0 / 75.0 %（release）、24.9 / 49.9 / 74.9 %（dev）、100 ms に立ち上がり 100 回。100 kHz の 50 % は
50.0 % / 48.7 %、10 ms に 1000 回。初めの 1 回ですべて通った。

- `adc`: `Adc::adc16h(p.adc_b, &clocks)`。16 ビット A/D 変換器 ADC16H（PAC では ADC_B、MSTPCRD.MSTPD21）の
  ユニット 0 を SAR モードで 1 回ずつ変換する。RX72M の S12AD とは別物（ハードウェアマニュアル 60 章、14000 行）:
  - 変換クロック ADCLK は 25〜60 MHz で、PCLKA が 120 MHz 以上だと PCLKA を源にできない（表 67.52 の注 2）。
    クロック発生回路の ADCCLK（ADCCKCR / ADCCKDIVCR、PRCR_S.PRC0、9 章の切り替え手順: ADCCKSREQ → ADCCKSRDY を
    待つ → 分周と源 → ADCCKSREQ を戻す → ADCCKSRDY が消えるのを待つ）を PLL1P の 60 MHz 以下になる最小の分周
    （1、2、4、6、8、10、16、32）にし、ADCLKCR = 0（ADCCLK / 1）、ADCLKENR、ADCLKSR を待つ。PLL1 480 MHz で /8、
    600 MHz で /10、どちらも 60 MHz。PLL1 が止まっていれば panic。
  - 変換の前に自己キャリブレーションが要る（表 60.18）。使わないユニット 1・2 は ADUSLPCR1 / 2 でスリープにして
    （スリープのユニットはキャリブレーションしない）、ユニット 0 を ADCALSTR = 3（内部回路、ゲインとオフセット）で
    キャリブレーションし、ADCALENDSR を待って ADERSR.ADERF0 を見る（エラーなら panic）。ADCALSTCR.CALADCST は
    ADCNVSTR と同じ値にする。
  - 変換は「アナログチャネル → 仮想チャネル（ADCHCRn）→ スキャングループ → ユニット」の表を通す。読むたびに
    スキャングループ 0（ユニット 0、SAR シングルスキャン、ADSGCR0 と ADMDR はリセット値）の仮想チャネル 0 を
    変換したいものにし、ADSTR0 で起動して ADSCANENDSR を待つ。結果は AN 端子なら ADDRn（アナログチャネルの番号）、
    内部の信号源なら ADEXDRn（n = CNVCS − 60h）で、bit 31 の ERR は `ConversionError` にする。
  - SAR の結果は 12 ビット（それ以上は保証されない）なので、ADDOPCRC で 12 ビットの符号なし（ADPRC = 10b、
    SIGNSEL = 1。SAR では自己診断以外 SIGNSEL = 1 にする決まり）にする。リセット値のサンプリング（2 ステート）、
    逐次変換時間（ADCNVSTR、3 ステート）、SIGNSEL（0）はどれも SAR では使えない。逐次変換時間は 100 ns 以上
    （表 67.116: 100〜200 ns、60 MHz で 6）、サンプリングテーブル 0（AN 端子と自己診断）は 1 us、テーブル 1（内部の
    信号源: 4.15 us 以上、D/A の出力は 2 us 以上）は 5 us。`set_sampling_states` でテーブル 0 を変えられる
    （1 ADCLK + 40 ns 未満と 1023 超は panic）。
  - `self_diagnosis(DiagVoltage::Zero / Negative / Positive)`: CNVCS 60h を差動・16 ビット符号付きで変換し、
    ADSGDCR0.DIAGVAL（100b / 101b / 110b）の理想値は 0000h / 8000h / 7FFFh。
  - `read_internal_reference()`: ADREFCR.VDE、CNVCS 65h（0.77〜0.84 V）。`read_da(n)`: D/A 変換器 n の内部出力
    （CNVCS 68h / 69h、DACR0.DAOUTDIS = 1 で動かしておく）。D/A のドライバはまだなく、テストがレジスタを直接書く。
  - `TemperatureSensor::new(p.temps)`（MSTPCRD.MSTPD22）: 図 62.3 の手順（TSCR.TSEN、30 us、TSOE、CNVCS 64h）。
    校正値は TSCDR（Tj = 125 °C）と TSCDR2（−40 °C、どちらも AVCC0 = 3.3 V での 12 ビットの値）の 2 点で、
    `read_millicelsius` はその 2 点を結ぶ直線で温度にする（傾きの符号や典型値を使わない）。
  - `gpio` に `into_analog()`（PmnPFS.ASEL = bit 15）を足した。ADC が使う P400〜P407（AN000〜AN007、
    `AnalogCapable`）だけにあり、ほかのモードへの変更は ASEL を消す。ユニット 1・2 の AN008〜AN035 はまだ使えない。
  - レジスタはセキュア側のアドレス（8743_8000h〜）で扱う。マニュアルの要約はサブエージェントに作らせ、
    アドレスとビットは行番号つきで照合した（表 60.62 の手順 3 は ADSYCR.ADSYDISm の 1 / 0 が逆に書かれている）。

確認: test/rust/hw-adc-rx74n（rt グループ、19 チェック、release / dev とも通る。2026-11-08、初めの 1 回で通った）。
配線なしで確かめる。PLL1 480 MHz、ADCLK 60 MHz。
- ADCCKCR = 05h、ADCCKDIVCR = 04h、MSTPD21、ユニット 1・2 のスリープ、ADCLKSR、ADCNVSTR = 06_0606h、
  ADSSTR0 = 300 / 60、キャリブレーションのエラーなし。
- 自己診断: 0001h / 8009h / 7FFFh（dev の 0 は FFFDh = −3）。
- 内部基準電圧: 992（VREFH0 = 3.3 V として 0.799 V、典型 0.80 V）。
- 温度センサ: TSCDR = 1382、TSCDR2 = 830（傾き 3.35 カウント / °C = 2.70 mV/°C、表 67.130 の典型 2.7）、変換値
  1051 で 26.1 °C。
- DA0 の内部出力を 400h / 800h / C00h にすると 400h / 800h / C00h（dev は 801h / C01h）。
- P400 の `into_analog`（ASEL、PMR / PDR / PCR = 0）と `read`、`set_sampling_states(120)`、`into_input` で ASEL が消える。

確認: test/rust/hw-hal-rx74n（rt グループ、10 チェック、release / dev とも通る。LED が順に光る）。
test/rust/hw-clock-rx74n（rt グループ、20 チェック、480 MHz と `--cfg clock600` の 600 MHz、release / dev とも通る）は
設定後のレジスタに加えて、CAC でクロックを測る。メインクロック対 MOCO / 8192 は 23580（MOCO が 4% 速い）、
PLL1P / 16 対メインクロック / 8192 は 10239（480 MHz、計算は 10240）と 12799（600 MHz、12800）、PCLKB は 20479 と 25599。
CMT0 で測った 100 ms の遅延、CPU のループ（100 万回で 37.5 ms と 30 ms、比は 600 / 480）も確かめる。
examples/rx74n-serial-echo（CPUCLK0 600 MHz）は `scripts/hw/serial-echo.ps1 -Port COM9` ですべて通った（1 行のエコー、
4096 バイトを 375 ms で取りこぼしなし、ブレークがフレーミングエラーになり、そのあともエコーが続く）。
hw-run.sh で動かしている間に実行する。MOCO 8 MHz のままでも通っていた（4096 バイトで 390 ms）。

CAC はメインクロックが発振していないと測定が終わらない（基準にしても対象にしても MENDF が立たない）。OSCSF.MOSCSF は
MOSCWTCR の待ち時間が過ぎたことを示すだけで、発振しているかどうかは分からない。外部クロック入力の設定で PLL を
メインクロックにつなぐと、PLLSF は立つが周波数が違う（PCLKB を MOCO / 8192 で測ると 12033。60 MHz なら 6 万近く）。

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
  `into_analog()` は A/D のアナログ入力を持つ端子（`AnalogCapable`: P40〜P47 = AN000〜AN007、PE2〜PE7 = AN100〜AN105、
  PD6 / PD7 = AN106 / AN107、PD0〜PD5 = AN108〜AN113、P90〜P93 = AN114〜AN117、P00〜P02 = AN118〜AN120）を
  PMR = 0、PDR = 0、PCR = 0、PFS = 80h（ASEL）にする（I/O ポートの章の注意のとおり）。ほかの `into_*` は PFS.ASEL を
  読んで、立っていれば消す（ASEL のない端子では読むと 0）。

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

- `spi`: `Spi::rspi2(p.rspi2, pins.p56, pins.p54, pins.p55, MODE_0, 1_000_000, &clocks)`。RSPI2（MSTPCRC.MSTPC22）を
  無線モジュール用コネクタ CN9 の P56 = RSPCKC-B / P54 = MOSIC-B / P55 = MISOC-B（PSEL 0Dh）でマスタにし、
  embedded-hal の `SpiBus<u8>` を実装する。クロック同期式動作（SPCR.SPMS = 1、3 線）なので SSL 端子は使わず、
  CS はアプリケーションが GPIO で扱う（embedded-hal-bus の `ExclusiveDevice` など）。SPCMD0 だけ、1 フレーム 8 ビット、
  MSB ファースト、SPDR はバイトアクセス（SPDCR.SPBYT = 1、SPDR.HH = 000D_0304h）。遅延は既定のまま（RSPCK 遅延と
  SSL ネゲート遅延が 1 RSPCK、次アクセス遅延が 1 RSPCK + 2 PCLKA）。
  RSPI のクロックは PCLKA（120 MHz）で、ビットレートは PCLKA / (2 × (SPBR + 1) × 2^BRDV)。要求以下で最も高いものを選ぶ
  （30 MHz は SPBR = 1、1 MHz は SPBR = 59、最低は PCLKA / 4096 = 29.3 kHz）。
  SPTEF / SPRF をポーリングする（マニュアル 45.5.4 のとおり SPTIE / SPRIE = 0 で）。1 バイトずつ受信してから次を送るので
  受信バッファはオーバーランせず、エラーはない（`Error = Infallible`）。`read` は FFh を送る。MISO には内蔵プルアップを入れる。
  `set_loopback(Some(false))`（SPPCR.SPLP2、そのまま）/ `Some(true)`（SPLP、反転）で、配線なしにシフトレジスタの
  送信側を受信側につなげる（RSPCK と MOSI は端子にも出る）。

確認: test/rust/hw-spi（ボード、rt グループ、31 チェック、release / dev とも通る。2026-11-08）。
- ビットレートの選び方（7 通り）、MPC（P54〜P56 の PSEL = 0Dh、PMR、P55 の PCR）、SPCR = 49h。
- 内部ループバック: SPLP2 で 00h〜FFh がそのまま、SPLP で反転して戻る。`read` / `write` / `transfer`（長さの違う
  バッファ）/ `transfer_in_place`。4 つのモードで 1 MHz と 30 MHz。
- 端子: アイドル時の RSPCK（P56）が CPOL のとおり、MOSI（P54）が最後のビットのまま。
- 64 バイトの転送時間を CMT0（PCLKB / 8）で計る: 1 MHz で 8 ビット分（3840 カウント）以上、500 kHz でその 2 倍。
  ボードでは 5032 カウント（671 us、1 バイト約 10.5 RSPCK）と 10053 カウント（比 2.00）。
- CN9 の 3・4 番（MOSI・MISO）をつなぐと、内部ループバックなしでも戻ることを確かめる（`loopback=1`）。つないで
  いなければ MISO はプルアップで FFh を読む。

- `adc`: 12 ビット A/D 変換器（S12ADFa）。`Adc::s12ad(p.s12ad, &clocks)`（ユニット 0、AN000〜AN007、ADCLK = PCLKC、
  MSTPCRA.MSTPA17）と `Adc::s12ad1(p.s12ad1, &clocks)`（ユニット 1、AN100〜AN120、内部基準電圧、温度センサ、
  ADCLK = PCLKD、MSTPA16。ADSAM.SAM = 1 の中速変換にする。ADSAMPR で保護を外し、割り込み禁止で書く）。
  embedded-hal 1.0 には A/D のトレイトがないので、`read(&pin)` が `into_analog()` した端子（`Channel<Unit0 / Unit1>`）
  を 1 回変換して 12 ビットの値（右詰め）を返す。ソフトウェアトリガのシングルスキャンで、ADCSR.ADST が消えるのを待つ。
  2 つのユニットはレジスタの並びが同じ（0x89000 / 0x89100）なのでアドレスで直接扱う。PCLKB : ADCLK は 1、2、4、8 の
  どれか（それ以外は panic）。
  - サンプリング時間: リセット値の 11 ステート（60 MHz で 183 ns）は、電気的特性の表 65.55 / 65.56（信号源 1 kΩ）の
    条件より短い。初期化で全チャネル（ADSSTR0〜7 / 0〜15、ユニット 1 は ADSSTRL も）を表の条件にする: ユニット 0 は
    60 MHz で 16 ステート（267 ns）、ユニット 1 は 60 MHz で 38 ステート（633 ns）を ADCLK に合わせて切り上げ、最低 5
    （`rated_sampling_states()`。ユニット 1 は 30 MHz で 19、表の 30 MHz の条件は 15）。信号源のインピーダンスが高い
    ときは `set_sampling_states(n)` で延ばす（58.5。5 未満は panic）。（rx-rust#126 のレビューで指摘された）
  - `self_diagnosis(DiagVoltage::Zero / Half / Full)`: ADCER.DIAGM と DIAGLD（電圧固定モード）でチャネルを選ばずに
    変換し、ADRD の値と状態（bit 15〜14、どの電圧を変換したか）を返す。
  - `read_internal_reference()`（ユニット 1）: ADEXICR.OCSA、400 ns 待って変換し ADOCDR を返す。1.13〜1.23 V。
  - `TemperatureSensor::new(p.temps, p.tempsconst)`（MSTPCRB.MSTPB8）: `read_raw(&mut adc1)` は図 60.3 の手順
    （TSCR.TSEN、30 us、TSOE、ADEXICR.TSSA で変換、センサを止める）で ADTSDR を返す。`calibration()` は TSCDR
    （FE7F_7D7Ch、Tj = 125 °C、AVCC1 = 3.3 V での変換値）。`read_millicelsius` は TSCDR と典型の傾き 4 mV/°C から
    T = 125 + (raw − TSCDR) × 3.3 V / 4096 / 4 mV（AVCC1 = 3.3 V とみなす。1 点校正の誤差は ±7 °C ほど、図 60.2）。
  - 温度センサと内部基準電圧はサンプリング時間を 5 us 以上にする（58.2.18 の注。電気的特性の表 65.59 では温度センサは
    4.15 us）。ADSSTRT / ADSSTRO は 8 ビット（最大 255 ステート）なので、ADCLK = PCLKD が 51 MHz 以下でないと
    足りない。リセット後の 240 MHz の設定では PCLKD = 60 MHz（4.25 us まで）なので、使うときは `pclkd: Div::Div8`
    （30 MHz、150 ステート）にする。足りなければ panic する。

確認: test/rust/hw-adc（ボード、rt グループ、28 チェック、release / dev とも通る。2026-11-08）。配線なしで確かめる
（ボードの AN 端子は拡張コネクタにだけ出ていて、決まった電圧につながっているものはない）。PCLKD = 30 MHz。
- 自己診断: 両ユニットで 0 V が 0、1/2 が 2047、基準電圧が 4094〜4095。状態のビットも変換した電圧と合う。
- 内部基準電圧: 1467（1.182 V。AVCC1 = 3.3 V として）。ADSSTRO = 150。
- 温度センサ: TSCDR = 2001（125 °C で 1.612 V）、変換値 1498〜1502 で 24.7〜25.3 °C（室温）。ADSSTRT = 150、
  終わったあと TSCR = 0。
- サンプリング時間: ユニット 0 の ADSSTR0〜7 = 16、ユニット 1 の ADSSTR0〜15 と ADSSTRL = 19、
  `set_sampling_states(40)` のあとユニット 0 が 40。
- P40（AN000）/ PE2（AN100）の `into_analog`（PFS = 80h、PMR / PDR = 0）と `read`（何もつながっていないので値は
  見ない）、読んだあとのチャネル選択と ADST が 0、`into_input` で PFS.ASEL が消えること。

- `pwm`: `Pwm::mtu6(p.mtu6, pins.pj1, 1_000, &clocks)`。MTU6（MTU 全体のモジュールストップは MSTPCRA.MSTPA9）を
  PWM モード 1 にし、PJ1 = MTIOC6A（PSEL 01h）から出す。PJ1 はモニタ LED の LD2（High で点灯）なので、ボードの外に
  配線せずに端子で測れて、目でも明るさが分かる（MTIOC6B は PJ0 = LD1 だが、PWM モード 1 の出力は A と C）。
  embedded-hal の `SetDutyCycle`。
  - カウントクロックは PCLKA（120 MHz）の 1、2、4、8、16、32、64、256、1024 分周（TCR.TPSC と TCR2.TPSC2、表 24.9）
    のうち、周期が 65535 カウント以下に収まる最小のもの（デューティの刻みが最も細かい）。`Timing::new` が周波数に
    最も近い周期を選ぶ（1 kHz は 2 分周で 60000、50 Hz は 64 分周で 37500、1 MHz は 1 分周で 120。1.8 Hz〜60 MHz）。
  - TCR.CCLR = TGRA のコンペアマッチでクリアし、周期は TGRA + 1。TIORH は IOA = 0001b（A で Low）、IOB = 0010b
    （B で High）で、TGRB のマッチから TGRA のマッチまで（TGRA − TGRB カウント）High。`max_duty_cycle()` は周期の
    カウント数。デューティ 0 と最大は TIORH を A・B とも Low（0001b）または High（0110b）にし、TGRB = 0 にする。
    TGRB が TGRA と同じだとコンペアマッチで出力が変わらない（図 24.29）ので、初めのドライバは 100 % にしても Low の
    ままだった（hw-pwm で見つけた）。
  - TGRB はバッファしない（新しいデューティは次の TGRB のマッチから。書いた周期に 1 回、半端なパルスが出ることがある）。
    MTIOC6A は TOERB の対象ではない（MTIOC6B / 7A / 7B / 6D / 7C / 7D だけ）。TSTRB.CST6 は MTU7 と共用なので
    critical-section の中で書き換える。PAC の MTU のまとまりは不規則（MTU0〜2 のベースが同じなど）なので、MTU6 の
    レジスタはアドレス（000C_1A00h〜）で扱う。

確認: test/rust/hw-pwm（ボード、rt グループ、21 チェック、release / dev とも通る。2026-11-08）。PJ1 を PIDR で読む
ループで測る。CMT0（PCLKB / 8 = 7.5 MHz）の 2 回の読み出しの間の時間を、先に読んだレベルの時間として足す
（サンプルの数で数えると、dev ではループ 1 回の時間がレベルで違うためデューティが 2〜3 % 低く出た）。
- `Timing::new`（5 通り）、MPC（PJ1PFS = 01h、PMR）、TCR = 20h、TCR2 = 1、TMDR1 = 2、TGRA = 59999、CST6。
- 1 kHz: デューティ 0 で High の時間が 0、最大で全部 High。25 / 50 / 75 % は 24.9 / 50.0 / 74.9 %（release）、
  24.9 / 49.9 / 75.0 %（dev）、100 ms に立ち上がりが 100 回。
- 100 kHz（1 分周、周期 1200）の 50 %: 49.8 % / 51.9 %、10 ms に立ち上がりが 1000 回。
- 終わったあとは 1 kHz・5 % で LD2 が薄く光る。

### UART の受信と相手機器との通信（2026-11-04）

CN8 に USB シリアル変換（FTDI、3.3 V、Windows では COM8）をつなぎ、examples/serial-echo（下記）と
`scripts/hw/serial-echo.ps1` で確かめた。デバッガ（E2 Lite）をつないで走らせている間に次がすべて通る。

- 送信: 起動時の 1 行が PC にそのまま届く。
- 受信: RXD2（P12、PSEL 0Ah）からの受信、RXI2 = IR062（ベクタ 62）、`Read::read` / `ReadReady::read_ready`。
  1 秒に 1 バイト（00h、FFh、55h、AAh など 14 個）はすべて届き、送り返した 1 行もそのまま戻る。
- 4096 バイト（00h〜FFh を 16 回）を続けて送ると、すべて順に戻る（360 ms、115200 bps で送るのにかかる時間と
  同じ。オーバーランなし）。
- ブレーク（20 ms）が `Error::Framing`（FER）になり、その後もエコーが続く。

まだ確かめていないこと:

- **デバッガを切り離したあと**。`hw-run.sh` が切り離す（`D;1`）と、シリアルに何も出なくなる（ずっと行を
  出し続ける診断用のプログラムでも出ない）。E2 Lite をつないだまま SW1 でリセットしても同じだった。
  examples/blinky は切り離したあとも点滅し続けた（2026-10-28）ので原因は分かっていない。E2 Lite を外して
  電源を入れ直したときの動きはまだ見ていない。
- 2 バイト目以降のエラーを保持して次の `read` で返す処理（rx-rust#38）、オーバーランとパリティエラー。
- CN8 の 1・2 番を直結する hw-uart の折り返し（`loopback=1`）。

## 例: examples/blinky（2026-10-28）

LD1 / LD2 を 2 Hz で交互に点滅させる。点滅の処理 `blink` は embedded-hal 1.0 の `StatefulOutputPin` /
`OutputPin` / `DelayNs` だけで書いた汎用の関数で、`main` が rx72m-hal の端子（PJ0 / PJ1）と CMT2 の `Delay`
を渡す（クロックは PLL 240 MHz）。コードフラッシュに置くので、デバッガを外したあともリセットや電源の入れ直しで動く。

- ビルド: `scripts/hw-build.sh examples`（`build/hw/examples/blinky.elf`、約 4.6 KB）
- 実行: `HW_TIMEOUT=10 scripts/hw-run.sh build/hw/examples/blinky.elf`。`_done` のないプログラムは
  HW_TIMEOUT 秒走らせ、最後の ELF ならデバッガを切り離して走らせたままにする（run.py）。
- ボードで確かめたこと: 30 秒で `BLINKS` = 121（250 ms ごと）、PJ の PODR が 01h / 02h と交互になり、LD1 / LD2 が
  交互に点滅するのを目でも確かめた。切り離したあとも点滅し続ける。

## 例: examples/serial-echo（2026-11-04）

CN8（SCI2、115200 bps 8N1）に届いたバイトをそのまま送り返す。エコーの処理 `echo` は `embedded-io` の
`Read` / `Write` だけで書いた汎用の関数で、`main` が rx72m-hal の `Serial`（クロックは PLL 240 MHz）を渡す。
受信エラーは `! framing error` のような 1 行で知らせ、エコーを続ける。`RX_BYTES` / `RX_ERRORS` が数える。

- 配線: USB シリアル変換（3.3 V）の TXD を CN8 の 1 番（RXD2）、RXD を 2 番（TXD2）、GND を GND に。
- ビルド: `scripts/hw-build.sh examples`（`build/hw/examples/serial-echo.elf`）
- 実行: `HW_TIMEOUT=60 scripts/hw-run.sh build/hw/examples/serial-echo.elf` を走らせている間に、Windows で
  `powershell -File scripts/hw/serial-echo.ps1 -Port COM8`（1 行、4096 バイト、ブレークの 4 チェック）。
  切り離したあとは動かない（上記）。

## 予定

1. ~~クロック設定~~、~~GPIO~~、~~MPC~~、~~遅延~~、~~UART（送信）~~（2026-10-28）、~~UART（受信）~~（2026-11-04、デバッガをつないだ状態。上記）
2. ~~SPI（RSPI）~~（2026-11-08、内部ループバック。CN9 に相手機器はまだつないでいない）、~~ADC（S12AD）~~（2026-11-08、自己診断・内部基準電圧・温度センサ。外部の電圧はまだ）、~~PWM（MTU）~~（2026-11-08、MTU6 を LD2 に。GPTW はまだ）、I2C（RIIC）
3. 必要なら async（embassy の time driver を CMT で）、embedded-hal-nb の serial
