<a id="piano-cover-transcription-confidence-and-missing-notes"></a>
# ピアノカバー転写: 信頼度と欠落音符

[English](piano-transcription-evaluation.md)

2026-10-21 のデコーダー改訂で、付属 Basic Pitch ピアノカバーバックエンドを改善しました。音符デコードと重大なステレオ相殺処理を変更しましたが、モデル重み、作曲家、楽器合成、保存プロジェクト内の既存音符は変更していません。ライブエージェントも同じ `transcribe_piano_file`／`create_piano_cover` セッション経路を使います。

[測定記録](../tools/eval/references/piano-transcription-2026-10-21.json)には録音／参照ハッシュ、実行ファイルハッシュ、全ケース、オンセット／オフセット指標、選択判断、回帰を保持します。ベースラインはリビジョン `e0d97d06` です。

<a id="what-changed"></a>
## 変更点

以前は0.3を超える活動だけで、オンセットなしに音符を開始できました。弱い持続予測や実アタック前後の導入／リリースが余分なイベントになり、11フレームの最小値は短い反復音符を消し、最初のフレームは学習済みオンセットになれませんでした。単体テストは変更前にこれらの失敗を再現します。

新しいデコーダーは、0.5の学習オンセットピークをまず受け付け、0.3の活動、0.4以上の平均信頼度、6フレーム（約69 ms）で裏付けます。非活動フレームは最大4つ橋渡しします。第2パスは、8フレーム以上、平均0.45、ピーク0.5の残余活動から弱いオンセットを回復します。承認済み音符の前8フレームと後4フレームをガードして重複アタック／リリースを防ぎ、回復時は0.1以上の活動を最大4フレームだけバックトラックします。反復アタック、重なり、ソース長の境界、既存の長いファイルのタイムスタンプ対応は保持します。

チャンネル平均は通常の前処理のままです。窓のモノラルパワーが最強チャンネルの1%未満なら、録音を消去せずそのチャンネルを使います。数値波形テストと実モデル推論は、逆位相ステレオ、同位相ステレオ、無音、有限でないサンプルを対象にします。ミックス内の任意の相殺を解決するものではありません。

モデルは Spotify の変更していない Apache-2.0 ICASSP 2022 ONNX アーティファクトで、出所は[付属告知](../crates/auris-analysis/NOTICE-BASIC-PITCH.md)にあります。Rust の後処理は Auris 実装です。[上流後処理](https://github.com/spotify/basic-pitch/blob/fa5997af0a8210982619003269994a1be25eddf3/basic_pitch/note_creation.py)はオンセット／残余デコードの参考であり、Python と同じ動作を主張するものではありません。

<a id="frozen-scoreaudio-pairs"></a>
## 固定したスコア／音声ペア

`evaluate_piano_transcription` は既存の作曲家で36個の短いペアを作ります。

* `city-pop`、`jazz-trio`、`orchestral`、ヴァース4小節とリリース1秒。
* 開発シード101/102、検証301/302、保留901/902。
* ドラムを除く全ピッチパートのピアノ演奏と、ドラムを干渉として残すネイティブ楽器ミックス。ネイティブ楽器は外部 SoundFont 依存を避けます。

生成器は、タイミング／ベロシティ変換と反復を含むクリップの演奏音符を一度解決します。正確なイベントを通常クリップに固定し、アノテーションとレンダラーは同じ固定演奏を使います。ドライミックスの共通ピークは0.7です。スコアラーは保存プロジェクトの一定テンポと960ティックの拍グリッドを独立に読み、固定音符に対する各アノテーションを検証します。未解決の演奏変換、ループ、ミュートトラック、タイムスタンプ不一致は評価に失敗します。

12 ms以内の同音高参照は統合します。ピアノ鍵盤は同時のユニゾンを2つの声部として表せないためです。マッチングは、正確な MIDI 音高、オンセット50 ms許容、最大一対一二部マッチングを使います。タイミング誤差は未一致推定（FP）と欠落参照（FN）の両方になり得ます。レポートにはオンセット100 msスコアと、リリース用 max(50 ms, 参照長の20%) を使うオンセット／オフセットスコアも残します。これらはアレンジ品質や両手演奏のしやすさを測りません。

開発専用の探索では414の後処理設定を比較しました。オンセットしきい値0.6は F1 を上げましたが検証の欠落を増やしたため却下しました。選んだ0.5と信頼度付き回復は、検証で FP と FN の両方を減らします。保留と既存スコアの候補結果は選択後にだけ読みました。公開する推定値は探索用 Python デコーダーではなく、実 Rust セッションバックエンドから取得します。Python は独立したマッチングとアノテーション検査を行います。

| 作曲家分割、ピアノとミックス | FP 変更前 → 後 | FN 変更前 → 後 | Onset F1 変更前 → 後 |
| --- | ---: | ---: | ---: |
| 開発、12ケース | 486 → 266 | 248 → 232 | 0.616 → 0.708 |
| 検証、12ケース | 460 → 267 | 277 → 270 | 0.619 → 0.693 |
| 保留、12ケース | 568 → 327 | 292 → 287 | 0.595 → 0.675 |

改善は一様ではありません。保留ミックスは FP を189 → 105に減らしますが FN は202 → 203に増えます。検証の city-pop ミックス2件は onset F1 が少し下がります。Orchestral ミックスには多くの欠落が残ります。ケースごとの記録にはこれらの失敗も残ります。

<a id="an-independent-existing-score"></a>
## 独立した既存スコア

以前の監査に使った独立 Mutopia SATB *Ode to Joy* スコアをピアノと reeds/strings でレンダーしました。音符も新デコーダー結果もパラメーター選択には使っていません。238イベントの参照は書き出した MIDI から独立に読み、入力 WAV は[測定スナップショット](../tools/eval/references/agent-production-audit-2026-10-21.json)にハッシュされたファイルと一致します。これは既存作品の管理された合成で、独立した音響録音ではありません。

| 入力 | FP 変更前 → 後 | FN 変更前 → 後 | Onset F1 変更前 → 後 |
| --- | ---: | ---: | ---: |
| Piano | 35 → 20 | 0 → 0 | 0.932 → 0.960 |
| Reeds + strings | 60 → 19 | 144 → 115 | 0.480 → 0.647 |

混合ソースのオンセット precision は61.0%から86.6%、recall は39.5%から51.7%に上がります。オンセット／オフセット F1 は0.311から0.453に上がりますが、多くのリリースと音符はなお誤っています。既存スコアのドラフトは実際のピアノカバーセッションコマンドで適用、保存、レンダーし、1ステップ Undo／Redo で検証しました。比較 WAV は両版で同じゲインを使い、ハッシュとゲインは記録にあります。主観的な聴取スコアは主張しません。

<a id="reproduce-and-extend"></a>
## 再現と拡張

一度生成したら、すべてのデコーダーで同じマニフェスト、音声、アノテーションを保持します。

```sh
cargo run -p auris-session --example evaluate_piano_transcription -- generate target/piano-transcription/corpus
cargo run -p auris-session --example evaluate_piano_transcription -- run target/piano-transcription/corpus/manifest.json target/piano-transcription/before
uv run --with numpy --with scipy tools/eval/piano_transcription.py target/piano-transcription/corpus/manifest.json target/piano-transcription/before --json target/piano-transcription/before.json
# Change the decoder, retaining the frozen corpus and archived baseline executable.
cargo run -p auris-session --example evaluate_piano_transcription -- run target/piano-transcription/corpus/manifest.json target/piano-transcription/after
uv run --with numpy --with scipy tools/eval/piano_transcription.py target/piano-transcription/corpus/manifest.json target/piano-transcription/after --json target/piano-transcription/after.json --baseline target/piano-transcription/before.json
cargo run -p auris-session --example evaluate_piano_transcription -- cover target/piano-transcription/after/city-pop-901-mixture.json target/piano-transcription/listening-cover
```

出力ディレクトリ／レポートは新規でなければなりません。履歴ベースラインの再現では、`e0d97d06` に対してこの開発例をビルドできます。セッション API は当時から存在します。ペア採点は、録音、ラベル、固定プロジェクト、モデル重み、マッチング規則、コホートの変更を拒否します。比較は1ファイル10,000音符／候補ペア400万件までです。大きい場合は抜粋を使います。Python/scipy は評価専用で、本番推論はキャンセルと既存の10分／8チャンネル／200,000音符制限を持つ Rust/ONNX CPU のままです。

```sh
uv run --with numpy --with scipy --with pytest pytest tools/eval/test_piano_transcription.py
uv run --with ruff ruff check tools/eval/piano_transcription.py tools/eval/test_piano_transcription.py
```

元の作曲家ペアは安価な管理回帰を可能にしますが、ネイティブ楽器とドライミックスは限定された領域です。ボーカル、密なマスター済み／商用ミックス、独立した音響ピアノ録音の精度は示しません。その主張には、より広くレビューした録音／アノテーションペアが必要です。[直接音声 LLM 実験](direct-audio-transcription-experiment.ja.md)は同じ保留録音で、認識をローカルモデル生成音符に置き換えられるかを検査します。
