<a id="optional-local-music-models"></a>
# 任意で使えるローカル音楽モデル

[English](music-analysis-models.md)

通常のコード、テンポ、単旋律のコマンドは、引き続きCPUで信号処理を行う。学習済み推論を追加するコマンドは2つある。YAMNetによる楽器存在タグ付けと、MuScriptor Smallによる複数楽器のノート下書きである。どちらもCPUで動作し、CUDA、AMD GPUバックエンド、学習、RunPod、音声アップロードを必要としない。モデルファイルはAurisの外部にあり、解析コマンドがダウンロードすることはない。

<a id="instrument-presence-with-yamnet"></a>
## YAMNetで楽器の存在を推定する

別の `uv` 環境で、波形からスコアを出すONNXエクスポートを一度だけ準備する。

```sh
uv run tools/music-models/export_yamnet.py --output models/yamnet.onnx
auris analyze-instruments recording.wav --model models/yamnet.onnx --threshold 0.2
```

スクリプトはハッシュを検証したGoogleのソースと重みをダウンロードし、前処理をONNXに含め、無音、ノイズ、正弦波、インパルスでTensorFlowとの一致を検証する。既存の出力を置き換えることはない。準備にはTensorFlowが必要だが、推論に必要なのはエクスポート済みONNXファイルだけである。固定された依存関係はスクリプトを参照し、[YAMNetの通知](../crates/auris-analysis/NOTICE-YAMNET.md)も確認する。Googleのメンテナーは、[公式ユーザーグループ](https://groups.google.com/g/audioset-users/c/Ly4guQnlv_o)でアーキテクチャと重みの両方がApache 2.0であることを確認している。再配布時は該当するライセンスと通知を保持する。

デスクトップでは、オーディオをインポートし、クリップメニューから **楽器を推定…** を選び、準備したONNXファイルを指定する。**解析下書きを表示** にはクリップの平均値と、重なり合う窓ごとの仮説が表示される。このコマンドはトラックを変更せず、ノートに楽器を割り当てない。MCP／エージェントからは `audio`、`model`、任意の `threshold` を持つ `analyze_instruments` を使う。

YAMNetはAudioSetの一般的なイベント分類器であり、専門的な楽器分離器ではない。スコアは独立した未較正のシグモイド値である。複数のラベルが返る場合があり、しきい値を超えるラベルがない場合は不在ではなく **unknown** になる。JSONには、楽器以外も含む強いイベントラベル上位5件も保持する。英語ラベルはモデルの語彙に従う。クリップ平均では短時間だけ鳴った楽器が隠れることがあるため、窓ごとの結果も確認する。しきい値を下げると弱い仮説が現れる一方、誤検出が増えることがある。

CPUプロバイダーは、演算内スレッド2本、16 kHzモノラル、975 msの窓、480 msのホップを使う。最後の窓はパディングする。制限は30分、入力8チャンネル、ONNXファイル32 MiBである。チャンネル平均により、逆位相のステレオが打ち消されることがある。デコードとリサンプリングは解析上限を確認する前に入力を確保するため、これらはストリーミング時のメモリ制限ではない。キャンセルは推論窓の間で確認する。

<a id="muscriptor-is-a-separate-noncommercial-option"></a>
## MuScriptorは別途提供される非商用オプション

MuScriptorのコードはMITだが、**重みはCC BY-NC 4.0**である。デスクトップは呼び出しごとに警告を表示し、ユーザーが非商用利用を明示的に選ぶまで推論を開始しない。キャンセルすると警告を閉じ、モデルを読み込まない。CLIとモデル向けツールでは、呼び出しごとに明示的な確認が必要である。確認は商用ライセンスではないため、権利者から別途許可を得ずに商用利用してはならない。[モデルの追加条件](https://huggingface.co/MuScriptor/muscriptor-small)と[CC BY-NC 4.0](https://creativecommons.org/licenses/by-nc/4.0/legalcode.en)を確認する。

この任意の統合が利用できる場合でも、Auris自身のApache 2.0ライセンスは変わらない。アプリケーションは非商用の重みを同梱しない。生成されたすべてのMIDIファイルのライセンスが自動的に変わるという宣言でもない。モデルの利用と原曲・出力に対する権利は、該当する条件を満たす必要がある。商用ワークフローでは通常のコマンドとYAMNetを使い、MuScriptorを呼び出さずに済む。

<a id="obtain-and-convert-the-model-once"></a>
### モデルを一度取得して変換する

ユーザーは条件に同意した上で、元チェックポイントをHugging Faceから直接ダウンロードする。元の重みも変換済みONNX重みもAurisには再配布しない。変換ツールが受け付けるのはローカルファイルだけで、チェックポイントを自分でダウンロードすることはない。

モデルページで利用条件に同意し、ローカルで `hf auth login` を実行して認証する。トークンをプロジェクトやチャットに置かない。PowerShellで検証済みリビジョンをダウンロードする。

```powershell
hf download MuScriptor/muscriptor-small model.safetensors config.json README.md `
  --revision 8c127f603b807520fa465c838e9bfee8a91ada4e --local-dir models/muscriptor-small
```

`uv` でCPU変換用の別環境を作る（Pythonは変換時だけ必要）。

```powershell
uv venv --python 3.12 models/muscriptor-convert
uv pip install --python models/muscriptor-convert/Scripts/python.exe `
  muscriptor==0.3.0 torch==2.5.1 torchaudio==2.5.1 `
  onnx==1.17.0 onnxruntime==1.20.1 "numpy>=1.26,<3" --torch-backend cpu
models/muscriptor-convert/Scripts/python.exe tools/music-models/export_muscriptor.py `
  --checkpoint models/muscriptor-small/model.safetensors `
  --output-directory models/muscriptor-onnx --acknowledge-noncommercial
```

元の `config.json` はチェックポイントの隣に置く。変換ツールは検証済みのSmallアーキテクチャだけを受け付け、既存の出力ディレクトリを拒否する。生成物は次のとおりである。

* `audio.onnx`: 条件付けプレフィックスに使う16 kHzモノラル5秒の音声。STFT、振幅メル特徴、元のnull楽器／データセット条件を含む。
* `decoder.onnx`: トークン埋め込み、位置、Transformer、ロジット、明示的なKVキャッシュ。
* `muscriptor.json`: 形式／バージョン、元チェックポイントのハッシュ、グラフチェックサム、トークン語彙、楽器名、非商用ライセンスメタデータ、数値検証結果。

変換ツールは、無音、ノイズ、正弦波について、音声特徴とキャッシュ付きデコードの連続ステップを公式PyTorch推論と比較する。確認に失敗した場合、パッケージを公開しない。3つの出力ファイルは同じ場所に保つ。Aurisでは **decoder.onnx** を選ぶ。通常の転写はRustとONNX Runtimeで実行し、PythonもPython環境変数も必要としない。変換してもモデルの非商用制限はなくならない。適応実装の[MIT帰属表示](../crates/auris-analysis/NOTICE-MUSCRIPTOR.md)は重みのライセンスとは別である。

<a id="make-and-accept-a-draft"></a>
### 下書きを作成して受け入れる

オーディオをインポートし、コンテキストメニューから **混合音源を転写（非商用）…** を選ぶ。この用途に対する警告を受け入れ、変換したdecoder.onnxを選択して、**解析下書きを表示** を確認する。**下書きを楽器トラックとして追加** は、予測された楽器グループごとにトラックを作り、1回のUndoでまとめて戻せる。元のクリップを保持し、トリム／ストレッチ／繰り返しをテンポマップに対応付け、ソースやドキュメントが編集された後の結果は拒否する。トラック名には `MuScriptor [NC]` を残す。音程トラックは適切なパッチを選ぶまでデフォルトの再生楽器を使い、ドラムは利用可能なら内蔵ドラムキットを使う。グループ名は予測値であり、復元されたパッチではない。

ファイルベースのJSONと新しいMIDIファイルを作るには次を使う。

```powershell
auris transcribe-mixture recording.wav --model models/muscriptor-onnx/decoder.onnx `
  --acknowledge-noncommercial --midi draft.mid
```

読み取り専用のJSONにする場合は `--midi` を省略する。既存プロジェクトにトラックを追加するには、`--project Song.auris --apply [--at-beat 0]` を指定する。既存のMIDI出力先は拒否する。共有 `transcribe_audio` ツールでは `backend:{kind:"muscriptor",model,acknowledge_noncommercial:true}` と、絶対パスの `path` またはライブ添付を選ぶ。呼び出し元は制限を提示し、その呼び出しでtrueを渡す前に明示的な同意を得る必要がある。認識は読み取り専用で、結果の `draft_id` は `import_transcription` で別途受け入れる。楽器ラベル、ソース秒のイベント、モデルハッシュは下書きレポートに保持する。

推論にはONNX RuntimeのCPUプロバイダー、float32、計算スレッド2本、貪欲デコード、5秒チャンクを使う。RustはKVキャッシュを管理し、未完了ノートから次のチャンクのタイプロローグを強制する。これはMuScriptor 0.3.0に従う。Pythonのサブプロセスも一時オーディオファイルもない。転写中にアプリケーションがHugging Faceへ接続することはない。

入力は10分／8チャンネル、各グラフ512 MiB、マニフェスト256 KiB、生成2,000トークン／チャンク、ノート200,000件、合計30分に制限する。ランタイムは使用前にモデルハッシュ、メタデータ、語彙、出力形状を検査する。ハッシュは変更されたファイルを検出するが、パッケージを準備した人物を認証する署名ではない。キャンセルはONNX呼び出しの間で確認する。読み込み、デコード／リサンプリング、実行中のONNX呼び出しは、キャンセルが確認される前に完了する。これらの上限はストリーミングやピークRAMの保証ではない。

これらは編集可能なノート下書きであり、完全なスコアを保証するものではない。モデルはベロシティを復元せず、インポートしたノートには固定値を使う。音高、オンセット、オフセット、楽器グループ、ドラムヒットは誤っていたり欠落したりすることがある。利用前に録音と照合して確認する。

<a id="local-measurements-2026-09-07"></a>
## ローカル測定、2026-09-07

Windows、AMD Ryzen 7 9800X3D、CPU推論のみ、最適化したデバッグRustビルドでの測定である。MuseScore Generalでレンダーした元の12秒ピアノ／アコースティックベース抜粋による単一のスモーク測定で、プロセスとモデルの起動を含む。開発中に取得したもので、安定したレイテンシー目標でも、実録音に対するホールドアウト精度の推定でもない。ピークRAMは測定していない。フィクスチャー生成器は `auris-session` の `render_analysis_fixture` である。

* YAMNet: ピアノ0.84秒、ベース1.02秒、混合0.35秒。3つのクリップ平均のいずれも0.2を超える楽器はなかった。音楽は認識したが特定の楽器は判定せず、これは認識成功ではなくカバレッジの失敗である。4つのフィクスチャーでTensorFlowと比較したエクスポートの最大絶対誤差は0.0000392で、Rustも保存済み参照と一致した。別の `measure_instruments` による3分の合成タイミングプローブは、プロセス起動とソース生成／デコードを除き、モデル読み込み／推論に0.33秒かかった。楽器の正解ラベルはなく、上記CLI測定とは直接比較できない。
* MuScriptor Small ONNX: 12秒で7.33秒（リアルタイム係数0.61）。グラフ読み込みとMIDI出力を含む。3チャンクにわたる58ノートは、音高、オンセット、オフセット、楽器ラベルのすべてが以前のPyTorch結果と完全一致した。変換ツールは3波形とそれぞれ8回のデコードステップを検査し、プレフィックス／ロジットの最大誤差は0.001971/0.001727だった。これはfloat32の数値一致であり、将来、僅差のargmaxがすべて一致する保証ではない。
* 以前のPyTorch測定は12秒で22.46秒（リアルタイム係数1.87）で、推定ノート58件、参照56件、MIDI出力成功だった。楽器を無視したオンセットF1は0.912、オンセットとオフセットのF1は0.807だった。すべてのノートを `acoustic_piano` に割り当て、ベース音高も同じ扱いだった。正しい楽器を要求すると、オンセットF1は0.789、オフセットF1は0.737に下がった。ONNXはこれらのスコアと楽器の混同を保持しており、ランタイムを変えても転写精度は改善しない。
* 別の4秒の純音混合ではノートを返さなかった。合成音色だけでは実録音に対する性能を示せない。

一致判定には、オンセット許容50 ms、音高許容50セント、オフセット許容 `max(50 ms, 20% of reference duration)` を使った。楽器を考慮した集計では、認識されなかったベース参照8ノートをミスとして数える。参照56件／推定58件に対して、オンセット一致は45件、オフセット一致は42件である。ベンチマーク成果物はハッシュとスコアを [learned-analysis-local.json](benchmarks/learned-analysis-local.json) に記録する。

ローカル推論経路は評価と手動修正に使える。しかし、完全な複数楽器復元を主張する根拠にはならない。より広い、許可を得た抜粋セットと、商用利用可能な転写バックエンドが次の評価課題である。[計算評価](music-analysis-compute.ja.md)も参照する。
