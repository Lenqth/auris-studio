<a id="real-recording-calibration-of-physical-choir"></a>
# Physical Choir の実録音キャリブレーション

[English](choir-copy-synthesis.md)

無歌詞コーラスの工場設定の声道形状、唇放射、Tone、Breath、Attack は、48 kHzで実際の Rust 楽器を使い、**VocalSet** の持続母音に合わせて調整します。損失は、ゲインに依存しない固定の時間対数メル距離です。録音と Python 依存関係は開発用アセットであり、アプリケーションが使うのは `auris-synth` に組み込まれた定数です。

<a id="reference-and-split"></a>
## 参照データと分割

[VocalSet](https://doi.org/10.5281/zenodo.1442513) は Julia Wilkins、Prem Seetharaman、Alison Wahl、Bryan Pardo によるデータセットで、プロの歌手が持続母音などの発声練習を行います。録音は [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) でライセンスされています。この実験では、その記録に添付された `VocalSet11.zip` アーカイブのうち、20人の歌手全員による /u/、/a/、/i/ のモノラル `long_tones/straight` 録音60本を使います。

`tools/eval/references/choir-captures.json` には、アーカイブ、ZIP オフセット、CRC、元ファイルの SHA-256 値を固定します。HTTP range リクエストで選択したエントリだけを取得します。音声は選択した開発ディレクトリ内に置き、ソース／ライセンス告知も保存します。

抽出器は各録音から信頼できる1.2秒の持続音を最大2つ取得します。YIN は24 kHz、4096サンプルの窓で周期性を測定します。オクターブ検査では、長い周期を受け入れる前に奇数倍音のスペクトル証拠も要求します。これにより、強い第2倍音がガイドの基音になるのを防ぎます。付属の C/F 練習グループは、自然なビブラートを断片化せずにレジスター変更を分離します。最大100 msの周期性の隙間を平滑化し、音符には少なくとも1.2秒の安定した区間が必要です。オンセットは局所 RMS の10%までバックトラックします。録音をピッチシフト、タイムストレッチ、ループ、フレームごとの正規化はしません。

固定した結果は **104音** です。歌手 f1、f5、m1、m2 が、ソプラノ、メゾソプラノ、バリトン、テノールを含む訓練音24個を提供します。残り16人の歌手が検証音80個を提供します。オプティマイザーは訓練音声だけを読み込みます。検証音声はパラメーター選択後に別の比較コマンドが読み込みます。`choir-notes.json` には抽出時刻、音符ハッシュ、ピッチガイドのフィンガープリントを保存し、ツールはそのコホートや分割の変更を拒否します。

Voice Size は、データセットの声種メタデータから得る固定ヒューリスティックで、ソプラノの0.15からバスの0.95までです。解剖学的な測定でも、歌手ごとにフィットしたパラメーターでもありません。両バージョンには、20 ms間隔の参照専用 YIN ピッチガイドを同じように与えます。この診断では Ensemble、Width、Vibrato を0にするため、フィットは一貫した声道／音源の音声を測ります。別の工場アンサンブル検査では、各バージョンの通常のアンサンブルとビブラートを復元し、ガイドを省略します。

<a id="objective-and-search"></a>
## 目的と探索

指標には `tools/eval/physical_mel.py` を再利用します。生成した PCM を24 kHzにダウンサンプルし、各音符全体を RMS 0.1 に正規化してから、512／2048サンプルの窓と240サンプルのホップで Hann STFT を取ります。各解像度には30 Hz〜10 kHzの64個の面積正規化 HTK メル帯域があり、`log(1 + 10000 * mel_power)` として圧縮します。最初の15フレームと残りのサステインの L1 誤差には同じ重みを与えます。解像度と音符は平均します。これは無次元の音響距離であり、dB 誤差でも知覚品質スコアでもありません。

`choir_mel.py fit` は、シード20261004、次元ごとに候補3つの母集団、6世代、研磨なしの差分進化を使います。2ラウンドで、共有ソース／放射フィットと母音ごとの個別声道形状フィットを交互に行います。

| 変数 | 範囲 |
| --- | --- |
| Tone | 0–1 |
| Breath | 0–0.5 |
| Attack | 10–500 ms |
| Radiation corner | 30–4000 Hz、対数 |
| 母音ごとの声道面積7個 | 初期面積の1/3–3倍、0.1–10 cm²に切り詰め |

最初の面積は固定します。すべての面積の一様スケーリングは圧力から流量への変換で相殺され、探索次元を無駄にするためです。初期形状と音源制御は `choir-mel-initial.json` に固定されています。開発専用の `choir-calibration` feature では、候補面積、放射、出力ゲインを本番 DSP に通せます。フィット前の訓練 PCM を正確に再現できますが、合成用の Python 代理モデルはありません。

1,066回の損失評価後、フィットした値を丸めて Rust 定数にしました。3つの出力ゲインは、フィット前の訓練音の母音ごとの RMS 中央値を保ちます。これらは正規化した損失とは別で、検証音声は使っていません。既存の保存済みパラメーター値は明示的なままとし、工場デフォルトは新しい楽器に適用されます。

<a id="results-at-48-khz"></a>
## 48 kHzでの結果

以下は、丸めた定数と出力正規化を含む、コンパイル済みの工場楽器から得た値です。音符ごとの完全な損失と実行ファイルのハッシュは、コホートのロックとともに `choir-mel-calibration.json` と `choir-mel-validation-48k.json` にあります。

| コホート／母音 | 音符 | 変更前 | 変更後 | 平均削減率 |
| --- | ---: | ---: | ---: | ---: |
| Training, guided voice | 24 | 0.089050 | 0.071616 | 19.6% |
| Validation, guided voice | 80 | 0.097766 | 0.085592 | 12.5% |
| Validation / oo | 26 | 0.086153 | 0.076982 | 10.6% |
| Validation / ah | 29 | 0.121168 | 0.102043 | 15.8% |
| Validation / ee | 25 | 0.082696 | 0.075463 | 8.7% |
| Validation, factory ensemble | 80 | 0.106988 | 0.098582 | 7.9% |

平均はすべての母音で改善しますが、個別の音符には悪化するものもあります。これは、部屋で録音した完全なコーラスではなく、プロの歌手それぞれの音声です。短い持続抜粋が評価するのはオンセットと母音スペクトルであり、長いフレーズのダイナミクス、リリースの自然さ、アンサンブルの混ざり、聴取者の好みではありません。リバーブは楽器の外側にある通常のエフェクトのままです。

事前に選んだ聴取ケースは f2 と m3 で、各母音の最初に選んだ音符を recorded/before/after の順に並べます。比較では26.1秒の WAV、帰属／順序マニフェスト、共通スケールの対数メルプロットを書き出します。聴取用コピーだけに RMS 正規化、共有ピークヘッドルーム、5 msの終端フェードを適用します。測定する PCM にはこのようなフェードを適用しません。

<a id="reproduce"></a>
## 再現

```sh
uv run tools/eval/fetch_choir_reference.py target/choir-mel/vocalset
cargo build --release -p auris-synth --example physical_fit_render --features choir-calibration
uv run tools/eval/choir_mel.py fit target/choir-mel/vocalset target/release/examples/physical_fit_render target/choir-mel/fit.json
```

Windows では実行ファイル名に `.exe` を付け、`cp` の代わりに `Copy-Item` を使います。キャリブレーションを有効にした worker は、工場定数が変わった後でも、固定した初期形状と音源制御を再生できます。フィットの上書きなしで最終 worker をビルドする前に、コピーを保持してください。

```sh
cp target/release/examples/physical_fit_render target/choir-mel/calibration-renderer
cargo build --release -p auris-synth --example physical_fit_render
uv run tools/eval/choir_mel.py compare target/choir-mel/vocalset target/choir-mel/calibration-renderer target/release/examples/physical_fit_render target/choir-mel/comparison --initial-profile
cargo test -p auris-synth --all-targets --features choir-calibration
uv run --with numpy==2.5.3 --with scipy==1.18.1 --with soundfile==0.14.0 --with pytest pytest tools/eval/test_choir_mel.py tools/eval/test_physical_mel.py
uv run --with ruff ruff check tools/eval/choir_mel.py tools/eval/choir_reference.py tools/eval/fetch_choir_reference.py tools/eval/test_choir_mel.py
```

測定には Python 3.12、NumPy 2.5.3、SciPy 1.18.1、SoundFile 0.14.0 を使います。音声定数を変更する前後に、[evaluation.md](evaluation.ja.md) のシンボリックルーラーと Audiobox 評価を実行してください。このコーラスキャリブレーション後も、出荷する9つのプリセットは WAV ハッシュ、シンボリック行、学習済みスコアが同一です。
