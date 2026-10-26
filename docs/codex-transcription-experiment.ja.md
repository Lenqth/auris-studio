<a id="codex-led-transcription-experiment"></a>
# Codex 主導の転写実験

[English](codex-transcription-experiment.md)

2026-10-21 の実験では、インストール済み Codex CLI 0.163.0、ユーザー設定の `gpt-6.1-sol`、高い reasoning effort、ChatGPT サインインを使いました。Codex は Basic Pitch を使わず、独自の信号分析プログラムを書いて実行することで、利用可能な音符ドラフトを生成しました。総合精度は現在の認識器を下回りました。

[測定記録](../tools/eval/references/codex-transcription-2026-10-21.json)には、プロンプト、モデルメタデータ、コマンドトレース、元の音符、固定した生成プログラム、音声／参照ハッシュ、再生チェック、指標、配信承認を保持しています。これは開発用ツールと証拠を追加するもので、製品の転写は変更しません。

<a id="direct-audio-versus-a-tool-using-agent"></a>
## 直接音声とツールを使うエージェント

インストール済みの app-server スキーマには `audio` と `localAudio` の入力型があります。無音、分離した6音、3音コード2つを使い、ツールを持たない新しい Codex スレッド3つに実際の `localAudio` 入力を送りました。3つの応答も空配列を返し、添付音声にアクセスできないことを明示しました。モデルカタログは選択したモデルについてテキストと画像入力を示し、7つのエントリのどれにも音声モダリティはありませんでした。これは音楽認識精度の測定ではなく、入力能力の失敗です。[公式 app-server ドキュメント](https://learn.chatgpt.com/docs/app-server)にモデルカタログの `inputModalities` フィールドの説明があります。

2つ目の経路では、Codex に、15個の WAV スナップショット、匿名ファイル名、長さだけを含む別のワークスペースを渡しました。そこには前の実験と同じ作曲家録音12個と、3つのコントロールがありました。スコア、身元、BPM、コード、MIDI、Basic Pitch 推定値は伏せました。Codex は独自の方法を選び、Python を書き、サンプル測定とプロットを検査してコードを改善しました。提供した uv 環境には numpy、scipy、soundfile、matplotlib がありました。事前学習済み転写モデル、外部認識サービス、インターネット調査、委任は除外しました。Codex には正確性のフィードバックを返していません。

最初の CLI 試行は never-approve 設定でネイティブコマンドを実行できず、ファイルを処理しませんでした。測定対象の試行では、文書化された `--approve-for-me` ワークスペースモードを使いました。ユーザー設定、フック、プラグイン、アプリ、継承した MCP 接続は除外し、設定済みのモデル／effort は明示的に渡しました。コマンドトレースには、元のケースの身元やアノテーションへのアクセスはありません。これらは実験上の制御であり、敵対的なファイルシステム分離を保証するものではありません。

<a id="what-codex-built"></a>
## Codex が作ったもの

完了したエージェントは535.75秒かけ、15ファイルすべてで846音符イベントを書きました。再生を聴くのではなく、サンプルとプロットを分析したと明示的に報告しました。最終プログラムは次を使います。

* ステレオ平均、22,050 Hzへのリサンプル、4,096サンプルの Hann 窓、220サンプルのホップ、8,192ポイント FFT による中心化フーリエ分析。
* 広帯域フロアの除去と周波数ホワイトニング。
* MIDI 21〜108を対象とする、ピアノキーごとの8つの解析的な調波プロファイル、その後の疎な非負スペクトルフィットと基音ピークの支持。
* アクティビティしきい値、反復アタック検出、境界の改良、同期した上位倍音を抑えるヒューリスティック。

これらのプロファイルは数学的テンプレートであり、ダウンロードした重みや事前学習済み重みではありません。完全なプログラムは記録に保存されています。最終 SHA-256 は `1ad425dd3758b046d23c3e3a11bee0ce2815402fda64fc69f77a9cdd873c08b5` です。公開前に、15個すべての元ドラフトを固定プログラムで正確に再現しました。参照スコアリング後にモデルやプログラムを編集していません。

<a id="accuracy-on-the-same-inputs"></a>
## 同じ入力での精度

コーパスは、[以前の評価](../tools/eval/references/piano-transcription-2026-10-21.json)と同じ保留合成録音12個で、3プリセット、2シード、ピアノ版／混合版です。一致するユニゾンを統合すると、参照イベントは924個です。独立スコアラーは、固定したプロジェクトのアノテーションと録音ハッシュを検証します。マッチングには正確な MIDI 音高と50 ms以内のオンセット、最大一対一マッチングを要求します。F1 はイベントの precision と recall を組み合わせたもので、アレンジ品質は測りません。

| 入力コホート | Codex 生成方式 | 現在の Basic Pitch |
| --- | ---: | ---: |
| 録音12個すべて、onset F1 | 54.95% | 67.48% |
| ピアノ録音6個 | 60.37% | 71.19% |
| 混合録音6個 | 49.11% | 62.71% |
| 一致イベント | 483 | 637 |
| 余分なイベント (FP) | 351 | 327 |
| 欠落イベント (FN) | 441 | 287 |

分離音とコードのコントロールは、リリース検査を含む12イベントすべてに一致し、無音コントロールは正しく0イベントを生成しました。空のケースの数値 F1 は直感的には未定義で、スコアラーは0として保存します。無音の受け入れは別に評価し、転写の失敗とは説明しません。

結果にはばらつきがあります。Codex 方式は city-pop のピアノケース2つで5.4ポイントと1.8ポイント改善しましたが、残り10録音では悪化しました。100 ms許容では総合 onset F1 は58.36%、onset-plus-offset F1 は42.32%です。これはコーディングツールを持つエージェントの結果であり、以前の一発の Gemma 音声リクエストと、それ以外を管理したモデル品質比較ではありません。

方式を固定した後、ホストは独立した Mutopia SATB *Ode to Joy* 録音に変更なしで適用しました。追加の Codex 推論や調整はありません。これは管理された既存スコアの合成であり、音響演奏ではありません。

| 既存スコア入力 | Codex onset F1 | Basic Pitch onset F1 | Codex FP / FN |
| --- | ---: | ---: | ---: |
| Piano | 78.12% | 95.97% | 21 / 72 |
| Reeds and strings | 59.40% | 64.74% | 20 / 129 |

Reeds/strings ソースでは、onset-plus-offset F1 が45.26%から58.86%に改善しましたが、オンセット再現率は低下しました。完全な記録には、この改善と欠落音符が残っています。調波抑制で実際のオクターブ／倍音ボイシングを削除することがあり、静かな音符、サステイン中のアタック、広帯域干渉は引き続き難しい課題です。どちらの方式も両手演奏のしやすさや主観的な聴取品質は採点していません。

<a id="application-boundary-and-an-editable-cover"></a>
## 適用範囲と編集可能なカバー

この経路を採用する場合、2つの統合上の制限があります。

1. 現在の Auris Codex 添付経路は、範囲を制限したテンポ／コード測定と不透明な添付 ID を渡します。元のサンプルを一般的な分析入力として公開するものではありません。このプロトタイプは代わりに、自分のワークスペースで WAV スナップショットを受け取りました。
2. `PianoTranscription::validate` は、付属 Basic Pitch モデルの正確なハッシュを要求します。そのため通常のピアノカバーセッションコマンドは、この別認識器のドラフトを拒否します。偽のモデルハッシュに置き換えず、拒否を記録しました。

配信検証では、ホストが固定した Reeds/strings ドラフトを MIDI に変換し、実際の保存済みプロジェクトの `import_midi` と `render` ツールを呼び出しました。これはホストの承認チェックであり、Codex のモデルツール呼び出しではありません。129音符すべてが物理ピアノプロジェクトへのインポート後も残り、最大のタイミング変化は MIDI 丸めによる0.260 msでした。試聴用のベロシティは一律83で、ダイナミクスから推定したものではありません。38.39秒の WAV はフルスケールサンプルなしでエンコードファイル検査に合格しました。記録にはそのハッシュが保存されています。

この実験は、ツール支援による Codex 転写を動くプロトタイプとして支持しますが、このアルゴリズムで全録音にわたり現在の認識器を置き換えることは支持しません。エージェントには、信頼できる音声分析の実装と、ソースへの適切なアクセス、バックエンドに依存しない音符ドラフトのインポート境界がなお必要です。

上記の API 制限は実験時点の版を説明しています。その後の [転写 API](transcription-api.ja.md) は、元の添付の書き出しと認識器に依存しないドラフトインポートを提供しますが、これらの認識スコアは変えません。

<a id="reproduce"></a>
## 再現

ハーネスとスコアラーには numpy/scipy を含む uv 環境を使います。公式 Codex CLI からサインインしてください。ハーネスはサブスクリプションの認証情報ファイルを読みません。設定済みのモデルと reasoning 設定を使います。出力パスは新しく用意する必要があります。

```powershell
$codexExe = (Get-ChildItem "$env:APPDATA/npm/node_modules/@openai/codex" -Recurse -Filter codex.exe | Select-Object -First 1 -ExpandProperty FullName)
uv run --with numpy --with scipy tools/eval/codex_transcription.py target/agent-direct-transcription-controls/manifest.json target/codex-direct-new --codex $codexExe --split control
uv run --with numpy --with scipy tools/eval/codex_transcription.py target/piano-transcription/corpus-v1/manifest.json target/codex-tools-new --codex $codexExe --mode tools --controls target/agent-direct-transcription-controls/manifest.json
uv run --with numpy --with scipy --with pytest pytest tools/eval/test_codex_transcription.py tools/eval/test_direct_audio_transcription.py tools/eval/test_piano_transcription.py
uv run --with ruff ruff check tools/eval/codex_transcription.py tools/eval/test_codex_transcription.py
```

ツール使用モードでは別の uv 環境を準備し、記載された科学計算パッケージをインストールすることがあります。サインイン済み Codex アカウントの利用量を消費します。新しいエージェント実行では別のプログラムが生成される可能性があるため、記録された固定ソースと出力を使ってこの過去の結果を監査してください。再生時間には診断プロットの生成も含まれ、純粋な推論ベンチマークとは比較できません。
