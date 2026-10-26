<a id="grand-piano-calibration"></a>
# グランドピアノの較正

[English](grand-piano-calibration.md)

2026-10-26 の[鍵盤全体の整音](grand-piano-register.ja.md)で、ここに書いたモデルと工場値は置き換わりました。そのホールドアウト検証には、ここの音と指標を使っています。

ネイティブの Physical Piano 工場音色は、Evan Mazunik が演奏した University of Iowa の [Steinway model B 録音](https://theremin.music.uiowa.edu/MISpiano.html) に対して較正しています。2026-10-05の改訂では、フェルトの硬さ、打弦位置、弦の剛性、損失、12帯域の響板色付けを調整しました。オシレーターは有限接触の結合ユニゾンモデルのままで、コールバックの確保やI/Oは追加していません。

<a id="reference-and-selection"></a>
## 参照と選択

基準は `7e1bf881f4c188feceffb55384dfbb1015e2d537` です。元の AIFF ハッシュ、アタックオフセット、固定チューニングオフセット、分割は `tools/eval/references/iowa-captures.json` と `iowa-notes.json` にあります。18個の原ノートはそれぞれ、ワープしていない1秒、3秒、6秒のカットを提供します。ピッチは MIDI 48、52、55、60、64、67、強さは pp／mf／ff（序数ベロシティ0.35／0.65／0.95）です。

最適化に入れるのは、ピッチ48／55／64の mf／ff 6ノートだけで、**3秒**窓を使います。残り12ノートはすべてホールドアウトし、pp のノートも含みます。1秒／6秒窓は期間の検査であり、検索目的ではありません。範囲を制限した2段階検索と正則化色付け適合は `fit_physical_mel.py` を再利用し、各段階で差分進化を8世代、パラメータごとの個体数を5、シードを20261005／20261006とします。

目的関数は固定の [temporal mel distance](physical-copy-synthesis.ja.md) です。全音符 RMS 正規化、2つの STFT 解像度、アタックとサステインを同じ重みで測る L1を使います。測定するのはスペクトルと時間の類似度であり、知覚されるピアノ品質ではありません。Python の色付け代理は基準 Rust ワーカーと最大サンプル誤差0.0000019以内で一致します。

<a id="compiled-results"></a>
## コンパイル結果

最終測定では、保存した基準とコンパイル済み工場制御の両方をレンダリングします。48 kHzから固定24 kHzの測定空間へリサンプリングした結果は次のとおりです。

| ホールドアウト期間 | 変更前 | 変更後 | 低下率 | 改善ノート |
| --- | ---: | ---: | ---: | ---: |
| 1秒 | 0.171541 | 0.162723 | 5.1% | 12/12 |
| 3秒 | 0.218340 | 0.205345 | 6.0% | 12/12 |
| 6秒 | 0.259861 | 0.243660 | 6.2% | 12/12 |

24 kHzでは対応する低下率は4.7%、5.5%、5.7%で、各窓とも12/12です。学習誤差は両レート、3期間すべてで改善します。ノートごとの全値、実行ファイルハッシュ、工場パラメータ、パッケージバージョン、参照ハッシュは [grand-piano-validation.json](../tools/eval/references/grand-piano-validation.json) にあります。[training report](../tools/eval/references/grand-piano-calibration.json) には検索範囲、初期色付けゲイン、両段階、選択した制御を保存しています。

出力倍率は、基準に対する3秒学習ノート RMS の中央値を保ちます。倍率は0.164845から0.186700へ変わりますが、公開レベルの既定値は-12 dBのままです。明示的に保存されたプラグインパラメータは、新しい工場既定値より優先されます。響板色付けは進化する楽器実装の一部です。

これらの検査は、1台の録音グランドピアノ、C3〜G4、保持ノート、3つのダイナミクスラベルを対象とします。鍵盤全体、別のグランドピアノ、ペダル表情への忠実度は示しません。決定的な試聴には事前選択した mf C4 検証ノートを使い、**録音／変更前／変更後** の順に、全音符 RMS を一致させ、共通のピーク余裕と5 msの境界フェードを適用します。採点にはフェード前のPCMを使います。

<a id="system-checks"></a>
## システム検査

ワークスペーステスト、197個の楽器テスト、ワークスペース clippy、21個の Python 評価テスト、ruff はすべて通過します。記号ルーラーの出力は変更前後でバイト単位に一致します。

9つの既定シード作曲プリセットを、別々に固定した基準CLIと更新CLIでレンダリングしました。学習評価器は同じ3つの10秒窓、固定モデル、固定プロンプトを使い、レベル正規化はしません。TuneJury 平均報酬は0.128734→0.159262、MuQ-MuLan 正のコサインは0.241841→0.241711、コントラスト余白は0.286418→0.285843に変わりました。すべての抜粋位置は一致します。4つのプリセットは WAV ハッシュとスコアが同一です。city-pop、jazz-trio、pop-band の報酬は0.0769／0.1197／0.0788改善し、rock は0.0028改善、synthwave は0.0034低下しました。これらは単一シードの曲診断による補助的な証拠であり、普遍的な品質を主張するものではありません。

完全な [変更前](../tools/eval/references/grand-piano-music-before.json) と [変更後](../tools/eval/references/grand-piano-music-after.json) のレポートには、窓ごとのスコア、音声ハッシュ、パッケージバージョン、モデル成果物ハッシュを残しています。[system summary](../tools/eval/references/grand-piano-system.json) にはCLIハッシュと一致する記号ハッシュも記録します。評価は4つの推論スレッドを使うCPUで実行しました。

<a id="reproduction"></a>
## 再現

まず基準コミットで `physical_fit_render` をビルドして保存します。以下では実行ファイルを `target/piano-grand/before-renderer.exe` とし、macOSでは `.exe` を省きます。更新済みソースツリーからこの基準に対して適合を繰り返すときは、保存した初期色付けプロファイルを明示します。

```powershell
uv run tools/eval/grand_piano.py target/piano-grand/references prepare
uv run tools/eval/grand_piano.py target/piano-grand/references fit `
  target/piano-grand/before-renderer.exe target/piano-grand/new-fit.json `
  --initial-profile tools/eval/references/grand-piano-calibration.json
cargo build --release -p auris-synth --example physical_fit_render
uv run tools/eval/grand_piano.py target/piano-grand/references evaluate `
  target/piano-grand/before-renderer.exe target/release/examples/physical_fit_render.exe `
  target/piano-grand/new-validation.json
uv run --with pytest --with numpy --with scipy --with soundfile `
  pytest tools/eval/test_grand_piano.py tools/eval/test_physical_mel.py
uv run --with ruff ruff check tools/eval/grand_piano.py tools/eval/test_grand_piano.py
```

評価は JSON レポートの隣に `.wav` 拡張子で試聴ファイルを書き込みます。取得した原音と生成音声は `target/` の下に残り、録音や Python ツールがアプリケーションに同梱されることはありません。
