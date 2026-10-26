# M5: 起動コード・リンカスクリプト（rx-rt）と Rust だけのプログラム

## 構成
- `crates/rx-rt/`（クレート）
  - `src/lib.rs`: リセットハンドラ（`global_asm!`）、`.data` コピー / `.bss` ゼロ化（Rust）、`interrupt!` マクロ、
    `interrupt::{enable, disable, is_enabled, free, wait}`。
  - `build.rs`: 可変ベクタ表（256 エントリ、`_rx_vector_N`）と固定ベクタ表（0xFFFFFF80〜）を `vectors.s` に生成し、
    `link.x.in` に `PROVIDE(_rx_vector_N = _rx_default_handler)` を埋めて `link.x` を出力、OUT_DIR をリンク検索パスに追加。
  - `link.x.in`: アプリの `memory.x`（RAM / ROM の 2 領域、ROM は 0xFFFFFFFF で終わること）を INCLUDE。
    `.text`（先頭に `.text.rx_reset`）/ `.rodata` / `.rx_vectors` を ROM、`.data` を RAM（LMA は ROM）、`.bss` を RAM、
    `.rx_fixed_vectors` を 0xFFFFFF80。GCC/Renesas 流の P/C/D/B セクション名も拾う。
- アプリ側: `#![no_std] #![no_main]`、`#[unsafe(no_mangle)] fn main() -> !`、`#[panic_handler]`。
  `.cargo/config.toml` で `-C link-arg=-Tlink.x`、`build-std = ["core"]`、`build-std-features = ["compiler-builtins-mem"]`。
  `memory.x` は build.rs で OUT_DIR にコピーして検索パスへ（cortex-m-rt と同じ流儀）。

## 起動の流れ
1. リセットで 0xFFFFFFFC のベクタ → `_rx_reset`（スーパバイザモード、ISP、PSW.I=0）。
2. `mov.l #_rx_stack_top, r0`、`mvtc #_rx_stack_top, usp`、`mvtc #_rx_vectors, intb`。
3. `bsr.a start` → `.data` コピー、`.bss` ゼロ化（volatile ループ）→ `main()`。
- `_rx_stack_top` の既定値は RAM の末尾（memory.x で PROVIDE を上書き可能）。
- 固定ベクタ表: 0xD0 特権命令、0xD4 アクセス例外、0xDC 未定義命令、0xE4 浮動小数点、0xF8 NMI、0xFC リセット。
  それ以外（0x80〜0xCF のオプション設定メモリ / ID コード等）は 0xFFFFFFFF。例外ハンドラは
  `_rx_exception_{privileged,access,undefined,fpu,nmi}` を定義すれば差し替わる（既定は無限ループ）。
- **RX72N 等の実機向けの注意**: オプション設定メモリ（OFS0/OFS1/MDE）の位置と意味は品種ごとに違う（RX72N は
  0xFE7F5D00 付近）。実機の型番が決まったら memory.x とあわせて品種別に用意する。

## シンボル名
- DataLayout が `m:o` なので C / Rust のシンボルにはすべて `_` が付く。リンカスクリプトの `_rx_data_start` は
  Rust 側では `rx_data_start`、`fn main` は `_main`。`global_asm!` 内で手書きする名前も `_` 付きで書く。
  `sym` オペランドは LLVM の Mangler を通るので自動で `_` が付く。

## 割り込み
- `rx_rt::interrupt!(28, handler)` で `_rx_vector_28` を定義（`extern "C" fn()` を要求）。ベクタごとの入口は
  `pushm r1-r5 / pushm r14-r15 / mov.l #handler, r5 / bra.a _rx_isr_common`（GCC ABI の caller-saved）。
  rx-rt の共通部 `_rx_isr_common` が、rx-rt をビルドしたときの target feature に応じて退避してから `jsr r5` し、
  逆順に戻して `popm / rte`（issue #1 のステップ 5）:
  - `fpu`（rxv2 以上の既定）: FPSW（`mvfc fpsw` + push）。コード生成は fast-math の f32 演算と int→float 変換で FPU を使う
  - `dfpu`（rxv3 / rxv3e）: `dpushm.d dr0-dr1` と `dpushm.l dpsw-dcmr`。コード生成の DFPU 列は DR0 / DR1 / DPSW / DCMR
    を使い、割り込みは列の途中にも入りうる（DR2〜DR15 はコード生成が使わないので退避しない）
  - Cargo feature `save-acc`: DSP アキュムレータ。RXv1 は GCC の `-msave-acc-in-interrupts` と同じく ACC の
    63〜16 ビット（mvfachi / mvfacmi、戻すときは mvtachi と shll #16 + mvtaclo で下位 16 ビットは 0）、RXv2 以上は
    A0 / A1 の 72 ビット全部（mvfacgu / mvfachi / mvfaclo）。コード生成は DSP 命令を使わないので既定では無効
  - ハンドラは FPSW / DPSW = 0x100（最近接丸め、例外マスク、非正規化数は 0 扱い）で始まる。
- 起動時（`_rx_reset`）に `fpu` なら `mvtc #0x100, fpsw`、`dfpu` なら DPSW も 0x100 にする。
  RXv3 マニュアルでは FPSW.DN も DPSW.DDN もリセット値は 1（非正規化数を 0 扱いし、未実装処理例外を出さない）だが、
  ブートローダなどが変えている場合に備えて明示的に設定する。
- 判定は `cfg(target_feature = "fpu" / "dfpu" / "rxv2")`。そのため rustc に RX の target feature
  （rxv2 / rxv3 / rxv3e / fpu / dfpu、unstable `rx_target_feature`）を追加した。
- 検証: `test/rust/isr-save`（rxv1 + fpu + save-acc、`scripts/run-rust-rom.sh test/rust/isr-save`）が QEMU で 19 チェック全通過
  （release / dev）。割り込まれる側の FPSW（向き 0 丸め）と ACC がハンドラ（FPSW を変え、itof を使い、ACC を書き換える）の
  後も残ること、ハンドラ開始時の FPSW が 0x100 であること、起動時の FPSW が 0x100 であること。
  QEMU では MVTC で FPSW の要因フラグ（CV など）を立てられないので、それは使っていない。
  rxv2 / rxv3 向けの `_rx_isr_common` は逆アセンブルで確認（DFPU と RXv2 の ACC 命令は QEMU で動かない）。
- 使う側のクレートにも `#![feature(asm_experimental_arch)]` が必要。
- `atomic_cas = true` なので `fetch_add` / `compare_exchange` 等が使える。RXv2 以上では movli/movco で割り込みと
  競合しても正しく動く。RXv1 では割り込みを止めて実行する（スーパーバイザモードが前提）。

## rustc 側の変更（M5）
- `compiler/rustc_target/src/asm/rx.rs`: レジスタクラス `reg`（r1〜r15、型 i8/i16/i32/f32）。r0 / sp はエラー。
  clobber_abi("C") は r1〜r5, r14, r15。既定で `~{psw}` を clobber。LLVM 制約は `r`、ダミー出力型は i32。
- ターゲットのリンカを `rx-elf-ld` 直呼び（`LinkerFlavor::Gnu(Cc::No, Lld::No)`）に変更。libc が無く、
  実行時ルーチンは compiler_builtins が持つため。

## 検証（`scripts/run-rust-rom.sh [CRATE]`）
`test/rust/qemu-demo`: Rust だけ（C も GNU as も使わない）。objcopy で 512 KB のコードフラッシュイメージにして
`qemu-system-rx -M gdbsim-r5f562n8 -bios rom.bin` で**リセットベクタから起動**。SCI0 を `-serial file:` で取得し、
結果変数をモニタで読む。
- SCI0 に `Hello from Rust on RX!` と `ticks=5 soft=2 checks=8 failures=0` が出力される。
- `.data` 初期値・`.bss` ゼロ・`int #64`（ソフトウェア割り込み、PSW.I=0 でも INTB 経由）・CMT0 コンペアマッチ割り込み
  （ベクタ 28、IER03 bit4、IPR04=1、PCLK/8、CMCOR=1000）×5 回・`setpsw i` / `mvfc psw` / `wait`。
- **8 チェック全通過（release / dev とも）**。サイズ: text 2.8 KB + rodata 0.3 KB + ベクタ 1 KB（release）。
- QEMU の SCI: SCR.TE を立てると TDRE=1、TDR 書き込みで送信。TDRE（SSR bit7）を待ってから書けばよい
  （実機 RX62N 以降の SCIc/SCIg は SSR に TDRE が無い品種もあるので、実機ドライバは品種ごとに書く）。
- 既存テスト（core-smoke 80 チェック）も release / dev で通過。
