<a id="learned-music-calibration-october-2026"></a>
# 学習ベースの音楽較正（2026年10月）

[English](learned-music-tuning.md)

評価器は [evaluation.md](evaluation.ja.md) に記載した固定済みのTuneJuryおよびMuQ-MuLan手順を使います。この実験では作曲家を変更する前に9つのプリセットをすべて測定し、成功候補と却下候補の両方を保持しました。[測定成果物](data/learned-music-tuning-2026-10-04.json)には、モデルとパッケージのバージョン、プロンプト、チェックポイントとレンダラーのハッシュ、試行スコア、WAVハッシュ、窓位置、記号測定値、採用判断が含まれます。

<a id="method"></a>
## 方法

基準レンダラーは `354af0f2724ae9c75b7e1d87500ed10fa9abde32` からビルドしました。初期基準では各プリセットの既定シードと101/102、10秒窓3つを使いました。すべてのレンダーは追加のエフェクトテールなしのfloat32 WAVで、どちらの評価器もゲイン正規化を適用しません。

広いダイヤル探索は、101/102のシードでプリセットごとに元の設定を含むOptuna試行8回を実行しました。選択にはTuneJury報酬とMuQ-MuLan正コサインが低下しないことを求め、その後301/302で別の検証を行いました。探索では中央の窓を1つ使いました。キー、進行、形式、楽器編成は固定し、記号ルーラーをライター変更の前後に実行しました。

ライター実験では細分化範囲2と4を元の3と比較しました。範囲2は既存のメロディ反復ガードに失敗し、結合スコアも改善しませんでした。範囲4はチップチューンのコホートを改善しましたが、ロックとシンセウェーブへの広い変更は検証に残りませんでした。最終ライターはChiptuneパレットだけを変更し、密度ダイヤルの早い段階で準備を速め、1拍あたりの細分化を4に制限します。保持された到達とフレーズの呼吸はジェスチャーの一部です。

次の探索では、較正済みライターの下でchiptuneとgame-loopについて、humanizeとdynamics以外のダイヤルを固定し、それぞれ12回試行しました。両候補は中央窓の別検証に合格しました。出荷設定は次のとおりです。

| プリセット | Humanize | Dynamics |
| --- | ---: | ---: |
| chiptune | 0.384 | 0.829 |
| game-loop | 0.668 | 0.407 |

<a id="fresh-comparison"></a>
## 新しい比較

候補選択後だけにシード401/402を使いました。最終比較は**3つ**の窓（最初、中央、最後）に戻しました。各行は2シードのペア平均差です。コサインは埋め込み類似度であり確率ではなく、TuneJury報酬は未較正です。

| プリセット | TuneJury報酬差 | MuQ-MuLan正コサイン差 |
| --- | ---: | ---: |
| chiptune | −0.002384 | +0.004446 |
| game-loop | +0.134973 | +0.008355 |
| 更新プリセットの平均 | +0.066295 | +0.006401 |
| 9プリセット全体の平均 | +0.014732 | +0.001422 |

chiptuneの報酬低下は共有パレット較正におけるトレードオフとして明示的に残しました。新しい比較は、このプリセットの報酬向上を示していません。集計では主にgame-loopによって両方の目的が改善しました。新しいシード2つだけでは統計的有意性や聴取者の好みは確立できず、個別の窓やシードは悪化する場合があります。

広いjazz候補は中央窓の検証には合格しましたが、3窓手順の新しいシード両方で報酬を失いました（平均報酬−0.040662、正コサイン+0.025173）。プリセット変更は却下して元に戻しました。他の広いダイヤル候補は結合検証に失敗するか、試行0を保持しました。

最終CLIは新しい18ケースをすべて再レンダーしました。更新した4ファイルは、すでに採点済みのWAVハッシュと完全に一致しました。pop-band、city-pop、rock、jazz-trio、orchestral、synthwave、ambientは両シードで基準WAVとバイト単位で一致しました。これらの差はゼロなので、同一ペアについて絶対的な新モデルスコアを再推定せず省略しました。却下したjazzのレンダーと編集可能なプロジェクトは最終ケースとは分けて保持しています。

<a id="repeating-the-experiment"></a>
## 実験の再実行

編集前に基準CLIと対応する `resolved_dials` の例をアーカイブします。焦点を絞った探索には、ライター較正だけを含む第2のCLIを使います。

```sh
uv run tools/eval/music.py --preset all --seeds 3 --cli baseline/auris --workdir baseline/renders --json baseline/music.json
uv run tools/eval/tune.py --preset all --trials 8 --segments 1 --cli baseline/auris --dials-resolver baseline/resolved_dials --workdir broad --out broad.json
uv run tools/eval/tune.py --preset chiptune --preset game-loop --dials humanize dynamics --trials 12 --segments 1 --cli writer/auris --dials-resolver baseline/resolved_dials --workdir focused --out focused.json
```

新しい比較では、保存した各レンダラーをシード401と402で明示的に作曲し、`--bit-depth 32 --no-tail` でレンダーしてから、対応するフォルダーを `music.py --segments 3` で採点します。`game-loop-s401.wav` のようなファイル名で固定プロンプトプロファイルを選びます。同じモデルと前処理条件のレポートを比較するには `--baseline` を使います。JSON内のローカル成果物パスはリポジトリルートからの相対パスです。WAV、プロジェクト、実行ファイルは `target/tuning` に残ります。
