# RX 呼び出し規約（GCC 14.2 rx-elf から抽出、実測で要確認の項目に [要実測]）

参照元: `gnu/src/gcc-14.2.0/gcc/config/rx/rx.h`, `rx.cc`

## レジスタ
| reg | 役割 | 保存 |
|---|---|---|
| R0 | SP | 固定 |
| R1 | 戻り値 / 第1引数 | caller |
| R2–R4 | 引数 / 戻り値(〜16B) | caller |
| R5 | テンポラリ | caller |
| R6 | FP（使う場合） | callee |
| R7–R13 | 汎用 | callee |
| R14 | テンポラリ | caller |
| R15 | 構造体戻り値アドレス | caller |
| PSW(CC) | フラグ | 固定 |

- R8=static chain, R9=trampoline temp（Rust では基本不要）。
- **GCC rx は DFPU レジスタ (DR0–15) を扱わない**（FIXED_REGISTERS は 17 個のみ）。
  double は整数レジスタペアで受け渡し（`-m64bit-doubles` 時）。→ LLVM でも ABI 上は f64 = R ペア、
  DFPU 命令による演算は後段の最適化として扱う。

## 引数
- 引数はバイト累積で R1→R4（計 16B）に詰める。1 つの引数が残りに収まらなければスタックへ。
- 累積カウンタはスタックに積んだ引数でも進む（rx_function_arg_advance）。[要実測: 後続の小さい引数がレジスタに戻るか]
- 集約型でサイズが 4 の倍数でないものはスタック渡し。
- 可変長引数: 無名引数はスタック。[要実測: 最後の名前付き引数もスタックか]
- スタック上引数のアラインメント: 既定（RX ABI）は自然アラインメント、`-mgcc-abi` で 4B。[要実測: 既定がどちらか]
- スタックは下方向、STACK_BOUNDARY=32bit、FIRST_PARM_OFFSET=0。

## 戻り値
- 4B 未満の整数は int に昇格して R1。
- 集約型: サイズ 1〜16B かつ 4 の倍数なら R1–R4、それ以外はメモリ（アドレスは R15 で渡す）。

## 型
- char は unsigned、size_t = unsigned long(32bit)、wchar_t = long。
- BIGGEST_ALIGNMENT=32bit → i64/f64 のアラインメントは 4B [要実測]。Rust の data layout に反映すること。
- 非アラインアクセス可（STRICT_ALIGNMENT=0）。
