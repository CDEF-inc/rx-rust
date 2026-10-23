# M2 コード生成メモ（2026-10-23）

## 構成（llvm/lib/Target/RX）
| ファイル | 役割 |
|---|---|
| RXCallingConv.td | CC_RX / RetCC_RX / CSR_RX（GNURX 互換: 引数 R1-R4、戻り値 R1、callee-saved R6-R13） |
| RXInstrInfo.td 後半 | SDNode、AddrB/W/L（ComplexPattern）、擬似命令、選択パターン |
| RXISelLowering | 引数・戻り値・呼び出し、BR_CC/SETCC/SELECT_CC → RXISD::CMP + フラグ |
| RXISelDAGToDAG | アドレス選択（base + dsp、dsp はサイズの倍数かつ dsp:16 に収まる場合のみ）、FrameIndex → FRMIDX |
| RXFrameLowering | push.l で CSR 保存、`add #-N, r0, r0` で確保、フレームポインタなし |
| RXRegisterInfo | R0(SP)/PSW/FPSW/ACC 予約、eliminateFrameIndex（SP 相対） |
| RXInstrInfo | コピー・スピル・分岐解析、BranchRelaxation 用の範囲判定 |
| RXMCInstLower | 擬似命令の展開 + **最短符号への縮小**（下記） |
| RXAsmPrinter | lower → shrink → 出力 |

### 「最長で選択し、出力時に縮める」方式
命令選択は常に最長形式（#imm32、dsp:16、3 オペランド）を使い、`shrinkRXInst` が最終オペランドを見て
アセンブラと同じ規則で短い形式に置き換える。フレームオフセット確定後に縮められるので選択が単純になる。
- `mov.l #imm` / `cmp #imm`: uimm4 → uimm8 → simm8(負) → simm16 → simm24
- `add #imm, rs, rd`: rd==rs かつ uimm4 なら 2 バイト形式
- mul/and/or/xor/max/min/div… #imm: uimm4（あれば）→ simm8/16/24
- ロード/ストア dsp:16 → `[Rn]`（0）/ dsp:5（r0-r7）/ dsp:8
- 3 オペランド add/sub/mul/and/or → rd が入力と同じなら 2 オペランド形式
- `shll #n, rs, rd` → rd==rs なら 2 バイト形式
`getInstSizeInBytes` は最長形式を返す（分岐緩和に対して安全側）。

### 比較と分岐
- `cmp src, src2` は src2 - src のフラグ。`(RXcmp a, b)` → `cmp b, a`。
- 条件コード: eq ne geu ltu gtu leu pz n ge lt gt le o no（0〜13、逆条件は bit0 反転）
- BCC 擬似 → `b<cc>.b`（±128）。届かなければ BranchRelaxation が「逆条件 b<cc>.b + bra.a」に組み替える。
  無条件分岐は常に `bra.a`（24bit）。
- SETCC → `sc<cc>.l`、SELECT_CC → 分岐の菱形（custom inserter）。

### シンボル名
DataLayout を `e-m:o-p:32:32-i64:32-f64:32-n8:16:32-S32` に変更。`m:o` で C シンボルに `_` が付く（GCC rx と同じ）。
副作用として内部ラベル接頭辞が `L`（`LCPI…` 等がローカルシンボルとして残る）。Rust の target spec の data-layout もこれに合わせること。

## 検証
`scripts/run-codegen-test.sh`（`test/codegen/i32.ll` + `test/codegen/main.c`）:
- llc → .s → GNU as と、llc → .o 直接の 2 経路でビルド
- GCC（-mcpu=rx600 -O2）のドライバ・libgcc とリンク、QEMU `gdbsim-r5f562n8`（RXv1）で実行
- 結果は QEMU モニタでメモリを読む（`finished` / `checks` / `failures` / `first_failure`）
- **4131 チェック全通過**（-O0 / -O1 / -O2 / -O3、as 経路・obj 経路とも）
- 内容: 四則演算（div/rem 含む）、論理、シフト（定数/レジスタ）、符号/ゼロ拡張、bswap、smax/smin/umax/abs、
  ctpop/ctlz（展開）、10 種の比較、select、ループ、再帰（fib）、分岐緩和（>128 バイト）、グローバル変数、
  8/16bit ロード・ストア、ローカル配列（動的インデックス）、8 引数（4 つスタック）、GCC 関数の呼び出し（スタック引数 + CSR）、
  char/short の昇格
- 反証: `t_add` を sub に壊すと 120 件失敗を検出することを確認
- M1 の MC 差分テストも全通過（basic 210、branch、symbol、scc 14）

シミュレータについて: Zephyr SDK の gdb は `target sim` 非搭載、GDB sim は未ビルド。QEMU 8.2 の RX は RXv1 のみ
なので実行テストは `-mcpu=rxv1`。RXv2/v3 の命令を出すようになったら別の検証手段（実機 or GDB sim）が要る。

## M3: i64 / float / double（2026-10-23）

### GCC（rx-elf-gcc -mcpu=rx600 -m64bit-doubles）で実測した ABI
| ケース | 配置 |
|---|---|
| i64 / double の戻り値 | R1 = 下位語、R2 = 上位語 |
| `f(int, long long x)` | x は R2:R3（偶数レジスタ揃えなし） |
| `f(int, int, int, long long x)` | 12+8 > 16 なので x は丸ごとスタック +0 |
| その後ろの `int y` | スタック +8（一度溢れたら以降はすべてスタック） |
| 5 個目の int の後の `long long` | スタック +4（4 バイト境界） |
| float | 汎用レジスタ（R1 等） |
| 可変長 `f(int n, ...)` | n（最後の名前付き）も含め全部スタック |
| 構造体戻り値（メモリ） | アドレスは R15 |
ソース上の根拠: rx_function_arg / rx_function_arg_advance（バイト数は常に加算）/ rx_function_arg_boundary（自然境界）。

### 実装
- `analyzeRXArguments`（RXISelLowering.cpp）: 上表の規則を C++ で実装。TableGen の CC_RX は削除。
  sret ポインタは R15。byval は未対応（エラー）。
- i64 加減算: ADDC/ADDE/SUBC/SUBE を Legal にし `add`/`adc`、`sub`/`sbb` を選択（キャリーは PSW.C 経由）。
  乗除算・可変シフトは libgcc（__muldi3, __divdi3, __udivdi3, __moddi3, __umoddi3）または展開。
- float / double: レジスタクラスを持たせず soften。演算・比較・変換はすべて libgcc の soft-fp
  （__addsf3, __adddf3, __muldf3, __ltdf2, __unorddf2, __floatdidf, __fixdfdi, __extendsfdf2, …）。
  リンクは **64-bit-double multilib の libgcc**（`rx-elf-gcc -m64bit-doubles -print-libgcc-file-name`）。
- `RuntimeLibcalls.td` に RX の SystemRuntimeLibrary を追加（LLVM 23 はターゲットごとに libcall を登録しないと
  "unsupported library call operation" で止まる）。
- llc では `-mattr=+64bit-doubles` を付けて e_flags を GCC -m64bit-doubles と揃える（LLVM の double は常に 64bit）。

### 重要: GCC の FPU 使用と finite-math
GCC rx は FPU を使うとき **`-ffinite-math-only` を自動で有効にする**（RX FPU 命令は NaN/無限大を扱えないため）。
Rust は IEEE 754 の NaN/Inf を前提とするので、LLVM 側は f32 もソフトフロートにしている（現状の選択が正しい）。
FPU 命令（fadd 等）を使うのは nnan/ninf 付きの演算（fast-math）に限る、が今後の最適化方針。
テストのドライバでは NaN をビット列から作っている（GCC が 0.0/0.0 を畳んでしまうため）。

### 検証
`scripts/run-codegen-test.sh test/codegen/m3.ll test/codegen/m3_main.c`:
- **3964 チェック全通過**（-O0〜-O3、.s 経由・.o 直接とも）。M2 の i32 テスト（4131）も全 -O で通過。
- GCC → LLVM: i64 演算 25 種、double 21 種、float 9 種、引数配置テスト（p_2/p_3/p_5/p_ll3/d_pos/d_stack/f_mix）
- LLVM → GCC: `l_cross` から c_mix（i32, i64, double, float, i64 → スタック引数込み）、i64/double を返す関数、
  可変長関数 c_va（i64, double, int を va_arg で読む）
- double はビット単位で一致比較。float は GCC 側が FPU、LLVM 側が soft-fp だが、非正規化数を避けた入力で一致。

## 構造体の値渡し・戻り値（2026-10-23）

### GCC で実測した規則
| ケース | 配置 |
|---|---|
| `struct s8` / `s12` / `s16`（サイズが 4 の倍数、16 以下） | 収まればレジスタ（R1 から、カウンタ位置から連続） |
| `f(int, int, struct s12)` | 8+12 > 16 → 丸ごとスタック +0、以降すべてスタック |
| `f(struct s3, int y)`（サイズが 4 の倍数でない） | s3 はスタック +0、カウンタは +4 だけ進むので **y は R2** |
| `f(struct s6, struct s3, int z)` | s6 +0、**s3 +6**（char 構造体は詰めて配置）、z は R4 |
| `f(int, int, int, struct s3, int y)` | s3 +0、y +4（int は 4 境界） |
| `f(struct s20, int y)` | s20 +0、y +20（16 を超えたので以降スタック） |
| `struct {short a, b;}` | R1（下位 16bit = a） |
| 戻り値: サイズ 1〜16 かつ 4 の倍数 | R1〜R4 |
| 戻り値: それ以外（s3, s20 など） | 呼び出し側が R15 にバッファを渡す。**呼ばれた側は R1 にもそのアドレスを返す** |

### バックエンドの扱い（フロントエンドとの約束 — M4 の rustc callconv はこれに従う）
- GCC がレジスタで渡す構造体（サイズが 4 の倍数で 16 以下）→ フロントエンドは `[N x i32]` 等の整数パーツで渡す。
  バックエンドは i64 と同様に「全部レジスタか全部スタック」で扱う（スタックでは 4 境界）。
  - 注意: 要素が char だけの 4 の倍数サイズ構造体がレジスタに入らずスタックに落ちる場合、GCC は 1 境界で詰めるが
    パーツ渡しだと 4 境界になる。フロントエンドはこの場合（カウンタで判定できる）byval を使うこと。
- それ以外の構造体 → `byval`（align は型の自然境界）。スタックに型の境界で詰めて置き、カウンタは
  サイズを 4 に切り上げた分だけ進める（後続の引数はレジスタに戻りうる）。呼び出し側のコピーはインライン展開
  （libc の memcpy に頼らない）。
- メモリで返す構造体 → `sret` 引数。R15 で渡し、カウンタに数えない。呼ばれた側は R1 に同じアドレスを返す。
- 構造体戻り値をレジスタで返す場合は `{i32, ...}` / `[N x i32]`（R1 から順）。

### 検証
`scripts/run-codegen-test.sh test/codegen/struct.ll test/codegen/struct_main.c`: **142 チェック全通過**（-O0〜-O3、両経路）。
- GCC → LLVM: L_g1〜L_g8（上表の各配置）、L_g10（byval のコピーを書き換えても呼び出し側は不変）、
  L_r8 / L_r16（レジスタ戻り）、L_r20 / L_r3（sret）
- LLVM → GCC: L_cross_s から C_g3〜C_g7（[3 x i32] と byval）、C_r8、C_r20 / C_r3（sret バッファ）
- sret の R1 返却は GCC のインラインアセンブリで `bsr.a _L_r20` 後の R1 を読んで確認
- 生成コードのオフセット（L_g4 の 6[r0] と R2、L_g5 の 9/11[r0] と R4、L_g7 の 4/8[r0]）が GCC と一致することも目視で確認

## 未対応（M4 以降）
- LLVM が戻り値を自動で sret 化する場合（16 バイト超のスカラー集合）の R15 渡しは未検証
- LLVM 側で定義する可変長関数（va_start / va_arg）
- FPU 命令による f32 演算（fast-math 時のみ）、DFPU による f64 演算（RXv3、QEMU では検証不可）
- emulu を使った i64 乗算の高速化
- 動的 alloca、フレームポインタ、CFI/アンワインド（GNU as が .cfi_* 非対応）、ジャンプテーブル、atomics
- プロローグ/エピローグの最適化（pushm/popm、rtsd）、mulhs/mulhu（emul の利用）
