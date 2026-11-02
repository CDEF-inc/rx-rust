# M2 コード生成メモ（2026-10-23）

## 構成（llvm/lib/Target/RX）
| ファイル | 役割 |
|---|---|
| RXCallingConv.td | CC_RX / RetCC_RX / CSR_RX（GNURX 互換: 引数 R1-R4、戻り値 R1、callee-saved R6-R13） |
| RXInstrInfo.td 後半 | SDNode、AddrB/W/L（ComplexPattern）、擬似命令、選択パターン |
| RXISelLowering | 引数・戻り値・呼び出し、BR_CC/SETCC/SELECT_CC → RXISD::CMP + フラグ |
| RXISelDAGToDAG | アドレス選択（base + dsp、dsp はサイズの倍数かつ dsp:16 に収まる場合のみ）、FrameIndex → FRMIDX |
| RXFrameLowering | pushm で CSR 保存、`add #-N, r0, r0` で確保、rtsd で解放・復帰・戻り（下記）、プロローグとエピローグの CFI（下記）、フレームポインタなし |
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
- switch はジャンプテーブル（下記「ジャンプテーブル」）か比較の木。indirectbr は `jmp Rs`。

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
なので実行テストは `-mcpu=rxv1`。RXv2/v3 の命令は、その後 CDEF-inc/qemu の rxv2 / rxv3 / rxv3e コア
（`RX_CPU`、NOTES-qemu.md）と実機の AP-RX72M-0A（NOTES-hw.md）で実行している。

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
  除算・可変シフトは libgcc（__divdi3, __udivdi3, __moddi3, __umoddi3）または展開。乗算は emulu と mul
  （下記「乗算の上位半分」。以前は __muldi3）。
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
- **3957 チェック全通過**（-O0〜-O3、.s 経由・.o 直接とも。QEMU 8.2.2 では 3964 と数えていた、下記）。M2 の i32 テスト（4131）も全 -O で通過。
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
  IR で値として返していて R1-R4 に入らない戻り値（16 バイト超の構造体・配列）は、LLVM 自身が隠れた sret 引数に
  変える。この引数も `sret` 属性付きなので、同じく R15・カウンタに数えない・R1 に返す、になる。
- 構造体戻り値をレジスタで返す場合は `{i32, ...}` / `[N x i32]`（R1 から順）。

### 検証
`scripts/run-codegen-test.sh test/codegen/struct.ll test/codegen/struct_main.c`: **142 チェック全通過**（-O0〜-O3、両経路）。
- GCC → LLVM: L_g1〜L_g8（上表の各配置）、L_g10（byval のコピーを書き換えても呼び出し側は不変）、
  L_r8 / L_r16（レジスタ戻り）、L_r20 / L_r3（sret）
- LLVM → GCC: L_cross_s から C_g3〜C_g7（[3 x i32] と byval）、C_r8、C_r20 / C_r3（sret バッファ）
- sret の R1 返却は GCC のインラインアセンブリで `bsr.a _L_r20` 後の R1 を読んで確認
- 生成コードのオフセット（L_g4 の 6[r0] と R2、L_g5 の 9/11[r0] と R4、L_g7 の 4/8[r0]）が GCC と一致することも目視で確認

`test/codegen/demote.ll` + `demote_main.c`: LLVM が隠れた sret に変える戻り値（rx-rust #56）。**254 チェック全通過**
（各 CPU の -O0 / -O2、両経路）。
- GCC → LLVM: `{i32 ×5}`、`[5 x i32]`、末尾が char の 20 バイト、`{i64 ×3}`（i64 引数 2 つが R1-R4）、引数 5 つ
  （a〜d が R1-R4、e がスタック）、byval の後の引数が R2、戻る経路が 2 つある関数
- LLVM → GCC / LLVM: L_cross_d から C_d20、C_q24、C_d5（引数 5 つ）、L_d5、L_dsel
- `bsr.a _L_d20` の後の R1 が渡したアドレスであることをインラインアセンブリで確認

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
  - メモリオペランド（2026-11-02、rx-rust #50）: 1 回だけ使うロードは fadd / fsub / fmul / fdiv / fcmp（RXv2 では fsqrt も）の
    ソースに畳む（`fadd [r2], r1`、`fsub 12[r2], r1`、`fmul 1200[r2], r1`）。擬似命令 FADD_mP などが LD_L と同じ
    (base, dsp) オペランドを持ち、RXMCInstLower が dsp:16 の形にしてから [Rs] / dsp:8 / dsp:16 の最短を選ぶ（dsp は 4 倍で
    数える。FPU には dsp:5 の形が無い）。eliminateFrameIndex も 4 倍として扱う（スタック渡しの float もそのまま読める）。
    fsub / fdiv / fcmp はロードが 2 番目のオペランドのときだけ畳む（fadd / fmul は交換して畳む）。
    itof / utof のメモリ（memex）形式はまだ使わない。
  - RXv2 の 3 オペランド形式（2026-11-02、rx-rust #50）: rxv2 以降は fadd / fsub / fmul を常に 3 オペランドで選ぶ
    （2 オペランド形式と同じ 3 バイトで、両方のソースが生きていてもコピーが要らない）。書き込み先がソースと同じなら
    RXMCInstLower が 2 オペランド形式で出す（整数の add / sub / mul と同じ shrinkRRR）。fdiv に 3 オペランド形式は無い。
    fpu.ll の `f3_live` で 9 → 5 命令、`f_args` で 1 命令減る。
- 検証: `test/codegen/fpu.ll` + `fpu_main.c`（`ATTRS=+fpu`）を QEMU で **4244 チェック全通過（-O0〜-O3、.s 経由・.o 直接とも。
  メモリオペランドと 3 オペランド形式の前は 3110）**、
  `+fpu` なしでも全通過。fast-math（preserve-sign の関数）の四則・比較・select・分岐が GCC（FPU 使用）とビット単位で一致、
  IEEE 側は NaN / ±無限大 / ±0 を含めて正しいこと、nnan+ninf でも IEEE 環境の関数（fa_add_ieee / fa_olt_ieee）は
  libcall のままで非正規化数の加算・比較が正確なこと、ビット演算・変換・定数・メモリ・6 引数（スタック渡し）の呼び出し。
  既存の i32 / m3 / struct / atomic も `+fpu` 付きで全通過。多数の f32 演算（frem、fma、min/max、丸め、sin、powi、
  飽和変換、i64 変換、is.fpclass、ldexp、lrint、half 変換、canonicalize、ベクトル、atomic）が rxv1+fpu / rxv2 / rxv3 の
  -O0 / -O2 でコンパイルでき GNU as でアセンブルできることも確認。
- `scripts/check-fpu.sh` が fpu.ll を rxv1 + fpu / rxv2 / rxv3 の -O0 / -O2 でコンパイルし、GNU as の結果と llc の .o を
  比べ、-O2 の各関数の FPU 命令と呼び出しを `fpu.expected` と比べる（メモリオペランドの各 dsp、3 オペランド形式、
  RXv1 の 2 オペランド形式と sqrtf の呼び出し）。
- **QEMU 8.2.2 のバグ**: 同じ変換ブロック内で直前にフラグを変える整数命令があると、`fcmp` の後の条件分岐が古いフラグを見る
  （`mov.l #1, r3 / add #1, r3 / mov.l #0, r4 / mov.l #0, r5 / fcmp r4, r5 / bne` が分岐する。`fcmp` 単独なら正しい）。
  fpu.ll を -O0 にすると `fa_max` の前にスタック確保の `add` が入ってこれに当たり失敗する（ISA 上は正しいコード）。
  QEMU 11.1 では上流で修正済み（NOTES-qemu.md）。CDEF-inc/qemu の QEMU では fpu.ll も -O0 で通る。
  このバグは GCC がコンパイルしたドライバ側（`if (b != 0.0f)` などの float 比較）の分岐も狂わせていて、
  本来実行しないチェックまで数えていた（m3 は 3964、fpu は 3141 と数えていた。正しくは 3957 と 3110）。

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
  SINT_TO_FP / UINT_TO_FP、下記の条件の変換を RXISD::DARITH / DSQRT / DCMP / ITOD / UTOD / ... に置き換える
  （`bitcast(build_pair(lo, hi))` で f64 に戻すので、以降の型合法化はそのまま）。
  BR_CC を f64 で Custom にしているのは、DAGCombiner が brcond + setcc から fast-math フラグ付きの BR_CC を作るようにするため。
- DR レジスタへの常駐（2026-11-02、rx-rust #49）: 置き換えたノードは DR レジスタの値を untyped の値として受け渡す。
  `DR_PAIR(lo, hi)` がワード対を DR に入れ、`DR_SPLIT` がワード対に戻す。f64 のオペランドは
  `DR_PAIR(splitF64(x))`、結果は `joinF64(DR_SPLIT(d))` にし、どの段階の DAG combine でも
  `DR_PAIR(DR_SPLIT(d))` を `d` に畳む。これで DFPU が計算して DFPU が使う値（同じ基本ブロック内の連鎖）は
  DR に残り、GPR を通らない。基本ブロックをまたぐ値・呼び出しの引数と戻り値・ロード / ストアは従来どおりワード対。
  - f64 を DR の合法型にしない理由は従来どおり（IEEE の double がすべて DR を通るようになる）。untyped は型合法化の
    対象外なので、合法化の前に作ってよい。
  - 選択は RXISelDAGToDAG の `selectDFPU`（untyped にはパターンが書けない）。DADD / DSUB / DMUL / DDIV / DSQRT /
    ITOD / UTOD / FTOD / DTOI / DTOU / DTOF は実命令をそのまま選び、DR_PAIR_P / DR_SPLIT_P / DR_LO_P（dtoi 等の結果の
    下位ワードを GPR へ）/ DCMP_P（`dcmp<cm> y, x` + `mvfdr`）は RXAsmPrinter が `dmov.l` などに展開する。
  - 割り付けるのは **DR0 と DR1 だけ**（DR2〜DR15 は予約レジスタ）。rx-rt の割り込み入口（dfpu feature）が退避するのが
    DR0・DR1 だけで、DR はすべて caller-saved（呼び出しをまたぐ値は退避される）。DR の退避・復帰は `dmov.d` の
    メモリ形（擬似命令 DST_D / DLD_D、dsp は 4 倍）、DR 間のコピーは `dmov.d dr, dr`。
    2 本では足りない式（dfpu.ll の `da_tree`）や、同じ入力を 2 回使う式（`da_chain` の `a`）は退避が入る。
    DR_PAIR_P は再実体化可能にしてあるが、入力の GPR が既に死んでいるので実際には使われない。
    DR0〜DR3 を割り付けると da_chain が 71 → 49 バイト、da_tree が 158 → 113 バイトになる（rx-rt で dr0-dr3 を退避する
    必要がある。今回は入れていない）。以前の方式（演算ごとに 28 バイトの擬似命令）では da_chain は約 85 バイト。
- 変換（2026-11-02、rx-rust #49）: IEEE の libcall と結果が同じになる条件で DFPU を使う（`flushesDenormals` = その型の
  非正規化数環境が preserve-sign）。
  - `fptosi` / `fptoui`（f64 → i32）→ `dtoi` / `dtou`: 0 方向の切り捨てで同じ。範囲外・NaN は LLVM では poison なので
    フラグは要らない。非正規化数はどちらも 0 になるが DDN = 0 だと例外になるので、double の環境が preserve-sign であること。
  - `fpext`（f32 → f64）→ `ftod`: 正確。float の非正規化数を 0 にするので float の環境が preserve-sign、NaN の
    ペイロードが libcall と違いうるので nnan。
  - `fptrunc`（f64 → f32）→ `dtof`: 最近接丸め（DPSW.RM = 0）・オーバーフローで無限大は libcall と同じ。非正規化数の
    入力・結果を 0 にするので double と float の両方が preserve-sign、NaN のため nnan。
  - i64 との変換、飽和変換（Rust の `as` は `fptosi.sat`）は libcall のまま。rustc は非正規化数環境を指定しないので、
    Rust のコードでは int → double 以外は使われない。
- DPSW などの DFPU 制御レジスタを予約レジスタに追加（暗黙の Uses が未定義使用として MachineVerifier に落とされるため）。
- 検証（QEMU に DFPU がないので実行はしていない）: `scripts/check-dfpu.sh` が `test/codegen/dfpu.ll` を rxv3 でコンパイルし、
  -verify-machineinstrs、GNU as でのアセンブル（直接出力の .o と同一。GNU as は itod / utod / ftod のレジスタ欄を
  入れ替える誤りがあるので、渡す .s ではその 2 つのレジスタ番号を入れ替えておく。NOTES-mc.md）、関数ごとの DFPU 命令列を
  `test/codegen/dfpu.expected`（マニュアルの意味と照合済み）と比較。四則・平方根・計算の連鎖・6 種の比較・select・分岐・
  int → double・呼び出しをまたぐ値、IEEE 側が libcall のままであること（nnan+ninf でも IEEE 環境の
  da_add_ieee / da_olt_ieee を含め 25 関数）。f64 の多数の演算（fops.ll の double 版）も
  rxv1〜rxv3 の -O0 / -O2 でコンパイル・アセンブルできることを確認。
- 検証（rx-rust #49 の後）: CDEF-inc/qemu の rxv3 / rxv3e で `dfpu.ll` + `dfpu_main.c` を -O0〜-O3、両経路で実行し
  7762 チェック全通過（GCC の soft-fp とビット単位で比較）。追加した関数: 3 つの中間値が同時に生きる `da_tree`
  （退避あり）、連鎖の先の比較・dtoi・dtof、ftod の後の連鎖、変換 4 種と IEEE 側の fpext / fptrunc（libcall で
  非正規化数を保つ）。float の非正規化数の ftod と、結果が非正規化数になる dtof が 0 になること。
  `check-dfpu.sh` の `dfpu.expected` は 36 関数。ループ・select・呼び出し・定数・変換が混ざる f64 の IR も
  rxv3 / rxv3e の -O0〜-O3 で -verify-machineinstrs を通り、GNU as でアセンブルできた。
- GNU as は dmov.d の dsp を 8 倍として符号化する（ハードウェアと LLVM は 4 倍。NOTES-qemu.md）。DR の退避で dmov.d の
  メモリ形を使うようになったので、run-codegen-test.sh と check-dfpu.sh が GNU as に渡す .s では
  `scripts/gas-dfpu.awk` が dsp 付きの dmov.d を `.byte` にする（ftod / itod / utod のレジスタ欄の入れ替えも同じ awk）。

## 乗算の上位半分（emul / emulu、rx-rust #58、2026-11-02）
- `emul` / `emulu src, Rd` は 32 × 32 → 64 ビットの積を Rd+1:Rd（Rd が下位）に書く。この組を表すレジスタクラスは
  無いので、SMUL_LOHI / UMUL_LOHI を擬似命令 SMUL_LOHI_P / UMUL_LOHI_P で選び、custom inserter が
  `R4 ← a; emul(u) b, R4（R5 を暗黙に定義）; lo ← R4; hi ← R5` に展開する。コピーはレジスタ割り付けが畳む。
  emul / emulu はフラグを変えない。
- MULHS / MULHU は Expand のままで、汎用の展開が *MUL_LOHI を使う。これで次が変わる（-mcpu=rxv3 -O2）。
  - 定数での除算・剰余 → 掛け算の列（`x / 10` は `emulu` + `shlr #3`。以前は `divu`）
  - `(u64)a * b`・mulhu / mulhs・32 ビットの overflow 付き乗算 → `emul(u)` 1 個（以前は `__muldi3`）
  - `u64 * u64` → `mul`・`emulu`・`mul`（以前は `__muldi3`）
  - `u64 / 10` などの一部の定数除算 → インライン展開（以前は `__udivdi3`）
- 即値形式（`emulu #imm, Rd`）は使っていない。定数は `mov.l` でレジスタに置く。
- 検証: `test/codegen/mulh.ll` + `mulh_main.c`（27 関数。除数を volatile にした GCC の除算・64 ビット積と比較、
  23,438 チェック）を各 CPU の -O0〜-O3、両経路で実行。`scripts/check-mulh.sh` が rxv1〜rxv3 で
  -verify-machineinstrs と GNU as / 直接出力の一致を見て、関数ごとの乗除算・呼び出し命令を `mulh.expected` と比べる。
- Rust: core-smoke（release）の .text が 18,936 → 18,844 バイト、`emul(u)` が 111 個。qemu-demo から `__muldi3`
  の呼び出しが消えた。

## ジャンプテーブル（rx-rust #59、2026-11-02）
- 以前は `setMinimumJumpTableEntries(UINT_MAX)` で無効にしていて、switch は全部比較と分岐の木だった。
  いまは LLVM の既定の判断（4 ケース以上で十分密なら表）に任せる。
- 形式は静的再配置モデルの既定（EK_BlockAddress）: `.rodata` に絶対アドレスの `.long` を並べ、
  `mov.l #LJTIn_m, rt; mov.l [ri, rt], rd; jmp rd`（レジスタ間接の mov.l は添字を 4 倍する）。範囲外は手前の
  `cmp` + `bgtu` で default へ。JumpTable は他のアドレスと同じく RXISD::Wrapper → `mov.l #imm32`。
  `brind` のパターン（`jmp Rs`）もこのとき足したので、indirectbr（計算型 goto）も通る。
- 表のラベルは `LJTI…`（DataLayout の `m:o` のため `.L` ではなく `L`）。`m:o` は MachO の扱いなので、表の直前に
  使われない `lJTI…` ラベルも出る。どちらもローカルシンボルで、動作には関係ない。
- GNU as は `.rodata + オフセット` を参照するときの加数を命令や表のバイト列に入れ、`llc -filetype=obj` は
  再配置の r_addend に入れる。リンク前のバイト列は違うが、リンク後は同じ（`check-jt.sh` はリンクしてから比べる）。
- 検証: `test/codegen/jt.ll` + `jt_main.c`（密、オフセット付きで穴あり、負のケース値、u8、1 関数に表 2 つ、疎、
  indirectbr。同じ switch を C で書いたものと、全ケース値・その前後・遠い値で比較、2,564 チェック）を各 CPU の
  -O0〜-O3、両経路で実行。`scripts/check-jt.sh` が rxv1〜rxv3 で -verify-machineinstrs とリンク後の一致を見て、
  表を使う関数を `jt.expected` と比べる。
- Rust: qemu-demo（release）に表が 1 つでき、.text が 4,133 → 4,151 バイト（rxv3、emul と合わせた値）。

## 未対応（M4 以降）
- LLVM 側で定義する可変長関数（va_start / va_arg）（rx-rust #51）
- 動的 alloca、フレームポインタ（rx-rust #51）

DFPU のコードと、アトミックの RXv2 経路（movli/movco ループ）は、CDEF-inc/qemu の rxv2 以降のコアと
実機の RX72M で実行して確かめた（NOTES-qemu.md、NOTES-hw.md）。

## プロローグとエピローグ（pushm / rtsd、2026-11-01）
- callee-saved（R6〜R13）は 1 つの範囲として保存する。determineCalleeSaves で、使うレジスタの最小〜最大の間を全部足す
  （r6 と r8 を使うなら r7 も保存する。GCC も同じ）。範囲は pushm 1 命令、1 本なら push.l（GNU as も `pushm r6-r6` を
  push.l に置き換える）。pushm は小さい番号を低いアドレスに置く。
- 戻りは rtsd: `rtsd #imm, lo-hi` は SP を imm − 4×本数 だけ戻して lo〜hi を復帰し、戻る。imm はローカル + レジスタで、
  最大 1020。超えるときは `add #ローカル, r0` のあとに `rtsd #4×本数, lo-hi`。レジスタが無ければ `rtsd #ローカル`
  （超えるか 0 なら add + rts）。rts の暗黙オペランド（戻り値のレジスタ）は rtsd に移す。
- popm は終端が rts でないブロック用に restoreCalleeSavedRegisters が出す（いまは末尾呼び出しも shrink-wrapping も
  無いので、実際には rtsd に畳まれる）。
- 検証: `test/codegen/frame.ll` + `frame_main.c`（GCC の呼び出し側が R6〜R13 に値を置いたまま呼ぶ。範囲の穴、1 本、
  8 本、1020 バイトの境目、関数の途中のエピローグ、スタック渡しの引数、i64 の戻り値）を各 CPU の -O0 / -O2 で実行し、`scripts/check-frame.sh`
  で関数ごとの命令列を `frame.expected` と比較。復帰を 1 本減らした版では frame_main.c が失敗することも確かめた。
- 非公開の RTOS アプリケーション（QEMU 向け ELF）で .text が 16,491 → 15,825 バイト（−4.0%）、push.l が 211 → 5 本（2026-11-01 に
  target を消して作り直した値。rx-rust #44 に書いた 16,188 バイト・106 本は、cargo が古いオブジェクトを使い回した値だった。
  rustc を作り直してもバージョン文字列が変わらないと cargo は再コンパイルしない）。

## CFI（2026-11-01）
- デバッグ情報がある関数（rustc の `debug = true`、llc の `-force-dwarf-frame-section`）は .debug_frame、uwtable の関数は
  .eh_frame を持つ（RXMCAsmInfo の `UsesCFIWithoutEH`。panic=abort なので例外処理は無い）。どちらも無ければ何も出さず、
  コードもデバッグ情報の有無で変わらない。
- CIE は GCC（rx-elf-gcc -g）と同じ: 戻り先の列は 17（PC）、初期状態は `CFA = r0 + 4`、`PC = [CFA − 4]`。
  DWARF 番号は R0〜R15 = 0〜15、PSW = 16、PC = 17。
- プロローグ: push / pushm の後に `.cfi_def_cfa_offset 4 + 4×本数` と各レジスタの `.cfi_offset`（小さい番号が低いアドレス）、
  `add #-N, r0` の後に `.cfi_def_cfa_offset 4 + フレームサイズ`。
- エピローグ（2026-11-02、rx-rust #60）: 復帰しないで SP を動かす命令の後に CFI を置く。rtsd の前では、フレームが
  1020 バイトを超えるときの `add #N, r0` の後の `.cfi_def_cfa_offset 4 + 退避バイト数` だけ（rtsd は解放・復帰・戻りを
  1 命令でするので、その後に書く位置が無い。rtsd 1 本のエピローグは CFI 無し）。終端が rts でないブロック用の popm の
  経路では、popm の後に `.cfi_def_cfa_offset 4` と各レジスタの `.cfi_restore` も書く（いまは通らない）。
  GCC はエピローグを記述しないので、add と rtsd の間で止めると GCC のコードでは CFA がずれる。
- 関数の途中にあるエピローグ（早期 return が後ろのブロックより前に置かれたとき）は、後ろのブロックをフレームのある
  状態に戻す必要がある。LLVM の CFIFixup パス（RXTargetMachine の `setCFIFixup(true)`）が、プロローグの最後の CFI の
  後に `.cfi_remember_state`、エピローグの後に続くブロックの先頭に `.cfi_restore_state` を入れる。CFIFixup は
  FrameSetup / FrameDestroy の印の付いた CFI でプロローグとエピローグを見分けるので、エピローグの CFI には FrameDestroy を
  付ける。CFI の無いエピローグ（rtsd 1 本）のブロックは「フレームあり」のまま扱われ、何も入らない（それで正しい）。
- GNU as は `.cfi_*` を受け付けない（"CFI is not supported for this target"。GCC は .debug_frame のバイト列を自分で書く）。
  対応しないことにした: rustc と `llc -filetype=obj` は統合アセンブラなので影響が無く、GNU as を通すのは
  run-codegen-test.sh・check-frame.sh などの CFI 無しの .s だけ。`-g` 付きの `.s` を GNU as でアセンブルしたいときは、
  llc に `-filetype=obj` を使う。
- 検証:
  - `scripts/check-frame.sh` が frame.ll を CFI 付きでもコンパイルし、コードが変わらないこと、各命令での CFA（命令から
    求めた値と CFI の値。remember / restore_state も追う）と各レジスタの位置が合うこと（rxv1〜rxv3、-O0 / -O2）、
    .debug_frame が `frame-cfi.expected` と一致することを確かめる。frame.ll には関数の途中にエピローグがある関数
    （`early_1024`: 退避あり、`early_big`: フレームだけ 1 KB 超、`early_small`: rtsd 1 本）がある。CFI をわざと壊した
    3 通り（位置、CFA、.cfi_offset の欠落）と、`-disable-cfi-fixup`（restore_state が無い）を検出することも確かめた。
  - QEMU の gdbstub と rx-elf-gdb（e2 studio 同梱の 16.2）で、frame.ll の関数の入口、pushm の直後、GCC の関数
    `touch` の中から `bt` で main までたどれ、`frame 1` の r6〜r13 と SP が退避した値と一致した（1 KB を超えるフレームも）。
    エピローグの add と rtsd / rts の間、および途中のエピローグの直後のブロックの先頭（frame_1024、frame_big、
    early_1024、early_big、rxv1 -O2）でも `bt` が main までたどれ、`frame 1` の SP が呼び出し前の値と一致した。
  - 同じ RTOS アプリケーション（release、`debug = true`）は .debug_frame を持ち、デバッグ情報なしで作った ELF と命令が同じ
    （関数の並びだけが違う。シンボルのハッシュが変わるため）。
  - これを QEMU の gdbstub で止めると、スケジューラの関数からタスクの入口関数まで、タイマー割り込みの handler から
    `rx_isr_common` まで、インライン展開された関数を含めてたどれた。ただしこれは CFI ではなく gdb の
    プロローグ解析によるものだった（下記「e2 studio の gdb と rustc の .debug_frame」）。

### 割り込み入口の CFI（rx-rt、2026-11-01）
- `interrupt!` の入口（`_rx_vector_N`）と `_rx_isr_common` は `global_asm!` に `.cfi_*` を書く（統合アセンブラなので通る）。
  受付時に CPU が PSW、PC の順に積むので、入口では `CFA = SP + 8`、PSW = [CFA − 4]、PC（戻り先の列 17）= [CFA − 8]。
  `pushm r1-r5` と `pushm r14-r15` の後は CFA = SP + 36 で、R5〜R1 が CFA − 12〜−28、R15・R14 が CFA − 32・−36。
  `_rx_isr_common` はこの状態から始まり、FPSW、DR0〜DR1・DPSW・DCMR、アキュムレータの push / pop ごとに
  `.cfi_adjust_cfa_offset` を書く（2 本の dpushm をまとめて書くと間の 1 命令で CFA がずれる。check-isr-cfi.py が検出した）。
  DR0・DR1 は DWARF 番号が無いので位置は書かない。
- 割り込まれたコードの SP（R0）: PSW.U = 0 なら CFA（ISP 上で受け付けた）だが、PSW.U = 1 ならユーザスタック（USP）で、
  DWARF には USP の番号が無い（gdb の rx_dwarf_reg_to_regnum も 0〜17 だけ）。そこで `_rx_isr_common` の先頭で USP を
  積み（`mvfc usp, r1` + `push.l r1`、CFA − 40）、R0 を `DW_CFA_val_expression` で `PSW.U ? [CFA − 40] : CFA` と書く
  （`.cfi_escape`。式はソースのコメント参照）。割り込み 1 回あたり 3 命令（復帰側は `add #4, r0`）・4 バイトのスタック増。
  RTOS のタスクは USP で動くことが多く、その場合これが無いとタスクのフレームより先が狂う。
- gdb の RX 移植（rx-tdep.c）は unwinder を「rx exception」→ dwarf2 →「rx prologue」の順に試し、関数の中に `rte` が
  あると（現在の PC から関数末尾まで命令を読む）CFI より先に自前の解析で割り込みフレームとして扱う。その解析は関数の
  先頭から push を数えるので、入口（`_rx_vector_N`）で積んだ 36 バイトが見えず、PC を誤る。`rte` に別のシンボル
  `_rx_isr_common_rte` を付けて `_rx_isr_common` を `rte` の手前で終わらせ、dwarf2 に回す（`rte` そのものの上で止めたときは
  自前の解析でも正しい: SP + 8、PSW.U なら USP）。
- `.cfi_sections .eh_frame, .debug_frame`: デバッグ情報のあるオブジェクトでは rustc（AsmPrinter）の `.cfi_sections
  .debug_frame` が後から上書きする。uwtable の関数がある場合に .eh_frame を消さないよう両方を指定する（link.x は
  .eh_frame を捨てる）。`.cfi_signal_frame` は .eh_frame の CIE にしか残らない（.debug_frame の CIE には拡張文字列が無い）
  ので、gdb は割り込まれた PC − 1 で FDE を探す。関数の先頭の命令で割り込まれたときだけ、その 1 フレームの関数名がずれる。
- 検証:
  - `scripts/check-isr-cfi.py ELF` が逆アセンブルで SP を追い、各命令の CFA と、`jsr` での PSW・PC・R1〜R5・R14・R15 の
    位置と R0 の式を .debug_frame（llvm-dwarfdump）と比べる。run-all-tests.sh が isr-save（`save-acc` 付き）の
    ELF に対して CPU・プロファイルごとに実行する（rx62n: FPU + RXv1 ACC、rxv2: FPU + A0/A1、rxv3: + DFPU）。CFA を
    1 つ変えた ELF と R0 の式を壊した ELF を検出することも確かめた。
  - USP で動くタスクを持つ RTOS アプリケーション（release）を QEMU の gdbstub でタイマー割り込みの handler の中で止め、
    e2 studio の rx-elf-gdb（下記の producer の変更後）で `bt` が handler → … → `interrupt!` の入口 → `rx_isr_common`
    → 割り込まれた関数（インライン）→ タスクの入口関数 → タスクの初期フレーム（RTOS が積んだ値。その先は PC = 0 で
    止まる）とたどれた。タスクのフレームの SP = USP、PSW = 0x30001、R1 / R5 / R14 = 0x11111111 / 0x55555555 /
    0xeeeeeeee（割り込まれた関数が置いた値）。変更前は `rx_isr_common` の次が PC = 0x102 などのごみだった。

### e2 studio の gdb と rustc の .debug_frame
- e2 studio 同梱の rx-elf-gdb（`GNU gdb (GDB) 16.2-20260702-a2e38af0`、llvm-gcc-renesas.com のビルド）は、rustc が出した
  ELF の .debug_frame を丸ごと捨てる（`set complaints` で "Corrupt data in …:.debug_frame"）。その結果、rustc の ELF では
  CFI が一切使われず、通常の関数は「rx prologue」、`rte` のある関数は「rx exception」の命令解析でたどっている。
- 原因は DW_AT_producer: rustc の CU の producer は `clang LLVM (rustc version …)`。同じ長さの文字列に書き換えて
  試すと、`clang` か `LLVM` で**始まる**と捨てられ（`clang xxxx (rustc`、`LLVM (rustc`、`LLVMx xxxx (rustc`）、それ以外は
  受け付ける（`xxxxx LLVM (rustc`、`rustc LLVM (rustc`、`xclang LLVM(rustc`、`GNU C 17.1 (rustc`）。.debug_frame の
  中身は関係ない（GCC の .debug_frame のバイト列を rustc の ELF に移しても捨てられ、.debug_info・.debug_abbrev・
  .debug_str のどれかを消すと受け付ける。CIE を v3 にしても、.eh_frame 形式に直しても駄目）。upstream の gdb 16 の
  dwarf2/frame.c には producer による分岐が無いので、Renesas の追加の変更と思われる。llc + GCC でリンクした frame.ll
  には CU が無いので影響を受けない。
- 対処（rust #18）: rx 向けの rustc は producer を `rustc version … (LLVM)` にする（rustc_codegen_llvm の
  debuginfo/metadata.rs。ほかのターゲットは `clang LLVM (…)` のまま）。upstream の gdb 16 が "clang " を見るのは
  producer_is_llvm だけで、使うのは x86 のプロローグ解析だけ（producer_is_clang は "clang version " で始まるものだけ）
  なので、RX では upstream の gdb の動作は変わらない。e2 studio の gdb では、書き換えていない ELF で .debug_frame が使われ、
  上の `bt`（タスクまで）、行番号つきのブレークポイント、`info line`、引数と構造体（`p *self`）の表示を
  確かめた。`check-isr-cfi.py` が producer の先頭も確かめる。
- タスクの入口関数より先は、RTOS がタスクのスタックの上端の外に残した値をたどるので、
  初期フレームと PC = 0 で止まるか、"previous frame inner to this frame" で止まる（RTOS 側の作り。入口の戻り先に
  0 を置けば止まる）。
