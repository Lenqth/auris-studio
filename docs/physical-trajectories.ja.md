<a id="sustained-and-moving-pitch-copy-synthesis"></a>
# 持続音と移動ピッチのコピー合成

[English](physical-trajectories.md)

これは [1秒の実録音較正](physical-copy-synthesis.ja.md) の続きです。オフラインワーカーは1つの励振を維持しながら、サンプル時刻のピッチベンドと表情を適用します。生産時サンプルレートを含む実際の Rust 楽器をレンダリングします。最適化器と録音は開発用で、リリースに含まれるのはモデルコードと小さな係数だけです。

<a id="references-and-supplied-performance"></a>
## 参照と提供された演奏

固定コホートには、元の短音165個に212個の抜粋を加えます。

| 参照 | 抜粋 | 学習 | 検証 |
| --- | ---: | ---: | ---: |
| Iowa ピアノ、3秒・6秒 | 36 | 12 | 24 |
| Iowa ギター、3秒・6秒 | 98 | 48 | 50 |
| Iowa バイオリン、3秒・6秒 | 54 | 19 | 35 |
| TU-Note バイオリン・グリッサンド、リリース込み約6〜10秒 | 24 | 12 | 12 |

Iowa は元のピッチ／ダイナミクス分割を使います。元のピッチ検査済み安定領域が対象時間を100 ms以上超える場合だけ、その長さを含めます。短い録音は除外として記録し、クリップを引き伸ばしたりループしたり次のノートへ延長したりしません。Iowa のノートオフ時刻には注釈がないため、これらの窓は終端まで保持してレンダリングし、ピアノペダルのタイミングや推定リリース表情の検証には使えません。

[TU-Note](https://depositonce.tu-berlin.de/handle/11303/7527)（Henrik von Coler、Jonas Margraf、Paul Schuladen、バイオリニスト Michiko Feuerlein）は、ビブラートの有無、mp と ff の実際の上行／下行5半音グリッサンドを提供します。DPA マイク録音を使い、アタック、遷移、リリース境界を手作業でラベル付けします。G／D弦を学習に使い、A／E弦はこの最適化から外します。これにより Iowa とは異なる奏者、楽器、録音セッションが加わりますが、任意の奏者や録音環境への一般化を示すものではありません。

TU-Note は [CC BY-ND 4.0](https://creativecommons.org/licenses/by-nd/4.0/) です。ツールは選択したアーカイブメンバーだけを範囲付きHTTPで取得し、サイズ、CRC、SHA-256を検証します。原録音とローカル処理した抜粋は無視対象の開発ディレクトリに置きます。リポジトリにはメタデータ、制御ハッシュ、係数、数値結果を保存します。試聴モンタージュは TU-Note には合成レンダーを使い、Iowa には Iowa の制限のない条件で録音を使います。

バイオリンでは各参照から演奏情報を提供します。

* 基本周波数は4096サンプルの Hann 窓、10 msホップ、既知のサブオクターブ区間での対数放物線スペクトルピーク補間から推定します。欠落したアタック／無音フレームは有声音の測定値から補間します。これにより演奏されたビブラートとポルタメントを残しますが、多声音転写ではありません。合成PCMでも有声音カバレッジを含めてピッチ追跡を再測定します。
* 表情は20 ms RMS、60 msのガウス平滑化幅、保持区間の95パーセンタイルレベル、50 msの制御点を使います。相対振幅の平方根は CC11 が弓の動きと出力ゲインの両方に与える影響を近似します。これは固定された演奏ガイドであり、検証録音ごとに最適化したものでも、通常再生中に自動推定したものでもありません。
* TU のノートオフは WAV の終端ではなく注釈されたリリース開始です。ピッチ／CCイベントは厳密にその前に置きます。Rust の `f32` へ変換する前に小数丸めを保護します。

すべてのビルド比較には同一のガイドを渡します。別の固定表情アブレーションで、変更した工場音色ではなく提供された表情による改善量を調べます。全音符正規化は録音ゲインを除きますが、演奏されたクレッシェンドは除きません。

<a id="objective-and-calibration"></a>
## 目的と較正

2解像度、64帯域の log-mel 表現は変更していません。長い抜粋にはアタック、初期サステイン、後期サステイン、ピッチ遷移、リリースの損失を別々に計算し、空でない位相に対して0.15／0.15／0.40／0.20／0.10を再正規化して重み付けします。抜粋の内部に厳密にある特徴中心だけを数え、境界からノートオフやリリース位相を作りません。正しいアタックが数秒の誤ったサステインを隠さないようにします。レポートには dB のエンベロープ誤差、セントのピッチ誤差、有声音カバレッジ、ノートごとの各位相損失も残します。有声音出力がない場合はピッチに100セントのペナルティを加えます。ピッチとエンベロープの測定は診断であり、別の適合目標ではありません。

データセットの重みは短い Iowa 0.3、長い Iowa 0.5、TU-Note 0.2で、ピアノ／ギターでは再正規化します。旧短音目的関数は回帰成分として残します。シード付き差分進化で励振／損失制御を適合し、固定ローカル座標スケジュールで最良の学習点の周囲を改良します。正則化した12帯域の色付け案は、主位相バランス mel 目的関数が改善した場合だけ採用します。検証録音から検索パラメータを選ぶことはありません。提供された演奏ガイドは、この条件付き合成タスクへの入力です。

バイオリンの検索にはリリースと `bow_response` を加えます。Bow response は速度ランプと出力表情平滑化器を制御し、その時定数は半分です。これにより、すでに振動している弦に CC11 が不連続な出力ゲインを導入することを防ぎます。初期表情はノートオン直前に直接設定し、後続の変更では波形を保ちます。ライブ応答の自動化は確保なしに係数を再計算します。コンストラクターの既定値は約57 ms、適合リリースは上限の1.5秒です。そのためリリース再構成は現在のモデル／検索区間に制限されます。

Python の色付け代理は Rust と同じ順序（先にボディ、その後に移動出力表情）を守ります。因果フィルターと動的ゲインは可換ではありません。代理はドライ出力の正の表情を元に戻してからフィルターし、同じ平滑化表情を戻します。弓に既に与えた表情は元に戻しません。探索前に実レンダラーとのPCM一致を検査します。最終レポートは完全な Rust ビルドを直接比較します。

ギターには長い減衰の較正を適用します。ピアノは現在の較正を保ちます。学習 RMS の中央値でモデル正規化スカラーを通じて工場レベルの規約を保ちます。明示したプロジェクトパラメータは明示のまま、作曲ロールは工場パラメータを上書きできます。

<a id="measured-results"></a>
## 測定結果

以下は `797a19b` から開始した完全に固定した Rust ビルドのホールドアウト mel 損失平均です。小さいほど良い値です。両ビルドには同じ録音ピッチと表情ガイドを渡します。Iowa の3秒／6秒ビューは重複する抜粋で、独立した演奏ではありません。

| 検証コホート | 24 kHz変更前 | 24 kHz変更後 | 48 kHz変更前 | 48 kHz変更後 |
| --- | ---: | ---: | ---: | ---: |
| Piano, long Iowa | 0.145584 | 0.145584 | 0.147241 | 0.147241 |
| Guitar, long Iowa | 0.127096 | 0.122735 | 0.127138 | 0.122772 |
| Violin, long Iowa | 0.091258 | 0.090017 | 0.094414 | 0.091412 |
| Violin, TU glissandi | 0.044139 | 0.043970 | 0.046787 | 0.046636 |
| Guitar, original short Iowa | 0.148237 | 0.147670 | 0.148302 | 0.147732 |
| Violin, original short Iowa | 0.084689 | 0.082639 | 0.088016 | 0.086301 |

48 kHzでは、長いギターの損失が3.4%、長いバイオリンが3.2%、短いバイオリンが1.9%低下します。グリッサンドの音色／制御較正の改善は約0.3%と小さい値です。固定表情アブレーションでは、従来バイオリンの TU 損失は固定弓で0.083713、新ビルドに録音表情を与えると0.046636（44.3%低下）です。これは条件付き演奏の追従とモデル変更を合わせた値で、同等の弓動作を自動生成できることは示しません。

ノートごとのピッチ、エンベロープ、カバレッジ、位相診断は [24 kHz](../tools/eval/references/physical-trajectory-validation-24k.json)、[48 kHz](../tools/eval/references/physical-trajectory-validation-48k.json)、[constant-expression](../tools/eval/references/physical-trajectory-constant-bow-48k.json) に保存します。学習専用の [guitar](../tools/eval/references/physical-trajectory-guitar-calibration.json) と [violin](../tools/eval/references/physical-trajectory-violin-calibration.json) 較正レポートには、Rust に丸める前の適合値を残します。

記号プリセット測定は変更前後でバイト一致します（SHA-256 `68a1be1d19d62d88d822022a67f7dd549e3875dd8f5cf6eb66800f18d6006496`）。9つの固定ソングプリセットでは学習 CE／PQ 平均が5.887／7.734から5.896／7.765へ、PCは4.473から4.443へ変化します。これは曲レベルの混在した診断値で、最適化目標ではありません。この開発機では48 kHz、256フレームのコールバックで24声を保持した場合、平均／p99コールバック時間はギター0.081／0.104 ms、バイオリン0.075／0.103 msです。ローカル計測であり、最悪時のリアルタイム保証ではありません。

<a id="reproduction"></a>
## 再現

レポートに記録した依存関係バージョンで `uv` スクリプト環境を使います。学習環境やGPUは不要です。Windowsではワーカーのパスに `.exe` を付けます。

```sh
uv run tools/eval/fetch_physical_reference.py target/copy-synthesis/iowa
uv run tools/eval/prepare_physical_reference.py target/copy-synthesis/iowa \
  --lock tools/eval/references/iowa-notes.json
uv run tools/eval/fetch_violin_transitions.py target/long-copy/tu-note \
  --lock tools/eval/references/tu-note-captures.json
uv run tools/eval/prepare_physical_trajectories.py target/copy-synthesis/iowa \
  target/long-copy/tu-note target/long-copy/references \
  --lock tools/eval/references/physical-trajectories.json
cargo build --release -p auris-synth --example physical_fit_render
# Freeze a worker before changing the audio implementation or factory calibration.
uv run tools/eval/fit_physical_trajectories.py target/long-copy/references \
  target/copy-synthesis/iowa target/long-copy/before-worker target/long-copy/fit.json \
  --fit --model violin --profiles tools/eval/references/physical-long-initial-profiles.json
uv run tools/eval/fit_physical_trajectories.py target/long-copy/references \
  target/copy-synthesis/iowa target/release/examples/physical_fit_render \
  target/long-copy/validation.json --before target/long-copy/before-worker --rate 48000
uv run tools/eval/fit_physical_trajectories.py target/long-copy/references \
  target/copy-synthesis/iowa target/release/examples/physical_fit_render \
  target/long-copy/constant-bow.json --before target/long-copy/before-worker \
  --model violin --constant-expression --rate 48000
uv run tools/eval/audition_physical_trajectories.py target/long-copy/references \
  target/long-copy/before-worker target/release/examples/physical_fit_render \
  target/long-copy/audition --constant-before
```

記録されたバイオリン適合を再現する場合、開始ワーカーには弓応答平滑化が含まれますが、以前の工場パラメータ／ボディゲイン（`797a19b`）を使います。グローバル反復は6回、既定のローカルスケジュール（境界幅の割合0.05、0.02、0.008）、シード20261001、差分進化の個体数倍率4を指定します。分離したソースチェックアウトと独立した Cargo ターゲットディレクトリで基準ビルドを安定させます。ワーカーは楽器が表情平滑化に対応するかを報告するため、例を以前のソースへコピーしても代理の動作を保てます。ギターも元の `797a19b` ワーカーに対して同じ6回のグローバル探索と固定ローカルスケジュールを使います。

コンパクトな参照ロックには WAV ハッシュと正規制御配列ハッシュを保存します。完全な制御点はローカルで再生成します。`--start-from` は、レンダラーと完全な参照ハッシュが一致する場合にだけ学習専用のローカル適合を再開します。レポートには既定値、サンプルレート、シード、境界、バージョン、ワーカーハッシュ、ノートごとの誤差を含めます。24 kHzと48 kHzの検査はいずれも固定済みの実バイナリを使います。

```sh
uv run --with numpy --with scipy --with soundfile --with pytest pytest \
  tools/eval/test_physical_trajectory.py tools/eval/test_physical_mel.py \
  tools/eval/test_fit_physical_body.py tools/eval/test_physical_ab.py
uv run --with ruff ruff check tools/eval/*physical*.py tools/eval/fetch_violin_transitions.py
cargo test --workspace
cargo test -p auris-synth --example physical_fit_render
cargo clippy --workspace --all-targets
```

数値回帰では、複数のレート／ダイナミクスでの12秒バイオリン持続音、表情の自動化への応答、出力の連続性、コールバックの確保、ピッチ追跡精度、長いクレッシェンド、分数ノートオフ、後期サステイン損失、64／257／1024フレームブロックにわたるサンプル単位の制御イベントを確認します。
