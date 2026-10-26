<a id="music-analysis-compute-and-model-deployment-assessment"></a>
# 音楽分析：計算資源とモデル配置の評価

[English](music-analysis-compute.md)

評価日：2026-09-07。実装済みの[音楽分析ワークフロー](music-analysis.ja.md)に続く計算資源評価です。

クラウド資源は準備しておらず、モデルも学習しておらず、音声もアップロードしていません。YAMNetは準備してCPU上でローカル実行しました。統合、ハッシュ、測定値、精度の限界は[モデルガイド](music-analysis-models.ja.md)にあります。

<a id="amd-only-hardware-does-not-make-every-neural-method-a-cloud-workload"></a>
## AMD専用ハードウェアでも、すべてのニューラル手法がクラウド処理になるわけではない

記号コード認識、オンセット／テンポ分析、クロマベースの音声コード、単旋律のピッチ追跡は、まずCPU実装を使うべきです。Basic Pitchと小型ビート／タグ付けモデルは事前学習済みCPU推論の候補です。学習要件から推論要件を決めてはいけません。Aurisでの速度はまだ測定していません。

測定環境はAMD Ryzen 7 9800X3D CPUとAMD GPUを備えたWindowsです。試行はCPUだけを使いました。GPU互換性とピークRAM／VRAM要件は未測定であり、GPUベンダーやチェックポイントサイズから推測しないでください。

現在の歌声バックエンドはWindowsでONNX Runtime DirectMLを有効にしています。DirectMLはAMDを含むDirectX 12ハードウェアをサポートするため、互換ONNXモデルの後続アクセラレーター候補です。`auris-gpu` のwgpu DX12ビルド問題は別の依存経路であり、それだけでDirectMLを除外する理由にはなりません。固定ランタイムで実際のモデルの演算子、精度、CPUフォールバックを試してください。現在のプロバイダー文書は、このリポジトリが使う古いランタイムでのサポートを保証しません。[ONNX Runtime文書](https://onnxruntime.ai/docs/execution-providers/DirectML-ExecutionProvider.html)を参照してください。

ROCmのサポートはGPU、OS、ソフトウェアのバージョンに依存します。ネイティブCUDA研究リポジトリをAMDへ自動移植できると計画せず、対象デバイスが分かってからAMDの互換性表を確認してください。[AMD互換性文書](https://rocm.docs.amd.com/en/latest/compatibility/compatibility-matrix.html)を参照してください。

<a id="separate-small-inference-trials-from-expensive-research-experiments"></a>
## 小規模推論と高価な研究実験を分ける

| 候補 | 配置と目的 | 配置の証拠／未解決の条件 |
| --- | --- | --- |
| Basic Pitch | 分離楽器のポリフォニーを試すローカルCPU試行。整合性確認後はDirectMLも任意で使用。 | 公式ONNX成果物、リポジトリはApache-2.0。取得成果物、前後処理の一致、`ort`互換性を確認する。[Source](https://github.com/spotify/basic-pitch) |
| Beat this! small | DSP精度が不十分な場合のビート／ダウンビート用ローカルCPU試行。 | 約8.1 MBの小型チェックポイント、コードと重みはMIT。サイズはピークRAMを示さない。Rust／ONNX出力は別途検証が必要。[Source](https://github.com/CPJKU/beat_this) |
| YAMNet | 任意のCPU楽器存在タグ付けを実装済み。 | Googleの重みと構成はApache-2.0、出力と参照の一致を確認済み。ローカル試料では閾値0.2で楽器カバー率が低かった。[Weight terms](https://groups.google.com/g/audioset-users/c/Ly4guQnlv_o) |
| MT3 | 複数楽器ノートの外部推論参照。 | T5X研究スタック、リポジトリはApache-2.0。計算資源を見積もる前にチェックポイントと実行環境を固定する。[Source](https://github.com/magenta/mt3) |
| YourMT3+ | 商用ワークフローの優先候補。RunPod提案前にCPUで試行。 | 実際のHFソースはApache-2.0ヘッダーを持ち、チェックポイントカードも重みをApache-2.0と明記する。正確なHF成果物を固定する。今回の取得はHTTP 429/500で失敗し、チェックポイントは未実行。[Code](https://huggingface.co/spaces/mimbres/YourMT3/blob/5e66c1ea173a8186e0d20432b841d3180cc015b5/amt/src/model/ymt3.py)、[weights](https://huggingface.co/mimbres/YourMT3/blob/main/README.md) |
| Demucs＋転写 | 分離で採用ノート精度が上がるかを調べる任意の外部診断。 | MITリポジトリ。4つの既定ステムは任意の個別パートを識別しない。直接転写と分離＋転写を、精度・実行時間・分離生成物で比較する。[Source](https://github.com/facebookresearch/demucs) |
| 転写／タグ付けモデルの学習・微調整 | エラー分析で具体的なデータ／モデルの不足が示された後の、別個のRunPod提案。 | データセット権利、学習手順、チェックポイント、測定済みメモリ／スループットが必要。根拠のある固定GPU時間見積もりはまだない。 |

Essentiaとmadmomは有用な研究参照ですが、成果物単位のライセンス確認が必要です。Essentiaの公開MTGモデルはCC BY-NC-SA 4.0（独自ライセンスも提供）で、madmomはコードとモデル／データの条件を分けています。[Essentia models](https://essentia.upf.edu/models.html)、[madmom](https://github.com/CPJKU/madmom)。

上記の扱いはこのプロジェクトの技術判断です。列挙した外部モデルすべてが数学的にNVIDIAハードウェアを必要とする、または最低メモリ量が測定済みだという主張ではありません。任意モデルがクラウドへのアップロードを暗黙に開始してはいけません。

<a id="a-bounded-runpod-pilot-would-answer-whether-multi-instrument-transcription-is-worth-integrating"></a>
## 限定したRunPod試行で、複数楽器転写を統合する価値を判断できる

初期案は、VRAM 24 GBのLinux NVIDIA GPU 1台、ホストRAM 32〜64 GB、固定チェックポイントと少量の抜粋用に約100 GBの作業領域です。バッチサイズ1と各モデルの対応チャンクから始めます。これは暫定的な実験割り当てであり、ベンダーの最低要件や適合保証ではありません。Slakhのような完全なデータセットには追加領域が必要で、試行では準備した小さなサブセットを使います。

最初のスモークテストには、課金GPU時間を最大2時間だけ確保します。設定とダウンロード時間を含め、時間を使い切ったら黙って延長せず停止します。実際のGPU、ストレージ、ネットワーク料金は準備時に取得してください。モデルが収まらなければ対応バッチ／チャンクを下げて再測定し、失敗と具体的な48 GB案を報告してから資源を増やします。パラメーター数だけでは活性値、アテンションキャッシュ、精度、フレームワークのオーバーヘッドを説明できません。

試行手順は次のとおりです。

1. コードと重みの正確なライセンスを確認し、コミット／チェックポイントのハッシュとコンテナイメージを固定し、許可済みのローカル評価抜粋と参照を準備します。音声学習データや認証情報をコンテナイメージに入れません。
2. 選んだ各モデルで30秒と3分の抜粋を1つずつ実行し、コールド／ウォーム実行時間、RAM／VRAM、失敗、楽器を考慮したノートF1を記録します。単純な混合と密な混合を含めます。
3. スモークテストが範囲内なら、ステージ3のローカル基準に対して未使用抜粋10〜20個を比較します。マスキングが診断された失敗であるサブセットに限り、直接転写と分離＋転写を比べます。
4. MIDI、ソース時刻イベントJSON、スコア、設定、ログ、ハッシュを返します。実際の課金時間とストレージ使用量を記録して計算資源を停止します。音声を診断ログに残さず、合意した保持方針に従ってアップロード済みコピーを削除します。
5. 訂正の手間、楽器を考慮した精度、レイテンシー、費用、配置可能性を合わせて統合を判断します。ライセンス済みCPU／AMD推論が製品予算を満たすならローカル配置を優先し、そうでなければ明示的にオプトインする外部ワーカー契約を提案します。

後続の学習リクエストでは、観測した失敗、利用可能なラベル付き例、基準／目標指標、データ分割、期待効果、短い試行で測定した毎秒ステップ数／ピークメモリを示してください。その後で初めて学習時間を見積もり、大きなGPUや複数GPUを選びます。今すぐ学習マシンを借りても、タスク範囲、認識精度、使えるモデル成果物という目前の疑問は解決しません。
