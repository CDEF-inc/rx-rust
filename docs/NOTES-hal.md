# RX72M の PAC と embedded-hal 実装のメモ

周辺機能のドライバ（HAL）を embedded-hal 1.0 のトレイトで実装するための方針と、その土台の PAC。

## 方針（2026-10-28）

- **embedded-hal**: crates.io の embedded-hal 1.0 と embedded-io 0.7（2026-11-20 から。それまでは
  CDEF-inc/embedded-hal の fork に git で依存していた）。
  embedded-hal、-nb、-async、-bus、embedded-io、-async、embedded-can はそのまま rx-none-elf でビルドできる。
  fork に依存すると、crates.io の embedded-hal とは別のクレートになる。HAL の型が実装するトレイトも別物になるので、
  embedded-hal-bus や crates.io のドライバ（センサなど）に HAL の型を渡すとコンパイルエラーになる。
  そのため HAL も試験も例も crates.io の版を使う。
  CDEF-inc/embedded-hal（上流と同じ内容）は、RX で問題が出たときの修正先として残す。直すときは、アプリの
  `[patch.crates-io]` で embedded-hal を fork に差し替える（版が同じ 1.0.0 なので 1 つにまとまる）。
- **PAC**: `crates/rx72-pac`。e² studio が入れる `RX72M.sfrx` からビルド時に生成する（下記）。
  生成物は Renesas の記述から作るので、リポジトリには入れない。
- **critical-section**: rx-rt の `critical-section` フィーチャー（PSW.I を保存して消し、戻す）。
- **HAL**: `crates/rx72-hal`。embedded-hal 1.0 のトレイトを実装する（下記）。

## チップ系列ごとの crate（2026-11-12、#165）

PAC と HAL は、周辺機能がほぼ同じチップをまとめて 1 crate にしている。チップは feature で 1 つだけ選ぶ
（どちらも選ばないか両方選ぶと、PAC の build.rs がエラーで止める）。

| crate | feature | 違い（M にだけあるもの） |
|---|---|---|
| `rx72-pac` / `rx72-hal` | `rx72m`（AP-RX72M-0A）、`rx72n` | ESC（EtherCAT スレーブ）、DSMIF、MSTPCRB.MSTPB11、MSTPCRD.MSTPD11、PACKCR.EPLLSEL、ELC.ELSR41〜44、ICU.PIARB、MPC.P70PFS |
| `rx74-pac` / `rx74-hal` | `rx74m`（EK-RX74M）、`rx74n`（EK-RX74N） | ESC、DSMIF、SYSC の ESCCKCR / ESCCKDIVCR / DSMIFCKCR / DSMIFCKDIVCR、MSTPCRC.MSTPC29、MSTPCRD.MSTPD8〜9、ELC.ELSR53〜60、POEG の DERRn、PSCU と CGFSAR の対応ビット |

e² studio の sfrx で比べると、どちらの組も M が N の上位集合で、共通のレジスタのアドレス・サイズはすべて同じ
（N にだけあるモジュール・レジスタ・ビットはない）。そこで PAC は、feature で読む sfrx（`RX72M.sfrx` /
`RX72N.sfrx`、`RX74M.sfrx` / `RX74N.sfrx`）を切り替える。HAL は ESC と DSMIF を使わないので、どちらの
feature でも同じコードになる。EK-RX74M のテストは今のところ `rx74n` でビルドしたものをそのまま動かしている
（RX74N のレジスタはすべて同じアドレスにある）。

ボードのテスト（test/rust/hw-*、hw-irq を除く）と examples/serial-echo も、ボードを feature で選ぶ:
`ap-rx72m`（AP-RX72M-0A）、`ek-rx74n`（EK-RX74N）、`ek-rx74m`（EK-RX74M）。テストは test/rust/hw-board に
依存し、そこにボードの HAL（`hw_board::hal`）、memory.x（`RX_MEMORY_X`、なければボードのもの）、結果の変数
（`checks` / `failures` / `first_failure` / `finished`）と `check` / `finish`、`_done`、panic handler がある。
両方のボードで動くテストは src/rx72.rs と src/rx74.rs に分かれ、src/main.rs が feature で片方を選ぶ（hw-pac は違いが小さいので 1 つのファイルで cfg）。
片方のボードだけのテスト（hw-uart は AP-RX72M-0A、hw-clock は EK-RX74N / EK-RX74M）は、
そのボードの feature だけを持つ。scripts/hw-build.sh は `HW_BOARD` から feature を決める（rx74m でも ek-rx74n）。

## PAC（crates/rx72-pac）

build.rs が次を行う。

1. `RX72M_SFRX`（`rx72n` では `RX72N_SFRX`、以下同じ）、なければ `~/.eclipse/com.renesas.platform_*/DebugComp/RX/IoFiles/RX72M.sfrx`
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
使う側で `[profile.*.package.rx72-pac] debug = false` にする（test/rust/hw-pac の Cargo.toml）。

確認: test/rust/hw-pac（ボード、scripts/hw-build.sh の rt グループ）が PAC で SYSTEM（PRCR、MSTPCRA）、
CMT、ICU を操作し、CMT0 の割り込みと critical_section::with を確かめる（11 チェック、release / dev とも通る）。

## PAC（crates/rx74-pac）

rx72-pac と同じ作りで、`RX74N.sfrx`（`rx74m` では `RX74M.sfrx`）から生成する。生成処理は `crates/sfrx-pac` に共通化した。
どちらも RX FSP のパックを e² studio に入れると `IoFiles` に置かれる（NOTES-hw.md）。`RX74N_SFRX` / `RX74M_SFRX` でも指定できる。

sfrx にはモジュール 283 個、レジスタ 33,277 個があり、半分は非セキュア側の別名（`_NS`）。リセット後はセキュアなので、`_NS` のモジュールは生成しない。
それでも生成されるソースは約 47 MB（CANFD が半分以上）で、ビルドに 3 分ほどかかる。

RX72M と名前が違う: SYSTEM は SYSC と MSTPREG に分かれ（`p.sysc.prcr_s()`、`p.mstpreg.mstpcrf()`）、ICU の IER は `ier3` のように 0 埋めがない。

確認: test/rust/hw-pac（ek-rx74n、EK-RX74N、rt グループ、11 チェック、release / dev とも通る）。

## EK-RX74N の配線（ボードのマニュアル R20UT5626EG0100 より）

| 用途 | 端子 | メモ |
|---|---|---|
| ユーザー LED | LED1（青）= PF05、LED2（緑）= P807、LED3（赤）= P604 | |
| ユーザースイッチ | SW1 = P414、SW2 = P109 | 押すと Low。SW3 はリセット |
| USB シリアル（J27） | P114 = TXD012-B、P115 = RXD012-B（RSCI12） | FT234XD。PC からは COM ポートに見える |
| メインクロック | 水晶 24 MHz（X1、EXTAL / XTAL） | MOMCR.MOSEL = 0（水晶）。外部クロック入力（MOSEL = 1）では発振しない |

ボードのマニュアルのジャンパ表では XTAL と X1 の間（E25）が Open と読めたが、水晶として動く。Renesas の RX FSP の
ボード設定（Renesas.RX_board_rx74n_ek パック）もメインクロックを水晶、PLL を 24 MHz / 3 x150 / 2 = 600 MHz にしている。

## HAL（crates/rx74-hal）

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
  いて、最初 1Fh（CLKOUT の行）と読み違えた）。API とテストは rx72-hal の `spi` と同じ（クロック同期式の 3 線、SPCMD0、
  8 ビット、MSB ファースト、`set_loopback`、`free`）。RX72M の RSPI との違い:
  - 制御レジスタは 32 ビット（SPCR、SPCMDm）、送受信は 4 段の FIFO。SPSR.SPTEF / SPRF は FIFO のしきい値で立ち、
    SPSCLR に 1 を書く（か DTC / DMAC）まで消えないので、FIFO の段数（SPTFSR.FREE、SPRFSR.FILL）を見る。
  - 動作クロック（TCLK）は SPICLK か PCLKA。`clock` は SPICLK を設定しないので、SPCR.SYNDIS = 1 で PCLKA にして
    同期化回路を飛ばす（47.3.19。8 ビットのフレームは PCLKA : TCLK = 1:1 で使える、表 47.17）。
  - SPDR はどの幅でも LSB 詰めで読み書きできるので、バイトで扱う。次アクセス遅延の既定は 1 RSPCK + 5 TCLK。

確認: test/rust/hw-spi の rx74.rs（rt グループ、29 チェック、release / dev とも通る。2026-11-08）。PLL1 480 MHz
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

確認: test/rust/hw-pwm の rx74.rs（rt グループ、22 チェック、release / dev とも通る。2026-11-08）。PLL1 480 MHz（PCLKD
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
  - ビジーループの待ち（立ち上がりの 2 us、温度センサの 30 us、スキャンの終わりから次の起動までの 6 PCLKA）は CPU の
    クロック CPUCLK0 から回数を決める。RX74N では CPU は ICLK ではなく CPUCLK0 で動く（480 MHz の設定で ICLK の 2 倍、
    分周しだいでもっと）ので、ICLK で数えると待ちが足りない（rx-rust#131 のレビューで指摘された）。
  - レジスタはセキュア側のアドレス（8743_8000h〜）で扱う。マニュアルの要約はサブエージェントに作らせ、
    アドレスとビットは行番号つきで照合した（表 60.62 の手順 3 は ADSYCR.ADSYDISm の 1 / 0 が逆に書かれている）。

- `i2c`: `I2c::ri3c0(p.ri3c0, pins.p102, pins.p103, Speed::Standard, &clocks)`。RX74N / RX74M に RIIC はないので、
  I3C バスインタフェース RI3C のチャネル 0（MSTPCRB.MSTPB4）を I2C で使う。端子は P102 = SCL00-A / P103 = SDA00-A
  （PSEL 11h、表 22.5）。EK-RX74M ではここに EtherCAT 用の EEPROM（U13、32 kbit、アドレス 50h）とプルアップが
  あり、EK-RX74N では空いている。embedded-hal の `I2c`（7 ビットアドレス、`transaction`）。
  - RI3C の I3C プロトコルモードはコマンドと応答のキューで動き、その legacy I2C は Fast-mode 以上だけ。I2C プロトコル
    モード（ICMR.OMS = 1、リセット値）は RIIC と同じくバイトごと（44.4.8）: ICCGR の ST / RS / SP、1 バイトの ICDR、
    ICCSR の TDRE / RDRF と ICSR2 の START / STOP / NACKF / TEND / AL / TMOF をポーリングする。ICSR2 と ICCSR の
    フラグは ICSER / ICCSER で検出を許可しないと立たない（リセット値は 0）。
  - バスのクロックは I3CCLK。クロック発生回路の I3CCKCR / I3CCKDIVCR（PRCR_S.PRC0、ADCCLK と同じ切り替え手順）で
    PLL1P を 60 MHz 以下にする（480 MHz で /8）。リセット値は MOCO。SCL の High / Low（ICFBR、I3CCLK / 2^CKS の
    クロック数 + ディジタルフィルタの 2）は計算で決め、I2C の規格の最小値に余裕を持たせる: Standard は 4.1 / 6.0 us
    （100 kHz 以下）、Fast は 0.7 / 1.8 us（400 kHz 以下）。マニュアルの表 44.4 の例は High が規格の最小値をわずかに
    下回る。ICBFTR はバスフリー時間（4.7 / 1.3 us）、ICICR はアナログとディジタル 2 段のフィルタ、SCL が止まれば
    ICTOR のタイムアウト（65536 クロック）で終わる。
  - 書き込み: TDRE はバイトがシフトレジスタに移るとすぐ立つので、前のバイトの ACK を待たずに次を書く。NACK のときは
    書いてあるバイトについて TDRE が立たない（44.5.5.2）ので、NACK を見つけた時点の TDRE で、NACK されたのが最後に
    書いたバイトか（TDRE = 1）、その前（TDRE = 0、1 バイト目ならアドレス）かを決める。アドレスだけの書き込み（プローブ）
    は完了のフラグがない（TEND はアドレスを除く）ので、SCL 10 周期待ってから NACKF を見る。
  - 読み出し: ICCSCR.WAITRE で 1 バイトごとに SCL を止め、ICDR を読むと次のバイトが始まる。最後のバイトを始める
    読み出しの前に ICACKR で NACK にし、最後のバイトを読む前に SP（または次の操作の RS）を出す。長さ 0 の読み出しは
    1 バイト読んで捨てる。エラーのあとはマスタのままなら STOP、そうでなければ ICRCR.ISRST で状態を戻す。
  - レジスタはセキュア側のアドレス（8745_F000h〜）で扱う。マニュアルの要約はサブエージェントに作らせた。
  - 初めの版は、アドレスの NACK をデータの NACK と取り違え、プローブの NACK を見落とした。後者は待ちのループが
    最適化で消えていたためで、ほかのドライバの待ちも同じだった（`core::hint::spin_loop` は RX では何もしない。
    rx-rust#133 で直した）。

確認: test/rust/hw-i2c-rx74n（rt グループ、22 チェック、release / dev とも EK-RX74M で通る。2026-11-08）。
- 設定前の P102 / P103 は High（プルアップ）。I3CCKCR = 05h、I3CCKDIVCR = 04h、MSTPB4、ICCR.ICE、
  ICRCCR = 1_0001h、ICFBR = 3F3F_79B2h、ICBFTR = 141、PmnPFS。
- 応答しないアドレス 33h への書き込み・読み出し・プローブがどれもアドレスの NACK になり、そのあとバスが空く（BFREE）。
- EEPROM（50h）が応答すれば（`eeprom=1`）: 0000h からの 16 バイト（アドレス 2 バイトの書き込み、反復スタート、読み出し）、
  1 / 2 / 4 バイトのランダムリードとカレントアドレスリードが一致、読み出し 2 つの transaction、Fast mode で同じ 16 バイト。
  16 バイトの読み出しは 100 kHz で 1.93 ms、400 kHz で 0.52 ms（171 ビット: それぞれ 88 kHz、330 kHz 相当）。
- EEPROM の中身は全部 FFh（未書き込み）なので、一致の確認はデータとしては弱い。書き込みは、ボードの EtherCAT の
  設定を書き換えないよう試していない。EK-RX74N（EEPROM なし）では `eeprom=0` で、そのチェックを飛ばす（未確認）。

確認: test/rust/hw-adc の rx74.rs（rt グループ、19 チェック、release / dev とも通る。2026-11-08、初めの 1 回で通った）。
配線なしで確かめる。PLL1 480 MHz、ADCLK 60 MHz。
- ADCCKCR = 05h、ADCCKDIVCR = 04h、MSTPD21、ユニット 1・2 のスリープ、ADCLKSR、ADCNVSTR = 06_0606h、
  ADSSTR0 = 300 / 60、キャリブレーションのエラーなし。
- 自己診断: 0001h / 8009h / 7FFFh（dev の 0 は FFFDh = −3）。
- 内部基準電圧: 992（VREFH0 = 3.3 V として 0.799 V、典型 0.80 V）。
- 温度センサ: TSCDR = 1382、TSCDR2 = 830（傾き 3.35 カウント / °C = 2.70 mV/°C、表 67.130 の典型 2.7）、変換値
  1051 で 26.1 °C。
- DA0 の内部出力を 400h / 800h / C00h にすると 400h / 800h / C00h（dev は 801h / C01h）。
- P400 の `into_analog`（ASEL、PMR / PDR / PCR = 0）と `read`、`set_sampling_states(120)`、`into_input` で ASEL が消える。

確認: test/rust/hw-hal の rx74.rs（rt グループ、10 チェック、release / dev とも通る。LED が順に光る）。
test/rust/hw-clock（rt グループ、20 チェック、480 MHz と feature `clock600` の 600 MHz、release / dev とも通る）は
設定後のレジスタに加えて、CAC でクロックを測る。メインクロック対 MOCO / 8192 は 23580（MOCO が 4% 速い）、
PLL1P / 16 対メインクロック / 8192 は 10239（480 MHz、計算は 10240）と 12799（600 MHz、12800）、PCLKB は 20479 と 25599。
CMT0 で測った 100 ms の遅延、CPU のループ（100 万回で 37.5 ms と 30 ms、比は 600 / 480）も確かめる。
examples/serial-echo の `ek-rx74n`（CPUCLK0 600 MHz）は `scripts/hw/serial-echo.ps1 -Port COM9` ですべて通った（1 行のエコー、
4096 バイトを 375 ms で取りこぼしなし、ブレークがフレーミングエラーになり、そのあともエコーが続く）。
hw-run.sh で動かしている間に実行する。MOCO 8 MHz のままでも通っていた（4096 バイトで 390 ms）。

CAC はメインクロックが発振していないと測定が終わらない（基準にしても対象にしても MENDF が立たない）。OSCSF.MOSCSF は
MOSCWTCR の待ち時間が過ぎたことを示すだけで、発振しているかどうかは分からない。外部クロック入力の設定で PLL を
メインクロックにつなぐと、PLLSF は立つが周波数が違う（PCLKB を MOCO / 8192 で測ると 12033。60 MHz なら 6 万近く）。

タイムアウトと panic（2026-11-20、#189。方針は rx72-hal と同じ、下の「タイムアウトと panic」）: `Config::try_configure` は
`ClockError`（`OutOfRange` / `NotAfterReset` は何も変えない。元の設定値を先に確かめるので分周比 0 でも 0 除算に
ならない。`MainOscillator` / `Pll` / `Mram` / `Voltage` は 100 ms 待っても準備ができない。待ちは MOCO で数える）。
UART の送信は 4 文字時間まで、SPI は `spi::Error::Timeout`、ADC は `adc::Error`（`Conversion` / `Timeout` /
`Calibration` / `Clock`、`try_adc16h`、キャリブレーションの失敗もエラー）、I2C は `try_ri3c0`（`SetupError`）、PWM は
`try_gptw8`。hw-clock に 2 回目の設定（`NotAfterReset`）と分周比 0（21〜23）、hw-adc に範囲外のサンプリング
ステート数（20、21）を足した。EK-RX74M で rt の 38 本と serial-echo（COM16）が通る。

## AP-RX72M-0A の配線（ボードのハードウェアマニュアル v6.0 より）

| 用途 | 端子 | メモ |
|---|---|---|
| モニタ LED LD1 / LD2 | PJ0 / PJ1 | High で点灯 |
| シリアル（CN8） | P12 = RXD2、P13 = TXD2（SCI2） | CMOS レベル。CN8 の 3・4 番（RTS・CTS）は短絡。折り返し試験は CN8 の 1・2 番をつなぐ |
| メインクロック | EXTAL / XTAL | 24 MHz 水晶（PLL で ICLK 240 MHz）。サブクロック 32.768 kHz |
| スイッチ | SW1 | リセットのみ。ユーザー用のスイッチはない |

## HAL（crates/rx72-hal）

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
  `into_open_drain_function(psel)` は同じうえで出力を N チャネルオープンドレインにする（ODR0 / ODR1、0x8C080 + 2 ×
  ポート、端子ごとに 2 ビット）。ほかの `into_*` は ODR を CMOS に戻す。
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

確認: test/rust/hw-pwm（ボード、rt グループ、22 チェック、release / dev とも通る。2026-11-08）。PJ1 を PIDR で読む
ループで測る。CMT0（PCLKB / 8 = 7.5 MHz）の 2 回の読み出しの間の時間を、先に読んだレベルの時間として足す
（サンプルの数で数えると、dev ではループ 1 回の時間がレベルで違うためデューティが 2〜3 % 低く出た）。
- `Timing::new`（5 通り）、MPC（PJ1PFS = 01h、PMR）、TCR = 20h、TCR2 = 1、TMDR1 = 2、TGRA = 59999、CST6。
- 1 kHz: デューティ 0 で High の時間が 0、最大で全部 High。25 / 50 / 75 % は 24.9 / 50.0 / 74.9 %（release）、
  24.9 / 49.9 / 75.0 %（dev）、100 ms に立ち上がりが 100 回。
- 100 kHz（1 分周、周期 1200）の 50 %: 49.8 % / 51.9 %、10 ms に立ち上がりが 1000 回。
- 終わったあとは 1 kHz・5 % で LD2 が薄く光る。

- `i2c`: `I2c::riic0(p.riic0, pins.p12, pins.p13, Speed::Standard, &clocks)`。RIIC0（MSTPCRB.MSTPB21）を
  P12 = SCL0 / P13 = SDA0（PSEL 0Fh、表 23.5）でマスタにし、embedded-hal の `I2c`（7 ビットアドレス、
  `transaction`）を実装する。AP-RX72M-0A ではこの 2 本がシリアル用の CN8 の 1 番（RXD2）と 2 番（TXD2）で、
  5 番が VDD、6 番が GND なので、I2C の機器は CN8 につなぐ。ボードの EEPROM（24LC16B）は SCI6 の P00 / P01
  （簡易 I2C）にあり、RIIC の端子にはつながっていない。RIIC1（P21 / P20、SD カード）と RIIC2（P16 / P17、USB）は
  コネクタに出ていない。
  - ICSR2 をポーリングする（43.3.3 / 43.3.4、ICIER = 0）。送信は 1 バイトずつ、前のバイトの ACK（TEND）か NACK
    （NACKF）を待ってから次を書くので、NACK がアドレスかデータか分かる。ICFER.NACKE で NACK のあとの転送は止まり、
    ストップコンディションを出す。受信は ICMR3.WAIT = 1（バイトごとに ICDRR を読むまで SCL を Low に保つ）で、最後の
    バイトの NACK（ACKWP を立てたうえで ACKBT）と、ストップコンディションまたはリスタートの要求を、SCL を放す前に
    書く。アドレスが NACK された読み出しは、SP のあとに ICDRR をダミーで読む（図 43.9。書き込みのあとでも害はないので、
    SP のあとは常に読む）。0 バイトの読み出しは 1 バイト
    読んで捨てる（最後のバイトは NACK するため）。
  - IICφ = PCLKB / 2^CKS。SCL の High / Low 幅は ICBRH / ICBRL + 1 サイクル（5 ビット、表 43.5 の式）。rx74-hal の
    RI3C と同じく、I2C の規格の最小値に余裕を持たせて計算で決める: Standard は 4.1 / 6.0 us（PCLKB 60 MHz で CKS = 4、
    ICBRH = 15、ICBRL = 22、96 kHz）、Fast は 0.7 / 1.8 us（CKS = 2、10、26、395 kHz）。表 43.5 の 60 MHz の値は
    規格の最小値ちょうど（Standard の High 4.0 us、Fast の Low 1.33 us）。ストップコンディションのあとのバスフリー時間は
    ICBRL の幅（43.10.3）で、4.7 / 1.3 us を満たす。ノイズフィルタ（1 段）と SCL 同期はリセット値のまま。
  - SCL が止まったときはタイムアウト（ICFER.TMOE、16 ビットのロングモード、TMOH / TMOL）で終わる。マスタでバスが
    ビジーのときと ST の要求中にだけ数える（43.11.1）ので、アイドルでは立たない。ST の前に BBSY が 20 ms ほど
    消えなければ `Error::Timeout`。エラーのあとはマスタならストップコンディション、だめなら RIIC リセット
    （ICE = 0 で IICRST）から設定し直す。内部リセット（ICE = 1 のまま IICRST）では BBSY が残り、SDA が Low のまま
    止まったバスではビジーが解けないため。CN8 に USB シリアルを挿したまま RIIC を動かすと（検出を外して確認）、SCL が
    アダプタの TXD で High に保たれて各転送がタイムアウト（約 17 ms）し、直後は SDA が RXD の弱いプルアップでゆっくり
    戻るので BBSY = 1 が見える（その後ストップコンディションとして検出されて消える）。
  - 初期化（図 43.4）: ICE = 0 で RIIC リセット、端子を RIIC に、ICE = 1 で内部リセットのまま ICSER = 0（リセット値は
    ジェネラルコールと SARL0 = 0 に応答する）、ICMR1、ICBRH / ICBRL（予約ビットは 1）、ICMR2、ICMR3.ACKWP、ICFER、
    ICIER = 0、IICRST = 0。ICCR1 には SOWP = 1 を書いて SCLO / SDAO を変えない。
  - 端子の PCR はそのまま残す。`into_pull_up_input()` した端子を渡すと内蔵プルアップが効く（RIIC が Low にしていない
    間は入力なので）。
- `sci_i2c`: `SciI2c::sci6(p.sci6, pins.p00, pins.p01, &clocks)`。SCI6（MSTPCRB.MSTPB25）の簡易 I2C モードで、
  P00 = SSDA6 / P01 = SSCL6（PSEL 0Ah、`into_open_drain_function` で N チャネルオープンドレイン）のボードの
  EEPROM（24LC16B、50h、JSW6 ON。プルアップはボード上）を読み書きする `I2c`（7 ビットアドレス、約 100 kHz）。
  - 42.7.4〜42.7.6 の手順（SIMR2.IICINTM = 1）をポーリングで: 開始 / 再開始 / 停止条件は SIMR3.IICSTIF、1 フレームの
    終わりは TXI6（IR087）、受信したバイトは RXI6（IR086）。割り込みは ICU で禁止したまま、`serial` と同じく IR を見る。
    送ったバイトの ACK は SISR.IICACKR。受信はダミーの FFh を TDR に書き、最後のバイトだけ SIMR2.IICACKT = 1（NACK）。
    0 バイトの読み出しは 1 バイト読んで捨てる。
  - SMR.CKS = 0、BRR = 18（60 MHz / (32 × 19) = 98.7 kHz。簡易 I2C の式 B = PCLK / (64 × 2^(2n−1) × (N+1))
    で 100 kHz を超えない最速）。表 42.26 から High / Low は周期の約 47 % / 53 % で Standard を満たす。
    SIMR1.IICDL = 18h（SSDA の遅延 23〜24 サイクル、約 0.4 us、SCL の立ち下がり 300 ns より長く）、SEMR.NFEN と
    SNFR.NFCS = 001b（ノイズフィルタ）、SIMR2.IICCSC（クロック同期）。
  - シングルマスタだけ（アービトレーションもバスビジーの検出もない）。待ちが ICLK / 1000 回のポーリング（PCLKB の
    レジスタを読むので 20 ms ほど）で終わらなければ `Error::Timeout`。エラーのあとは停止条件、だめなら 42.7.7（TE = RE = 0、SIMR3 = F0h、RDR を読む、TE = RE = 1）。

確認: test/rust/hw-i2c（ボード、rt グループ）。CN8 に何がつながっているかで RIIC の試験を変える（`i2c_bus`）。
SDA（P13）だけを GPIO で一瞬 Low にし、プルアップなしの入力にして、PIDR が High になるまでのポーリング回数を数える。
SCL（P12）は入力で読むだけで駆動しない（変換器の TXD が High を出しているので、Low を出すとぶつかる。P13 は変換器の
RXD で入力なので駆動してよい）。I2C のプルアップなら SDA はすぐ（2 回以内）戻って SCL は High、何もなければ SDA は
Low のまま。基準にボードの EEPROM のバス（P01 / P00）も同じように測ると 0 回だった。serial-echo の USB シリアル変換の
RXD のプルアップは弱く、SDA は 4〜9 回かかった（2026-11-18、release / dev で何度か）。RXD のプルアップが I2C 並みに強い
変換器だとバスと判定され、RIIC がタイムアウトまで TXD を Low に引く。
最初は両端子を 2 us だけ Low にしてから読んでいたが、変換器をつないだままで 2（バス）と判定して RIIC の試験が SCL の
タイムアウトで失敗した。次に両端子の回数で分けたが、SCL を Low にする時点で変換器の TXD とぶつかる（Codex のレビュー）
ので、SDA だけで判定するようにした。
- 0（それ以外。USB シリアル変換など）: バスの試験は飛ばす（RIIC が変換器の TXD を Low に引かないように）。
- 1（何もない）: 内蔵プルアップでアドレス 33h の NACK（書き込み、読み出し、0 バイトの書き込み）と、そのあとのバスフリー。
- 2（プルアップのあるバス）: 上の NACK、08h〜77h のスキャン、50h に 1 バイトアドレスの EEPROM（24C01〜24C16）が
  あれば（`eeprom=1`）EK-RX74M の hw-i2c と同じ読み出しだけの試験（16 バイト、ランダム / カレントアドレス読み出し、
  読み出し 2 つ、読み出し・書き込み・読み出しの transaction、Fast での同じ 16 バイトと時間）。ボードの 24LC16B を
  使うには、CN2 の 26 番（P01 / SSCL6）を CN8 の 1 番に、25 番（P00 / SSDA6）を 2 番につなぐ（CN2 は未実装の
  スルーホール。先頭 12 バイトは出荷時の MAC アドレス）。
- 毎回、SCI6 の簡易 I2C でボードの EEPROM を読む（`eeprom` の bit 1）: レジスタの設定、00h から 16 バイト（先頭
  12 バイトは MAC アドレスなので FFh ではない）、1 / 4 バイトのランダム読み出しの一致、33h のアドレス NACK、その後の
  カレントアドレス読み出し。
- 2026-11-18、AP-RX72M-0A（CN8 に USB シリアル変換をつないだまま、`i2c_bus=0`）: release / dev とも 15 確認すべて
  通る（RIIC の設定 8 と SCI6 の 7）。EEPROM の先頭 16 バイトは `00 0C 7B 4C 03 2A 00 0C 7B 4C 03 2B 7B 4C 03 2B`
  （MAC アドレス 00:0C:7B:4C:03:2A と :2B）。RIIC のバスの試験（CN8 に何もない場合とバスの場合）はまだ。

### UART の受信と相手機器との通信（2026-11-04）

CN8 に USB シリアル変換（FTDI、3.3 V、Windows では COM8）をつなぎ、examples/serial-echo（下記）と
`scripts/hw/serial-echo.ps1` で確かめた。デバッガ（E2 Lite）をつないで走らせている間に次がすべて通る。

- 送信: 起動時の 1 行が PC にそのまま届く。
- 受信: RXD2（P12、PSEL 0Ah）からの受信、RXI2 = IR062（ベクタ 62）、`Read::read` / `ReadReady::read_ready`。
  1 秒に 1 バイト（00h、FFh、55h、AAh など 14 個）はすべて届き、送り返した 1 行もそのまま戻る。
- 4096 バイト（00h〜FFh を 16 回）を続けて送ると、すべて順に戻る（360 ms、115200 bps で送るのにかかる時間と
  同じ。オーバーランなし）。
- ブレーク（20 ms）が `Error::Framing`（FER）になり、その後もエコーが続く。

デバッガを切り離したあとシリアルに何も出なかったのは、つながっている E2 Lite がセッションの終わりとリセットの
あとで MCU を止めていたため（rx-rust#187）。E2 Lite のケーブルをボードから外すと、リセットでも電源の入れ直しでも
起動し、SCI2 から 250 ms ごとに行を出す診断プログラムが CN8 に出し続けた（2026-11-18、docs/NOTES-hw.md の
「単体での起動」）。

まだ確かめていないこと:

- エコーそのものを E2 Lite なしで（単体で）。単体起動での SCI2 の送信は 2026-11-18 に確かめた（下記）。
- 2 バイト目以降のエラーを保持して次の `read` で返す処理（rx-rust#38）、オーバーランとパリティエラー。
- CN8 の 1・2 番を直結する hw-uart の折り返し（`loopback=1`）。

## 例: examples/blinky（2026-10-28、RX74 は 2026-11-18）

ボードの LED 2 つを 2 Hz で交互に点滅させる。ボードは feature で選ぶ（`ap-rx72m` は LD1 / LD2 = PJ0 / PJ1、
`ek-rx74n` / `ek-rx74m` は LED1 / LED2 = PF05 / P807）。点滅の処理 `blink` は embedded-hal 1.0 の
`StatefulOutputPin` / `OutputPin` / `DelayNs` だけで書いた汎用の関数で、`board::parts` が HAL の端子と CMT2 の
`Delay` を渡す（クロックは AP-RX72M-0A が PLL 240 MHz、EK-RX74N / EK-RX74M が CPUCLK0 600 MHz）。コードフラッシュ /
コード MRAM に置くので、E2 Lite のケーブルをボードから外せばリセットや電源の入れ直しで動く（NOTES-hw.md の
「単体での起動」。つないだままだとリセット後に止まる）。

- ビルド: `scripts/hw-build.sh examples`（`build/hw/examples/blinky.elf`、約 4.6 KB）
- 実行: `HW_TIMEOUT=10 scripts/hw-run.sh build/hw/examples/blinky.elf`。`_done` のないプログラムは
  HW_TIMEOUT 秒走らせる。最後の ELF ならデバッガを切り離すが、E2 Lite がつながっている間は MCU が止まる（run.py）。
  `HW_BOARD=rx74n` / `rx74m` では `build/hw-rx74n/examples/blinky.elf`。
- ボードで確かめたこと: 30 秒で `BLINKS` = 121（250 ms ごと）、PJ の PODR が 01h / 02h と交互になり、LD1 / LD2 が
  交互に点滅するのを目でも確かめた。2026-10-28 には切り離したあとも点滅し続けたと記録したが、2026-11-18 には
  切り離すと止まった（NOTES-hw.md）。
- EK-RX74M（2026-11-18）: hw-run.sh で書き込み、E2 Lite を J9 から外して SW3 を押すと、LED1 / LED2 が交互に
  点滅した。AP-RX72M-0A では feature 化したあとの版を実機で走らせていない（ビルドだけ）。

## 例: examples/serial-echo（2026-11-04）

ボードのシリアルポート（115200 bps 8N1）に届いたバイトをそのまま送り返す。ボードは feature で選ぶ（`ap-rx72m` は CN8 の SCI2、`ek-rx74n` / `ek-rx74m` は J27 の RSCI12。2026-11-12 に旧 examples/rx74n-serial-echo を統合、#165）。エコーの処理 `echo` は `embedded-io` の
`Read` / `Write` だけで書いた汎用の関数で、`board::serial` が HAL の `Serial`（AP-RX72M-0A は PLL 240 MHz、EK-RX74N / EK-RX74M は CPUCLK0 600 MHz）を作って渡す。
受信エラーは `! framing error` のような 1 行で知らせ、エコーを続ける。`RX_BYTES` / `RX_ERRORS` が数える。

- 配線: USB シリアル変換（3.3 V）の TXD を CN8 の 1 番（RXD2）、RXD を 2 番（TXD2）、GND を GND に。
- ビルド: `scripts/hw-build.sh examples`（`build/hw/examples/serial-echo.elf`）
- 実行: `HW_TIMEOUT=60 scripts/hw-run.sh build/hw/examples/serial-echo.elf` を走らせている間に、Windows で
  `powershell -File scripts/hw/serial-echo.ps1 -Port COM8`（1 行、4096 バイト、ブレークの 4 チェック）。
  単体で動かすには E2 Lite のケーブルを外してリセットする（上記）。

## 割り込みコントローラ: icu（2026-11-20、両 HAL）

割り込みで動くドライバと async（#188）の土台。`hal::icu::Interrupt` は要因（ベクタ番号と優先度レジスタ）で、
`enable` / `disable`（IERm.IENj）、`set_priority` / `priority`（IPRr）、`is_pending` / `clear_pending`（IRn）を持つ。
`trigger_swint` / `trigger_swint2` は SWINTR / SWINT2R でソフトウェア割り込みを起こす（実行器を起こすのに使う）。
ハンドラの登録はこれまでどおり rx-rt の `interrupt!(n, handler)`（n はリテラル）。

- ベクタ 128 未満では、IPR の番号はベクタ番号と同じとは限らない（RX72M のマニュアルの表 15.5、iodefine.h の
  `IPR_*`）。定数には正しい IPR を持たせた: SWINT2 / SWINT（26 / 27）は RX72M では IPR3 を共有し、RX74N / RX74M
  では IPR8 / IPR3。CMI0 / CMI1 / CMWI0 / CMWI1（28〜31）は両方で IPR4〜7。RX72M の RXI2 / TXI2（62 / 63）は
  IPR62 / 63。定数のない要因は `unsafe Interrupt::new(vector, ipr)`。
- 優先度は `Priority`（1〜15）。0 は要求が受け付けられないので作れない。
- アドレス: ICU は RX72M が 0008 7000h、RX74N / RX74M がセキュアの 8700 6000h。IRn = +n、IERm = +200h + m、
  SWINTR / SWINT2R = +2E0h / +2E1h、IPRr = +300h + r（両チップ共通）。IER の書き換えは critical-section の中。
- test/rust/hw-icu（20 チェック）: 優先度と IEN の読み戻し、IEN が 0 のあいだ SWINT は IR に残ること、許可すると
  受け付けて IR が消えること、PSW.IPL = 2 のあいだ優先度 2 の要求は待ち、3 にすると受け付けること、CMI0 を
  定数で許可して受け付けること、SWINT（優先度 3）のハンドラの中で CMI1 を優先度 7 なら受け付け、2 なら受け付け
  ないこと。定数の IPR が違うとその要因の優先度が 0 のままで受け付けられず、失敗する。クロックはリセット後の
  まま（RX72M は 240 kHz の LOCO）で、待ちは回数で区切る。AP-RX72M-0A と EK-RX74M（RX74N 用のビルド）で
  release / dev とも通る（2026-11-20）。

## 時刻: time（embassy-time のドライバ）と rx-rt の実行器（2026-11-20、両 HAL）

feature `embassy-time-driver` で `hal::time` が embassy-time のドライバになり、`embassy_time::Timer` / `Instant` /
async の `Delay`（embedded-hal-async の `DelayNs`）が使える。rx-rt の feature `executor` は embassy-executor の
実行器 `rx_rt::executor::Executor`（スレッドモード）で、実行できるタスクがないあいだ `wait` で眠る。

- 時刻の基準: CMTW0 を PCLKB / 8（RX72M 7.5 MHz、RX74N / RX74M は PCLKB 75 MHz で 9.375 MHz）、32 ビット、
  CMWCOR = FFFF FFFFh で回し、一周ごとの CMWI0（ベクタ 30）で周回を数えて 64 ビットにする。読むときは割り込みを
  止め、IR030 が立っていれば（ハンドラがまだ走っていない一周）1 周足して CMWCNT を読み直す。
- アラーム: CMTW1 を同じクロックで、アラームごとに止めて CMWCNT = 0、CMWCOR = 残りのカウント数にして動かす。
  CMWI1（ベクタ 31）でキュー（embassy-time-queue-utils の `Queue`、既定は embassy-executor と一体）の期限切れを
  起こし、次のアラームを設定する。2^32 カウント（約 7〜9 分）より先のアラームは途中で一度鳴り、残りを設定し直す。
- tick（embassy-time の `TICK_HZ`、既定 1 MHz）への換算は約分した比で正確に行う（9.375 MHz と 1 MHz なら
  75 カウント = 8 tick）。
- **CMWIOR.CMWE（bit 15）を 1 にしないとコンペアマッチがまったく起きない**（CMWI も、CMWCOR でのクリアもない）。
  最初はこれを立てず、CMTW1 のカウンタが CMWCOR を越えて回り続け、アラームが鳴らなかった。
- ハンドラは `hal::bind_time_interrupts!()`（rx-rt の `interrupt!` で 30 / 31 に `time::on_cmwi0` / `on_cmwi1`）。
  `time::init(p.cmtw0, p.cmtw1, &clocks, priority)` はクロック設定のあとに 1 回。モジュールストップは RX72M が
  MSTPCRA の bit 1 / 0、RX74N / RX74M が MSTPCRF の bit 13 / 12。
- 実行器: `__pender` がフラグを立て、ポーリングのあと割り込みを止めてフラグを見て、立っていなければ `wait`
  （PSW.I を立てて眠る 1 命令なので、そのあいだに来た起床も失われない）。embassy-executor の状態の更新は
  RXv2 以降の MOVLI / MOVCO（アトミック）になる。
- test/rust/hw-time（12 チェック）: CMT2 の `DelayNs` で 10 ms 待つと `Instant` も 10 ms 進む（換算の確認）、
  `Timer::after` の 1 / 100 / 1000 ms が 300 µs 以内の遅れで起きる、20 ms と 40 ms の 2 タスクがその順に起きる、
  CMTW0 を一周の 50 ms 前に進めて `Instant` が単調なまま一周をまたぐ、一周をまたぐ 100 ms のタイマが時間どおり
  起きる。EK-RX74M（RX74N 用のビルド）と AP-RX72M-0A で release / dev とも通る（2026-11-20）。

## 割り込みで動く UART: AsyncSerial（2026-11-20、rx74-hal の RSCI12、rx72-hal の SCI2）

feature `async` で `Serial::into_async(priority)` が `serial::AsyncSerial`（embedded-io-async の `Read` / `Write`）を
返す。ハンドラは `rx74_hal::bind_serial_interrupts!()`。

- RXI12 / TXI12 は RX74N / RX74M では選択型割り込みで、決まったベクタがない。SLIRn（ICU + 920h + 2 × (n − 144)、
  16 ビット）に要因の番号を書くとベクタ n に割り当たる（SLIXR128〜143 は ICU + 900h）。番号は FSP の
  `bsp_icu.h` の `ICU_EVENT_RSCI12_RXI` = 2ACh、`_TXI` = 2ADh。RXI12 をベクタ 144、TXI12 を 145 にした。
  ベクタ 128 以上の IR / IER / IPR はベクタ番号どおり（IPR144 など）。
- 受信: RXI12 のハンドラが RDR を 128 バイトのリングバッファに積み、読み手を起こす。満杯なら捨ててオーバーラン。
- 受信エラー（オーバーラン、フレーミング、パリティ）は RXI ではなく ERI12 で、グループ割り込み AL1（セキュア、
  ベクタ 105、IPR105、IER0Dh の bit 1）に入る。GENAL1_S（ICU + 7C4h）の EN7 で許可し、GRPAL1_S（+ 744h）の IS7 で
  見分ける。ハンドラはエラーを消して（RDR を読み、SSCR）次の `read` に返す。これがないと、オーバーランで受信が
  止まったまま読み手が起きない。
- 送信: TDRE が立っていれば TDR に書く。立っていなければ SCR0.TIE を立てて眠り、TXI12 のハンドラが TIE を下ろして
  起こす。TIE を立てたあとで TDRE をもう一度見る（そのあいだに空いた場合）。
- SCR0 の書き換え（TIE）は critical-section の中（ハンドラも書く）。
- examples/serial-echo-async（EK-RX74N / EK-RX74M）: AsyncSerial でエコーするタスクと、embassy-time で 100 ms ごとに
  数えるタスクを rx-rt の実行器で動かす。EK-RX74M で scripts/hw/serial-echo.ps1 -Port COM16 の 4 チェック（1 行、
  4096 バイトを全速で、ブレークがフレーミングエラーになる、そのあともエコーが続く）が 2 回とも通った（2026-11-20）。
  TICKS はまだ読んでいない（その後 E2 Lite が接続できなくなった）。
- rx72-hal の SCI2 も同じ API: RXI2 / TXI2 はベクタ 62 / 63（固定）、ERI2 はグループ割り込み BL0（ベクタ 110、
  GENBL0（0008 7670h）の EN5、GRPBL0（0008 7630h）の IS5）。ブロッキングのドライバが SCR.TIE を立てたまま IR063 を
  見る作りなので、TXI2 は TDR が空くたびに 1 回来る。ハンドラが「空いた」印を付けて書き手を起こす。切り替えのとき
  IR062 / IR063 は消さない（RDR に来ていたバイトと空いた TDR を、許可した時点でハンドラが受け取る）。
  AP-RX72M-0A で serial-echo.ps1 -Port COM8 の 4 チェックが通った（2026-11-20）。hw-time も AP-RX72M-0A で
  release / dev とも通る。
- E2 Lite が `Failed to initialise the emulator` で接続できなくなったことがある（EK-RX74M の例のあと）。USB の
  1 回目の挿し直しとボードの電源の確認では戻らず、もう一度 E2 Lite の USB を挿し直すと戻った。

## タイムアウトと panic（2026-11-20、#189、rx72-hal）

ハードウェアの異常（発振しない水晶、止まった周辺）で黙って止まらないように、待ちに上限を付け、設定の誤りを
`Result` で返せるようにした。

- 上限: `timeout::wait(spins, f)` が `f` を最大 `spins` 回呼ぶ。`spins(iclk, us)` は 1 回が 1 ICLK 以上かかる
  前提で、`us` マイクロ秒より短くならない回数（実際には数倍長い）。エラーは crate の `Timeout`。
- クロック: `Config::try_configure` が `ClockError`（`OutOfRange` は何も変えない、`MainOscillator` /
  `Pll` は 100 ms 待っても MOOVF / PLOVF が立たない。システムクロックは元のまま）。待ちの上限は、そのときの
  システムクロックの最大（SCKCR3 の源と SCKCR の分周: LOCO 264 kHz、HOCO 20 MHz、メイン 24 MHz、PLL 240 MHz）
  から計算する。`configure` は `try_configure(..).expect(..)`。
- UART: 送信（TXI2）と `flush`（TEND）の待ちは 4 文字時間まで、超えると `serial::Error::Timeout`
  （`ErrorKind::TimedOut`）。受信の `read` はデータが来るまで待つのが仕様なので上限はない（`ReadReady` か
  `AsyncSerial` を使う）。
- SPI: エラー型が `Infallible` から `spi::Error::Timeout` に（SPTEF / SPRF / IDLNF の待ちは 4 バイト時間まで）。
  `set_loopback` も `Result`。
- ADC: 変換（ADST）の待ちは 1 ms まで、超えたらスキャンを止めて `adc::Error::Timeout`。`read`、
  `self_diagnosis`、`read_internal_reference`、`TemperatureSensor::read_raw` / `read_millicelsius` は
  `Result`。5 µs のサンプリングが取れない ADCLK は panic ではなく `adc::Error::SamplingTime`。
  `set_sampling_states` は 5 未満で `OutOfRange`（何も変えない）。
- コンストラクタ: `Serial::try_sci2`、`Spi::try_rspi2`、`I2c::try_riic0`、`SciI2c::try_sci6`、
  `Pwm::try_mtu6`、`Adc::try_s12ad` / `try_s12ad1` が `OutOfRange` を返す。元の関数は `try_*(..).expect(..)`。
  エラーのときは渡した周辺と端子は触らずに捨てる。
- そのままのもの: I2C の転送は RIIC のタイムアウト検出（TMOF）と BBSY の回数制限で、もともと止まらない。
  `gpio::Pin::into_function` の PSEL の範囲（64 未満）はプログラムの誤りなので `assert` のまま。
- エラー型（`serial::Error`、`spi::Error`、`adc::Error`、`ClockError`）は `#[non_exhaustive]`。
- 試験: hw-hal に範囲外のクロック設定がエラーで何も変えないこと（18、19）、hw-adc に 5 未満のサンプリング
  ステート数がエラーで設定が残ること（30、31）を足した。AP-RX72M-0A で rt の 40 本がすべて通る（2026-11-20）。
  タイムアウトそのもの（水晶を外す、周辺を止める）はボードでは起こしていない。

## リセット要因とウォッチドッグ: reset、watchdog（2026-11-20、#190、両 HAL）

- `reset::cause()` がリセット要因のフラグを読み、`reset::clear()` が消す（1 を読んでから 0 を書く）。フラグは消すか
  パワーオンリセットまで残るので、起動直後に読んで消す。どれも立っていなければ RES# 端子（とデバッガ）のリセット。
  RX72M: RSTSR0（0008 C290h: PORF、LVD0RF〜LVD2RF、DPSRSTF）、RSTSR2（0008 00C0h: IWDTRF、WDTRF、SWRF）。
  RX74N / RX74M: RSTSR0（8701 9A40h: PORF、PVD0RF〜PVD2RF、PVD4RF / PVD5RF、DPSRSTF）、RSTSR1（8701 90C0h、32 ビット:
  IWDTRF、WDT0RF、SWRF、LM0RF、BUSRF、CMRF）、RSTSR3（8701 9A48h: CVMRF、TEMPRF）。名前のないフラグは `raw()`。
- コールドスタート / ウォームスタート: `set_warm_start` / `is_warm_start`（RX72M の RSTSR1.CWSF、RX74 の RSTSR2.CWSF。
  パワーオンリセットだけが消す）。
- `reset::software_reset()`: 割り込みを止め、PRCR（RX74 は PRCR_S）の PRC1 を立てて SWRR に A501h。
- `watchdog::Iwdt` / `Wdt`: レジスタスタートモード（OFS0 が既定の全ビット 1 のとき）。`start(p, config)` が
  IWDTCR / WDTCR（TOPS、CKS、RPES、RPSS）と IWDTRCR / WDTRCR（RSTIRQS: 1 ならリセット）を書き、最初の `feed`（リフレッシュ
  レジスタに 00h、FFh）でカウントが始まる。設定はリセット後に 1 回しか書けない。カウンタがもう動いている（0 でない）か、
  書いた値が読み返せなければ `StartError::Locked`（もう起動したか、OFS0 のオートスタートモード）。オートスタート
  モードでも IWDTCR / IWDTRCR は書いた値を読み返す（AP-RX72M-0A で確認）ので、読み返しだけでは分からない。止められない。
  - RX72M: IWDT は専用の 120 kHz、タイムアウトは 1024〜16384 カウント。WDT は PCLKB、1024〜16384。
    IWDT 専用のオシレータはリセット後に止まっている（ILOCOCR.ILCSTP = 1、マニュアル 9.2.11）ので、`Iwdt::start` が
    PRC0 を立てて ILCSTP を 0 にし、OSCOVFSR.ILCOVF を待つ（`StartError::Clock`）。止まったままだとカウンタは 0 の
    まま動かず、リセットも来ない（最初はこれで、0 のカウンタを `feed` の試験が素通りしていた）。
  - RX74N / RX74M: IWDT は 16384 Hz（FSP の `BSP_FEATURE_IWDT_CLOCK_FREQUENCY`）、タイムアウトは 128 / 512 / 1024 / 2048
    カウント（`IwdtTimeout`。FSP の `g_iwdt_timeout`）。リフレッシュ直後のカウンタが 7Fh 以下であることをボードで確かめた。
    WDT0 は PCLKB、1024〜16384。CKS の値は両チップで同じ（IWDT: 1 / 16 / 32 / 64 / 128 / 256 分周、WDT: 4〜8192 分周）。
- test/rust/hw-reset: 実際にリセットさせて 4 回走る（リセット要因で区別し、結果は rx-rt の `.uninit` に置いて最後に
  報告する）。① ランナーのリセット後: フラグを消し、ウォームスタートにしてソフトウェアリセット。② SWRF だけが立ち、
  ウォームスタートのまま。IWDT を最短のタイムアウトで起動し、半分で 20 回 `feed` してリセットされないこと、そのあと
  `feed` をやめる。③ IWDTRF。WDT で同じことをする。④ WDTRF。消してフラグがなくなること。カウンタが 0 でなく、
  減っていくことも確かめる（17、18）。16 チェック。E2 Lite でデバッグ中でも、どのリセットのあともそのまま走る。
  AP-RX72M-0A と EK-RX74M で release / dev とも通る（2026-11-20）。RX74M の IWDT は、起動のリフレッシュのあと
  IWDT のクロックで数サイクルのあいだカウンタが 0 と読め、それから 7Fh（128 カウント）から数える（試験は 0 でなく
  なるのを待って読む）。
- `ofs`（RX72M のみ）: オプション設定メモリ（FE7F 5D00h〜: MDE、OFS0、OFS1）。`Ofs0`（`iwdt_auto_start` /
  `iwdt_stop_in_sleep` / `wdt_auto_start`）、`Ofs1`（`lvd0_reset` / `hoco_at_reset`）を const で組み、
  `option_setting!(ofs0:, ofs1:)` が `.rx_option_setting` セクションに置く（memory.x に OFSM 領域が要る。
  test/hw/rx72m-memory.x）。プログラムと一緒にデバッガが書き、次に書くまで残る。オートスタートしたタイマは
  `Iwdt::running` / `Wdt::running` で受け取って `feed` する（`start` は `Locked`）。
  - 書かないもの: MDE は FFFF FFFFh 固定（リトルエンディアン、リニアモード）。ID コード（OSIS）、FAW、ROMCODE、SPCC、
    TMEF、BANKSEL には触れない（値を誤るとデバッガやフラッシュから締め出される）。RX74N / RX74M のオプション設定
    メモリ（OTP / セキュリティ領域）も扱わない。
  - hw-board: `own-option-setting` のない AP-RX72M-0A の試験は、消去値（全ビット 1）を置く。オプションを変えた試験の
    あとに走る試験が、それを引き継がないように。
  - test/rust/hw-ofs（AP-RX72M-0A のみ）: OFS0 で IWDT をオートスタート（256 分周、16384 カウント、約 35 秒）。
    読み返し（1〜4）、起動しなくてもカウンタが減る（5）、`start` が `Locked`（6）、`feed` で戻る（7）。release / dev
    とも通る（2026-11-20）。書き込む前のボードのオプション設定メモリは全部 FFh、hw-pac のあとも FFh に戻る。
- まだのもの: LVD（電圧監視。OFS1 の `lvd0_reset` はリセットの設定だけ）。

## DMA コントローラ: dma（2026-11-20、#188、両 HAL）

DMAC の 8 チャネルを、ノーマル転送モードで使う。`Dma::new(p.dmac)` がモジュールストップを解除し、DMAST.DMST = 1 に
して `ch0`〜`ch7`（`Channel<N>`）を返す。

- `Channel::copy(src, dst)`: スライスのコピー（u8 / u16 / u32、同じ長さで 1〜65535 個）。ソフトウェア起動
  （DMREQ.CLRS = 1 で SWREQ を立てたままにし、一度の要求で全部送る）。終わりを上限つきで待つ。長さが合わなければ
  `Error::Length`。
- `unsafe Channel::start(&Transfer)`: 任意の転送。アドレスは固定 / 増加 / 減少（DMAMD.SM / DM）、単位は 8 / 16 / 32
  ビット、起動はソフトウェアか割り込み要求（`Trigger::Interrupt(Interrupt)`: DMRSRn に起動元のベクタ番号を書き、
  DMTMD.DCTG = 01b。1 回の要求で 1 単位）。`is_done`（DMSTS.DTIF）、`remaining`（DMCRA）、`stop`（DTE を 0 にし、
  ACT が落ちるのを待つ）。
- 割り込みで起動する転送: DTE が 1 のあいだ、起動元の要求は CPU ではなく DMAC に行く。起動元の IER は 1 にしておく
  （0 だと DMAC にも届かない）。優先度は 0 でも DMAC には届く（hw-dma のチェック 19 で確かめた）。回数を終えると
  DTE が 0 になり、次の要求から CPU に行くので、起動元を止めるか、許可を外すか、ハンドラを置く。DMAC だけを
  起動する要因は `Interrupt::clear_priority` で優先度 0 のままにすれば、CPU には来ない。
- feature `async`: `dma::enable_interrupts(priority)` が転送終了割り込み DMAC0I〜DMAC3I（ベクタ 120〜123）と
  DMAC74I（124。チャネル 4〜7 で共有し、DMIST で見分ける）を許可する。ハンドラは `bind_dma_interrupts!()`。
  `Channel::done().await` は DMINT.DTIE を立てて待ち、ハンドラが DTIE を下ろして起こす（DTIF は future が見る）。
  `copy_async` は途中で future を捨てるとチャネルを止める。
- アドレス: RX72M は DMAC0 0008 2000h（チャネルごとに + 40h）、DMAST 0008 2200h、DMRSRn 0008 7400h + 4n、
  モジュールストップは MSTPCRA.MSTPA28（DTC と共通）。RX74N / RX74M はセキュアの DMAC0 8700 7000h、DMAST 8700 7800h、
  DMRSRn 8700 6400h + 4n、MSTPCRA.MSTPA22（FSP の `BSP_MSTP_BIT_FSP_IP_DMAC`）。DMCRB が 32 ビットで、DMCTL などが
  増えているが、ノーマル転送モードでは使わない。ベクタと IPR は両チップとも 120〜124。
- test/rust/hw-dma（19 チェック）: 8 / 16 / 32 ビットのコピー（チャネル 0、1、7）、長さの検査、固定の転送元での
  埋め、減少する転送元での逆順のコピー。CMT0 の CMI0（1 kHz）で 1 単位ずつ送る転送が 4 ms で半分ほど進み、
  そのあいだ CMI0 は CPU に来ない。`copy_async`（チャネル 3 = DMAC3I、5 = DMAC74I。速いので DTIE を立てる前に
  終わることもある）と、CMI0 で進むチャネル 3 / 4 の `done`（割り込みで終わる。rx-rt の実行器で眠って待つ）。
  CMI0 の優先度が 0 でも転送が進む（19）。AP-RX72M-0A と EK-RX74M（RX74N 用のビルド）で release / dev とも通る
  （2026-11-20）。
  - RX72M では、試験が PAC で MSTPCRA（CMT0）を書く前に PRCR.PRC1 を立てる。クロックの設定が PRCR を 0 に戻すので、
    そのままでは書き込みが無視され、CMT0 が動かず、CMI0 で進む転送が始まらなかった。

## 予定

1. ~~クロック設定~~、~~GPIO~~、~~MPC~~、~~遅延~~、~~UART（送信）~~（2026-10-28）、~~UART（受信）~~（2026-11-04、デバッガをつないだ状態。上記）
2. ~~SPI（RSPI）~~（2026-11-08、内部ループバック。CN9 に相手機器はまだつないでいない）、~~ADC（S12AD）~~（2026-11-08、自己診断・内部基準電圧・温度センサ。外部の電圧はまだ）、~~PWM（MTU）~~（2026-11-08、MTU6 を LD2 に。GPTW はまだ）、~~I2C（RIIC）~~（2026-11-17、RIIC0 を CN8 に。上記）、~~簡易 I2C（SCI6）~~（2026-11-18、ボードの EEPROM）
3. async（#188）: ~~ICU の API~~、~~embassy の time driver（CMTW0 / CMTW1）と実行器~~、~~割り込みで動く UART（RX74 の
   RSCI12、RX72M の SCI2）~~（2026-11-20、上記）。SPI / I2C の async、embedded-hal-nb の serial
