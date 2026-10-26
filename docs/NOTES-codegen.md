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

## アトミック（CAS / RMW、issue #1）
- 32bit の cmpxchg を擬似命令 `CMPXCHG32`（出力 dst / tmp は early-clobber、入力 ptr / cmp / new、PSW を破壊、16 バイト）で選択し、
  RXAsmPrinter が展開する。レジスタ割り付け後に展開するので、列の途中にスピルが入らない。
  - RXv2 以上: `1: movli [ptr], dst / cmp cmp, dst / bne.b 2f / mov.l new, tmp / movco tmp, [ptr] / cmp #0, tmp / bne.b 1b / 2:`。
    割り込み・例外を受けると LI がクリアされ movco が失敗（tmp = 1）してやり直す。
  - RXv1（movli なし）: `mvfc psw, tmp / clrpsw i / mov.l [ptr], dst / cmp cmp, dst / bne.b 2f / mov.l new, [ptr] / 2: mvtc tmp, psw`。
    割り込みを止めるのでスーパーバイザモードでのみアトミック（ユーザモードでは CLRPSW I が効かない）。
- `setMinCmpXchgSizeInBits(32)` で 8/16bit の cmpxchg は AtomicExpand が含むワードの CAS に変換。
  atomicrmw はすべて `AtomicExpansionKind::CmpXChg`（CAS ループ）。success フラグは既定の展開（読んだ値と cmp の比較）。
- 検証: `test/codegen/atomic.ll` + `atomic_main.c` を QEMU（RXv1 経路）で **1080 チェック全通過**（-O0 / -O2、.s 経由・.o 直接とも）。
  32bit cmpxchg（strong / weak）、8/16bit cmpxchg（ワード内の全位置、隣のバイトが変わらないこと）、
  atomicrmw 全種（add〜umin、uinc_wrap）、8/16bit の RMW、値を多く生かしたままの CAS ループ。
  RXv2 / RXv3 経路は `llc -mcpu=rxv2|rxv3` の出力が movli/movco ループになり、GNU as でアセンブルでき、
  直接出力した .o と同一バイト列であることを確認（実行は QEMU では不可）。

## 単精度 FPU（`+fpu`、issue #1）
- サブターゲット機能 `fpu`（rxv2 / rxv3 / rxv3e の既定。rxv1 は FPU のない RX100/RX200 もあるので既定では無効）。
  アセンブラは従来どおり CPU に関係なく FPU 命令を受け付ける。
- RX の FPU は NaN・無限大を IEEE どおりに扱わず（GCC は FPU 使用時に -ffinite-math-only を強制する）、
  非正規化数は FPSW.DN = 0 で「未実装処理」例外、DN = 1（リセット値。rx-rt も設定する）で同符号の 0 扱いになる。そこで
  **nnan と ninf の両方が付き、かつ関数の float の非正規化数環境が preserve-sign（`denormal_fpenv(float: preservesign)`、
  clang なら `-fdenormal-fp-math=preserve-sign`）の演算だけ** FPU 命令にし、それ以外は従来どおりソフトフロート
  （__addsf3 など）で IEEE の意味を保つ。nnan / ninf だけでは非正規化数の正しい扱いは免除されない（llvm-project#4 のレビュー）。
  rustc は非正規化数環境を指定しないので、Rust から FPU の四則・比較が使われることはない（int → float の変換だけ）。
- `+fpu` のとき f32 は GPR の合法型（`GPR` を [i32, f32] に）。FPU は汎用レジスタで演算し、ABI も float を整数レジスタで
  渡すので、呼び出し規約は変わらない（RetCC に f32 を追加しただけ。引数は analyzeRXArguments が型に依らない）。
  - FADD/FSUB/FMUL/FDIV（と RXv2 の FSQRT）は Custom: 上の条件（canUseFPU）を満たせばそのまま（fadd 等を選択）、
    でなければ libcall。
    FPU のない RXv1 の FSQRT は sqrtf を呼ぶ。
  - SETCC / BR_CC / SELECT_CC の float 比較: 同じ条件なら `fcmp`（O フラグが立たないので符号付きの条件がそのまま使える）、
    でなければ softenSetCCOperands（__ltsf2 など）+ 整数比較。DAGCombiner が作る BR_CC / SELECT_CC にもフラグが残ることを確認。
  - int → float は `itof`（RXv2 では `utof` も）を常に使う（入力が整数なので常に正確。丸めは FPSW.RM = 最近接）。
    float → int は非正規化数で例外になりうる `ftoi` を使わず libcall（__fixsfsi）。
  - FNEG / FABS / FCOPYSIGN はビット演算、その他（frem、fma、min/max、丸め、超越関数、half 変換など）は Expand（libcall）。
  - 定数は `mov.l #ビット列`（isFPImmLegal）、ロード・ストア・bitcast・select は整数と同じ命令。
  - パターン述語を使うため RXDAGToDAGISel に `Subtarget` メンバを追加。
- 検証: `test/codegen/fpu.ll` + `fpu_main.c`（`ATTRS=+fpu`）を QEMU で **3142 チェック全通過（-O1〜-O3、.s 経由・.o 直接とも）**、
  `+fpu` なしでも全通過。fast-math（preserve-sign の関数）の四則・比較・select・分岐が GCC（FPU 使用）とビット単位で一致、
  IEEE 側は NaN / ±無限大 / ±0 を含めて正しいこと、nnan+ninf でも IEEE 環境の関数（fa_add_ieee / fa_olt_ieee）は
  libcall のままで非正規化数の加算・比較が正確なこと、ビット演算・変換・定数・メモリ・6 引数（スタック渡し）の呼び出し。
  既存の i32 / m3 / struct / atomic も `+fpu` 付きで全通過。多数の f32 演算（frem、fma、min/max、丸め、sin、powi、
  飽和変換、i64 変換、is.fpclass、ldexp、lrint、half 変換、canonicalize、ベクトル、atomic）が rxv1+fpu / rxv2 / rxv3 の
  -O0 / -O2 でコンパイルでき GNU as でアセンブルできることも確認。
- **QEMU 8.2.2 のバグ**: 同じ変換ブロック内で直前にフラグを変える整数命令があると、`fcmp` の後の条件分岐が古いフラグを見る
  （`mov.l #1, r3 / add #1, r3 / mov.l #0, r4 / mov.l #0, r5 / fcmp r4, r5 / bne` が分岐する。`fcmp` 単独なら正しい）。
  fpu.ll を -O0 にすると `fa_max` の前にスタック確保の `add` が入ってこれに当たり失敗する（ISA 上は正しいコード）。
  そのため fpu.ll の実行は -O1〜-O3 で行う。実機では問題にならない。

## 倍精度 DFPU（`+dfpu`、issue #1）
- RXv3 ソフトウェアマニュアル（R01US0316EJ0100）で確認した意味:
  `DADD/DSUB/DMUL/DDIV src, src2, dest` は `dest = src2 op src`、`DCMPcm src, src2` は `DCMR.RES = src2 cm src`
  （cm = UN/EQ/LT/LE）、`MVFDR` は `PSW.Z = DCMR.RES`、`ITOD/UTOD` は 32bit 整数 → double（正確）、
  `DTOI` は 0 方向に丸めて下位 32bit。NaN・無限大は IEEE どおりだが、非正規化数は DPSW.DDN = 0 だとマスクできない
  未実装処理例外、DDN = 1 だと 0 扱い（**DPSW.DDN のリセット値は 1**）。
- f64 は従来どおりソフトフロート（GCC の ABI どおり汎用レジスタ対、下位ワードが先）のまま。f64 を DR の合法型にすると
  IEEE の通常の double（大半のコード）まで呼び出しのたびに GPR↔DR の転送と DR の退避が入るため。
- **型合法化の前**の PerformDAGCombine（LLVM 23 の SoftenFloatResult はターゲットのカスタム処理を呼ばない）で、
  nnan+ninf かつ double の非正規化数環境が preserve-sign（単精度 FPU と同じ canUseFPU）の
  FADD/FSUB/FMUL/FDIV/FSQRT、f64 比較（SETCC / SELECT_CC / BR_CC）と、常に i32 → f64 の
  SINT_TO_FP / UINT_TO_FP を、ワード対を受け渡しする RXISD::DARITH / DSQRT / DCMP / ITOD / UTOD に置き換える
  （`bitcast(build_pair(lo, hi))` で f64 に戻すので、以降の型合法化はそのまま）。
  BR_CC を f64 で Custom にしているのは、DAGCombiner が brcond + setcc から fast-math フラグ付きの BR_CC を作るようにするため。
- 擬似命令 DARITH_P / DSQRT_P / ITOD_P / UTOD_P / DCMP_P は DR0 / DR1 を固定の作業レジスタとして Defs に持ち、
  RXAsmPrinter が展開する（DR の値は擬似命令の外では生きていないので、レジスタ割り付け・退避の対象にならない）:
  `dmov.l alo, drl0 / dmov.l ahi, drh0 / dmov.l blo, drl1 / dmov.l bhi, drh1 / d<op> dr1, dr0, dr0 /
  dmov.l drl0, lo / dmov.l drh0, hi`（28 バイト）。比較は `... / dcmp<cm> dr1, dr0 / mvfdr` で Z に結果
  （EQ → Z、NE → !Z。GT/GE はオペランドを入れ替えて LT/LE）。
- DPSW などの DFPU 制御レジスタを予約レジスタに追加（暗黙の Uses が未定義使用として MachineVerifier に落とされるため）。
- 検証（QEMU に DFPU がないので実行はしていない）: `scripts/check-dfpu.sh` が `test/codegen/dfpu.ll` を rxv3 でコンパイルし、
  -verify-machineinstrs、GNU as でのアセンブル（直接出力の .o と同一。GNU as は itod / utod / ftod のレジスタ欄を
  入れ替える誤りがあるので、渡す .s ではその 2 つのレジスタ番号を入れ替えておく。NOTES-mc.md）、関数ごとの DFPU 命令列を
  `test/codegen/dfpu.expected`（マニュアルの意味と照合済み）と比較。四則・平方根・計算の連鎖・6 種の比較・select・分岐・
  int → double・呼び出しをまたぐ値、IEEE 側が libcall のままであること（nnan+ninf でも IEEE 環境の
  da_add_ieee / da_olt_ieee を含め 25 関数）。f64 の多数の演算（fops.ll の double 版）も
  rxv1〜rxv3 の -O0 / -O2 でコンパイル・アセンブルできることを確認。

## 未対応（M4 以降）
- LLVM が戻り値を自動で sret 化する場合（16 バイト超のスカラー集合）の R15 渡しは未検証
- LLVM 側で定義する可変長関数（va_start / va_arg）
- DFPU の実行検証（QEMU も GDB の RX シミュレータも RXv2/v3 命令を持たない）、f64 の DR への常駐（fast-math の連鎖で
  毎回 GPR を経由する）、dtoi / dtof / ftod（非正規化数・DDN の扱いのため libcall のまま）
- FPU のメモリオペランド形式（fadd [r1], r2 など）と RXv2 の 3 オペランド形式の選択
- emulu を使った i64 乗算の高速化
- 動的 alloca、フレームポインタ、CFI/アンワインド（GNU as が .cfi_* 非対応）、ジャンプテーブル
- アトミックの RXv2 経路（movli/movco ループ）の実行検証
- プロローグ/エピローグの最適化（pushm/popm、rtsd）、mulhs/mulhu（emul の利用）
