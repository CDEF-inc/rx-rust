# RXv3e メモ（RX74M / RX74N）

参照元: `ref/r01us0745ej0101-rxv3esm.pdf`（RXv3e ISA ソフトウェアマニュアル Rev.1.01, 2026-08-06）、
テキスト版 `ref/rxv3esm.txt`。マニュアル本文は転載せず、事実だけをまとめる。

## 結論（コンパイラへの影響）
- RXv3 と上位互換。追加は **Protect Zone 用のシステム命令 4 個** と **制御レジスタ系 8 命令の仕様拡張** のみ。
- 一般命令・DSP・単精度 FPU・倍精度 FPU（21 命令、オプション）は RXv3 と同じ。
  → **コード生成は RXv3 + DFPU と同一**。RXv3e は MC 層（アセンブラ／逆アセンブラ）と
  ランタイム（起動コード、割り込み、ワールド遷移）でのみ扱う。
- 通常のアプリは「Flat-Project」（root ワールドのみで動作）として RXv3 と同様に書ける。
- LLVM: `-mcpu=rxv3e`（FeatureRXv3e ⊃ RXv3、+ DFPU）。新命令は FeatureRXv3e の述語で有効化する。

## Protect Zone
- ワールド: root(ID 0) / secure(ID 1) / non-secure(ID 2)。リセット直後は root。
- secure ⇔ non-secure の直接遷移は不可。必ず root を経由。
- 割り込み優先度: root/secure 宛ては 1〜31、non-secure 宛ては 1〜15（IPL が 5bit に拡張）。
- 高速割り込みは Flat-Project のみ。

## 新命令（3.2.1 / 4.2.4）
| 命令 | 機能 | 長さ | 符号（先頭バイトから） |
|---|---|---|---|
| `trcall #imm8` | root ワールドへの遷移呼び出し | 4 | `FD 75 00 imm8` |
| `trmvfc cr, rd` | root の制御レジスタ → Rd | 3 | `FD 5A [cr:4][rd:4]` |
| `trmvtc #imm, cr` | 即値 → root の制御レジスタ | 4〜7 | `FD [0111 li:2 11] [0010 cr:4]` + imm（li: 01=s8,10=s16,11=s24,00=32） |
| `trmvtc rs, cr` | Rs → root の制御レジスタ | 3 | `FD 58 [rs:4][cr:4]` |
| `trrte` | root ワールドでの例外復帰 | 2 | `7F 97` |

※ 上の符号は 4 章の符号の図と照合済み（CDEF-inc/qemu の RXv3e 対応、NOTES-qemu.md）。

## 仕様拡張された命令（3.2.2）
- `pushc` / `popc` / `mvfc` / `mvtc`: 対象に IVC, WID, WPRI を追加。
- `int`: ソースにレジスタ指定を追加（`int Rs`）。
- `mvtipl`: 指定できる優先度を拡張（5bit）。
- `rte` / `rtfi`: non-secure ワールドでは PSW.IPL[4] への書き込みを制限。

## 制御レジスタ番号（mvfc/mvtc の cr）
| cr | レジスタ |
|---|---|
| 0 | PSW |
| 1 | PC |
| 2 | USP |
| 3 | FPSW |
| 5 | IVC（新） |
| 8 | BPSW |
| 9 | BPC |
| 10 | ISP |
| 11 | FINTV |
| 12 | INTB |
| 13 | EXTB |
| 14 | WPRI（新、mvfc/mvtc） / TRCTB（新、trmvfc/trmvtc） |
| 15 | WID（新、mvfc/mvtc） / WINTB（新、trmvfc/trmvtc） |

- IVC/BPSW/BPC/ISP/INTB/EXTB は実体がワールドごとに 3 本（`_TR` / `_S` / `_NS`）。
  `mvfc`/`mvtc` は現在のワールドのもの、`trmvfc`/`trmvtc` は root（`_TR`）のものにアクセスする。

## ツールチェイン状況（2026-10-23 時点）
- 手元の GNURX 14.2（`~/work/gnurx`）は RXv3e 未対応（`-misa=v1|v2|v3` のみ）。
- Zephyr の RX74 対応 PR（zephyr#118284）は新命令を使わず `-mdfpu` を付けてビルドしている。
- binutils 2.43 も RXv3e 未対応 → 新命令の検証には GNU as が使えない。マニュアルの符号表で直接検証する。
