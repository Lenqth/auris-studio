<a id="physical-instruments-in-the-bass-register"></a>
# 低音域の物理楽器

[English](physical-low-register.md)

2026-10-21 の修正では、Physical Strings の金属的な低音アタックと、Physical Piano の過剰な非調和性に対処しました。ここで MIDI 24/36 は C1/C2 を表し、A4 = MIDI 69 = 440 Hz です。元のレンダラーは `2c4bcfd1` です。

<a id="bow-scattering"></a>
## 弓の散乱

以前は、相対的な弓／弦速度が静止限界を越えると力が不連続に跳びました。たとえば回帰プローブでは、入力の変化 0.0000002 によって出力力が 0.0207999 から 0.0058681 に変化しました。このような不連続が繰り返され、低音アタックで弓とブリッジの間の短い区間が励起されていました。ボディ放射をバイパスしても、不安定な起動はなくなりませんでした。

現在の接合部は、[bow-table の定式化](https://www.dsprelated.com/freebooks/pasp/Bowed_Strings.html)と、同じ方式を使う [STK](https://github.com/thestk/stk/blob/master/include/BowTable.h) に従った、連続かつ有界な散乱係数を使います。圧力は固着範囲を制御します。硬さはスライド係数の三次減衰と四次減衰をブレンドします。係数は0から1の間に保たれ、動いている弓に対する受動性を維持します。実装は算術演算だけで動き、コールバック内で新たな割り当て、I/O、ロックを行いません。

実効ブリッジ位相補償は、一極 DC 遅延の0.82倍です。これは広帯域の弓奏リターンに対する経験的な調整です。連続接合部で DC を完全に補償すると、高音域が約10セント鋭くなりました。同じ補償を、励振、ベンド、グライド、ライブの損失変更にも適用します。MIDI 69/85/88 の工場サステインプローブは、44.1/48/96 kHz で4セント以内に収まります。既存の基音レベルと上中域のガードも引き続き有効です。

<a id="piano-dispersion"></a>
## ピアノの分散

ピアノの工場非調和性係数は、広いメル帯域を使って C3〜G4 でフィットされていました。この係数を巻線の低音弦にそのまま使うと、倍音が過度に引き伸ばされました。これは弓奏弦の力の不連続とは別の原因です。

University of Iowa の mf Steinway 録音から得た第1〜第12部分音を使うと、非調和性の推定値は C1 で約0.000256、C2 で約0.000144です。各録音は固有のフィット済み基音を保持するため、録音のチューニングを分散と取り違えません。係数のフィットには、第1部分音で正規化した硬い弦の関係式を使います。

低音カーブは公開スティフネス制御を C1 で0.35倍、C2 で0.20倍にスケールし、既存の C3 キャリブレーションとは1倍の地点で接続します。三次 smoothstep による接続でレジスター境界の急変を避け、乗数は C1 より下で有界に保ちます。スティフネスがゼロなら、伸縮されていない弦になります。C3 以上の音符はベースラインと同じサンプルを保持します。

3つの生の基準ハッシュ、オンセット位置、部分音周波数、フィット残差は [測定レポート](../tools/eval/references/physical-low-register.json) に保存されています。これらの少数の基準音符は低音の分散を制約するもので、楽器全体の音響忠実度を示すものではありません。

<a id="measurements-and-listening"></a>
## 測定と聴取

ペア比較では、7つのパッチ（grand、felt、bright piano、violin、viola、cello、contrabass）、MIDI 24/30/36/48/69/85/88、ベロシティ 0.35/0.8、24/48/96 kHz を対象とし、294条件あります。各音符は3秒間持続し、2.5秒でリリースします。メモリ使用量を抑えるため、レンダーは逐次実行します。無関係な物理モデルと C3 以上のピアノ音符には、サンプルが同一の対照条件があります。

48 kHz、ベロシティ0.8での工場値:

| 測定 | 変更前 | 変更後 |
| --- | ---: | ---: |
| Piano C1 第8部分音の伸び | 40.1 cents | 14.0 cents |
| Piano C2 第8部分音の伸び | 40.1 cents | 8.2 cents |
| Strings C1 アタック1周期相関 | 0.792 | 0.975 |
| Strings C2 アタック1周期相関 | 0.942 | 0.995 |
| Strings C1 アタック上域／基音パワー | 13.5 dB | 4.9 dB |
| Strings C2 アタック上域／基音パワー | 5.2 dB | 7.7 dB |

アタック測定は300〜800 msを使います。相関では DC を除去し、演奏した周期の0.5%以内の遅延を探索します。上域は第5〜40倍音を含み、5 kHzを上限とします。これは最小化する目的ではなく、診断用の値です。C2 の上域増加は表に残しています。この修正が改善するのは周期性であり、すべての明るさの指標ではないためです。MIDI 24/30/36 の弓奏条件72件すべてで、修正後のアタック相関は0.975を超えます。

新しい摩擦カーブはレベルも変えます。デフォルトの C1/C2 サステイン RMS は約5.5 dB 増加します。公開レベルパラメーターの意味は変わりません。聴取ファイルには、各デフォルト C1/C2 楽器について共通ゲインのペアと、RMS を一致させた **before / gap / after** ファイルが含まれます。聴取境界には5 msのフェードを入れ、リミッターや EQ は適用しません。ゲインを変更していない評価 WAV の生データは、出力の `before/` と `after/` ディレクトリにあります。

シンボリックルーラーは変更ありません。学習済み聴覚評価は変更前後で試みましたが、初回実行はワークステーションのクラッシュで中断され、制限付きのオフライン再試行でも固定されたモデルチェックポイントをすべて取得できませんでした。TuneJury や MuQ のスコアは主張しません。波形診断が示すのは特定の回帰であり、聴取上の好みではありません。

検証完了: `cargo test -p auris-synth --all-targets -j 1` による207個のライブラリテストと8個の example-worker テスト、16個の Python 測定テスト、全ターゲットのワークスペース Clippy、Rust のフォーマット、Ruff。ワークスペース全体のテストは初回の UI 依存関係ビルド中に停止したため、合格とは報告していません。

```sh
cargo build -p auris-synth --example physical_fit_render --release -j 1
# Archive the original worker before changing DSP.
uv run tools/eval/physical_low_register.py before.exe after.exe target/low-register/check
# Add --reference-dir DIR when DIR contains Piano.mf.C1/C2/C3.aiff from Iowa.
uv run --with pytest --with numpy --with scipy --with soundfile pytest tools/eval/test_physical_low_register.py
uv run --with ruff ruff check tools/eval/physical_low_register.py tools/eval/test_physical_low_register.py
```
