<a id="folk-physical-instruments"></a>
# 民族楽器の物理モデル

[English](folk-physical-instruments.md)

Auris Studio には、民族楽器の音色を目的としたサンプル不要の楽器が2つあります。

- `auris.physical.hammered_dulcimer`: 0始まりのGMヒント15（1始まりの一覧ではプログラム16）。
- `auris.physical.tin_whistle`: 0始まりのGMヒント78（1始まりの一覧ではプログラム79）。

どちらも通常の登録済み楽器で、パラメータはプロジェクトに保存されます。SoundFontや別のサンプルライブラリなしで使えます。

<a id="models"></a>
## モデル

ハンマーダルシマーは24ボイス、24個の剛弦部分音、コースごとに2〜3本の軽くデチューンした弦を持ちます。有限の raised-cosine ハンマー接触が励振力を生み、部分音周波数には剛性で正規化した非調和比を使います。受動的なコース混合接合部でブリッジと響板の結合を近似します。制御項目はハンマー硬度、打弦位置、弦の減衰、ブリッジダンピング、弦の剛性、コースデチューン、リリース、レベルです。

ティンホイッスルは16ボイスです。各ボイスは準備済みの分数遅延 open-open 管と、フィードバックループ内のQ=3の第1管モード帯域通過を使います。非線形 fipple ジェットを圧力と管の戻り圧で駆動し、小さなノイズ項で乱流の立ち上がりを加えます。独立したオシレーターはありません。これは意図的に縮約したモデルで、ジェット対流やトーンホール網全体はモデル化しません。制御項目は息圧、ジェット形状、息ノイズ、アタック、リリース、レベルです。

すべてのボイス保存領域、遅延線、エンベロープ、フィルター状態は音声処理前に準備します。リアルタイム経路では確保、ロック、ブロック、I/Oを行いません。現在の実装はダルシマー24ボイス、ホイッスル16ボイスです。48 kHz、256フレームのステレオコールバックを512ブロック（ウォームアップ96ブロック）測定すると、ダルシマー24ボイスは平均1.030 ms／p99 1.115 ms、ホイッスル16ボイスは平均0.043 ms／p99 0.046 msでした。これらは同時発音が多い場合の最悪バースト上限を示しません。

<a id="recording-data-and-calibration"></a>
## 録音データと較正

参照マニフェストは `tools/eval/references/folk-notes.json` です。18ノート（ダルシマー10ピッチ、学習6・検証4、ホイッスル8ピッチ、学習4・検証4）を含み、MIDIピッチは分割間で重複しません。ホイッスル素材は連続したニ長調スケール1本なので、8つの独立テイクではなく1録音からの定常抜粋です。主ホイッスル指標ではアタックとリリースを除きます。ダルシマ―クリップは公開プレビューですが、利用できる素材はWAVへ変換した公開HQ MP3プレビューで、元WAVの取得には Freesound ログインが必要です。

ホイッスルの音源は [Wikimedia Commons の Whistle.wav](https://commons.wikimedia.org/wiki/File:Whistle.wav)（[直接WAV](https://upload.wikimedia.org/wikipedia/commons/6/65/Whistle.wav)）で、CC BY-SA 4.0です。作者欄は機械可読でないため、マニフェストは Escola Superior de Música de Catalunya (ESMUC) をクレジットしますが、それを作者本人だとは断定しません。ダルシマーのプレビューは [iternetcone の Freesound pack](https://freesound.org/people/iternetcone/packs/19445/) とマニフェスト記載の個別ページから取得し、CC BY 4.0です。準備済みマニフェストのチェックサムは `bcf55f3f56d867ec5f3e88aaf7813b9963df7009e891520ded691f182be19016`（CRLFをLFへ正規化）で、各生キャプチャと準備済みWAVにも厳密なバイトチェックサムがあります。自己相関と倍音間隔で C4／D4／E4 の弱い基音と強い上位部分音を区別します。別録音の High C5／High D5 は別ピッチとして保持し、丸めたオクターブ監査は `tools/eval/references/folk-pitch-audit.json` に残します。

較正は `tools/eval/references/dulcimer-initial.json` と `whistle-initial.json` の初期制御から始めます。これはリリース済み楽器ではなく、新規実装の基準です。最適化器が変更するのは較正ファイルに公開したモデル制御だけです。シード付き差分進化は4反復、個体数倍率4、研磨なしで、ダルシマー7制御に140評価、ホイッスル3制御に60評価を行います。目的に入るのは学習ピッチだけで、この短い検索は大域最適ではありません。レンダラーのベロシティは0.65固定で、録音のダイナミクスから推定しません。測定した音源デチューンをレンダラーへ渡し、抜粋時間からおおよそのノート保持を得ます。ダルシマーは位相重み付き log-mel 損失（アタック30%、ボディ30%、テール40%）、エンベロープ誤差、T90を使い、ホイッスルは定常抜粋の log-mel とエンベロープ誤差を使います。損失は異なるため、楽器間で数値を比較してはいけません。両方とも24 kHz評価レートで30 Hz〜10 kHzの64 HTKメル帯域、512／2048サンプル窓、240サンプルホップを使います。ノートは因果2次30 Hzハイパス後に RMS 0.1へ正規化し、タイムワープとフレーム単位ゲイン調整はしません。ダルシマーのアタック／ボディ／テール境界は0／50／250／3000 msです。ホイッスル参照は定常領域の先頭150 msと末尾100 msを捨て、残った定常抜粋を最大1秒、合成音はノートオン200 ms後から同じ長さだけ比較します。適合は24 kHzで行い、下記のネイティブ結果は最終既定値を48 kHzでレンダリングし、比較用に24 kHzへポリフェーズ再サンプリングします。

ネイティブ48 kHzのホールドアウト結果は次のとおりです。

| モデル | 検証 log-mel 初期 | 検証 log-mel 較正後 | 変化 | エンベロープ dB 初期→較正後 | T90 s 初期→較正後 |
| --- | ---: | ---: | ---: | ---: | ---: |
| Hammered dulcimer | 0.2616068 | 0.2421653 | 7.4316%低下 | 6.7731 → 4.7017 | 0.25623 → 0.16363 |
| Tin whistle | 0.0182215 | 0.0179626 | 1.4209%低下 | 1.2928 → 1.2981 | 測定なし |

各検証結果はホールドアウト4ピッチだけを対象にします。2,000回の対応ピッチブートストラップによる log-mel 変化の対応95%信頼区間は、両モデルで0を含みます（ダルシマー `[-0.05373, 0.01485]`、ホイッスル `[-0.000522, 0.0000651]`）。ホイッスルのエンベロープ誤差は適合後にわずかに悪化します。これは小規模コーパスの回帰測定であり、普遍的な知覚改善の証拠ではありません。ダルシマーではT90を評価しますが、ホイッスルでは定常抜粋の目的にT90を含めません。T90は保持した3秒窓で帯域制限エネルギーの90%を含む時間であり、利用できるプレビューを越えた完全な物理減衰の測定ではありません。ノートごとのスコア、完全精度の適合結果、初期制御、検索範囲、評価器ハッシュ、ランタイムバージョンは `tools/eval/references/{dulcimer,whistle}-*.json` に残します。

独立したピッチ検査では、24 kHzと48 kHzでベロシティ0.35、0.65、0.95の保持ノートを使います。ホイッスルは D5〜D7（MIDI 74〜98）、ダルシマーは C3〜C6（MIDI 48〜84）を対象にし、それぞれ150条件と222条件です。最大絶対ピッチ誤差はホイッスル1.8523セント、ダルシマー0.1567セントです。すべての条件で最強成分は基音でした。最大DC比はそれぞれ0.000830と0.003590です。1.2秒保持から350 msを捨て、20 Hz〜min(10 kHz, 0.49 × サンプルレート)で最強ピークを、補間した基音ピークとともに検査します。

<a id="reproduction"></a>
## 再現

以下はプロジェクトチェックアウトから実行します。参照準備にはMP3プレビュー用の `ffmpeg` が PATH に必要です。Python依存関係は `uv` が管理します。取得と固定（キャッシュ済みキャプチャを使う場合は `--offline` を追加）は次のとおりです。

```bash
uv run tools/eval/fetch_folk_reference.py target/folk-physical/reference --lock tools/eval/references/folk-notes.json
```

評価器用レンダラーをビルドします。

```bash
cargo build --release -p auris-synth --example physical_fit_render
```

最終48 kHz既定値をレンダリングし、固定した初期制御と比較します。`--calibration` は適合の出所を検証・記録しますが、現在の既定値を上書きしません。`--initial-params` で基準を再現できます。Windowsでは下記の `.exe` を使い、macOS／Linuxでは `.exe` を省きます。

```bash
uv run tools/eval/folk_copy.py target/folk-physical/reference target/release/examples/physical_fit_render.exe target/folk-physical/dulcimer-validation.json --model hammered_dulcimer --initial-params tools/eval/references/dulcimer-initial.json --calibration tools/eval/references/dulcimer-fit.json --rate 48000
uv run tools/eval/folk_copy.py target/folk-physical/reference target/release/examples/physical_fit_render.exe target/folk-physical/whistle-validation.json --model tin_whistle --initial-params tools/eval/references/whistle-initial.json --calibration tools/eval/references/whistle-fit.json --rate 48000
```

24 kHz検索を再実行するには、`--calibration` を `--fit-bounds tools/eval/references/dulcimer-bounds.json` または `whistle-bounds.json` に置き換え、`--rate 24000` と `--initial-params` を使います。48 kHz比較を残すため、出力ファイル名は変えてください。ピッチ検査は次のとおりです。

```bash
uv run tools/eval/folk_tuning.py target/release/examples/physical_fit_render.exe target/folk-physical/folk-tuning.json
```

編集可能な試聴例は次で生成します。

```bash
cargo run -p auris-session --example folk_demo -- target/folk-physical/final-demo
```

デモは48 kHzステレオ、12秒で、エフェクト、正規化、バランス調整はありません。測定ピーク値は、ダルシマーソロ0.275070071、ホイッスルソロ0.190560580、結合ファイル0.417004585です。生成プロジェクトにはノートと楽器パラメータが残るため、そのまま編集できます。

<a id="verification"></a>
## 検証

数値DSPテストでは、ピッチ、モード減衰、接触硬度、リリース、コントローラー応答、大きなベンド、極端なレート／制御での有限出力、ブロックサイズ独立性、リセット、コールバック確保数を確認します。セッションテストではネイティブ／GM音色、Undo選択、カスタム制御の保存／読み込み、SoundFontなしでのPCM再現を確認します。ワークスペース実行は4,056テスト（15件は無視）を通過し、Python評価スイートは43テストを通過しました。ワークスペース Clippy、Ruff、警告を拒否した rustdoc、フォーマットも通過しています。

記号プリセットレポートと9つの既存プリセットWAVは変更前チェックポイントとバイト単位で同一で、Audiobox学習スコアも同じです。これは既存プリセットを検証するもので、新しい楽器は評価しません。ハッシュ、スコア、デモレベル、コールバック時間は `tools/eval/references/folk-regressions.json` に残します。

```bash
cargo test --workspace -- --test-threads=1
cargo clippy --workspace --all-targets
cargo run --release -p auris-synth --example physical_bench -- --folk
```
