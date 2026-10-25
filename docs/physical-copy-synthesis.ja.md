<a id="physical-instrument-copy-synthesis"></a>
# 物理楽器のコピー合成

[English](physical-copy-synthesis.md)

内蔵ピアノ、ギター、バイオリンは、固定した時間メルスペクトログラム損失を使い、**実録音**に対して較正している。Rust製の楽器が候補をすべてレンダリングし、探索によって共通の励振・損失パラメーターと12個の放射フィルターゲインを調整する。製品に入るのはその数値だけである。ダウンロード、Python、最適化、プロット、試聴ファイルは開発用ツールである。

<a id="reference-and-split"></a>
## 参照データと分割

[University of Iowa Musical Instrument Samples](https://theremin.music.uiowa.edu/MIS.html) は、Lawrence FrittsとElectronic Music Studiosが作成した。大学は、これらのデータを制限なくダウンロードしてプロジェクトで使うことを認めている。録音の詳細と元のダウンロードリンクは、[ピアノ](https://theremin.music.uiowa.edu/MISpiano.html)、[ギター](https://theremin.music.uiowa.edu/MISguitar.html)、[2012年のバイオリン](https://theremin.music.uiowa.edu/MISviolin2012.html)の各ページにある。

コホートには、Evan Mazunikが演奏したSteinway model B、Brian Penkrotが演奏したRaimundo 118クラシックギター、Leonid Iogansenが演奏したNicolai Tambovskyバイオリンを使う。ギターとバイオリンは無響室で、ピアノは通常の部屋で録音された。そのため放射のフィットには励振、マイク、録音環境による色付けも含まれ、分離された胴体アドミタンスではない。

元録音は36件、抽出したノートは165個で、pp/mf/ffを速度0.35/0.65/0.95に対応させる。これらは順序尺度の演奏条件であり、測定したMIDI速度ではない。ダウンロードのロックファイルは `tools/eval/references/iowa-captures.json` である。ノートのロックファイル `iowa-notes.json` には、各ソース、切り出しオフセット、音高、測定チューニング、分割、SHA-256を記録する。ダウンローダーは元データのハッシュを検証する。準備ツールではノートロック全体を検証できる。準備したfloat WAVからタイムスタンプ付きPEAKメタデータを除くことで、繰り返し抽出してもバイトハッシュを維持できる。

| 楽器 | 音高の範囲 | 学習ノート | 検証ノート |
| --- | --- | ---: | ---: |
| ピアノ | 48, 52, 55, 60, 64, 67 | 6 | 12 |
| ギター | 40–47, 50–59, 64–71 | 34 | 44 |
| バイオリン | 55–59, 62–71, 76–83 | 30 | 39 |

ピアノの音高52/60/67はホールドアウトする。ギターとバイオリンでは、MIDI番号を3で割った余りが1になる音高をホールドアウトする。学習対象の音高であっても、pp録音はすべてホールドアウトする。どちらの最適化段階にも学習ノートだけを使い、フィットが終わってから検証データを読み込む。これにより、別の楽器ではなく、同じ録音セッションに含まれる新しい音高と弱いダイナミクスを検証する。

音高を条件にした倍音追跡で安定した音階音を特定し、上昇順であることを要求する。オンセット検出は最大200 msさかのぼり、局所的な2% RMSしきい値と5 msのガードを使う。可聴倍音から固定チューニングオフセットを推定し、ラベルから75セント以内であることを確認する。弱い倍音は除外する。レンダラーにはノートオン前にこの測定オフセットを渡す。検証録音に対して、音高、時間アラインメント、ノートごとの合成パラメーターを最適化することはない。

<a id="objective-and-search"></a>
## 目的関数と探索

各ノートは24 kHzのモノラルで、キーを押したままオンセット後の最初の**1秒**を収録する。この共通の窓で全録音を扱える。各ノート全体をRMS 0.1に正規化する。個々のフレームは正規化しないため、アタックと減衰を測定でき、時間ワープもない。

2種類のHann STFTで512/2048サンプルの窓と240サンプルのホップを使う。30 Hzから10 kHzを64本の三角形HTKメル帯域で覆い、面積を1に正規化する。パワーは `log(1 + 10000 * mel_power)` で圧縮する。各解像度で、最初の15フレームと残りのフレームについて絶対誤差を別々に平均し、その平均に同じ重みを与える。最終損失は、両方の解像度と全ノートを等しい重みで平均する。この定義では小さいほど近い。これは無次元の音響距離であり、知覚品質スコアでもデシベル単位の測定値でもない。

`fit_physical_mel.py` は2段階を2回交互に実行する。

1. シード付き差分進化で実際のRustの励振・損失制御を探索する。パラメーターごとの母集団サイズは5、世代数は8、ポリッシングは行わない。範囲とデフォルト値はレポートに保存する。シードは20261001と20261002である。
2. 因果フィルターバンクの代理モデルで12個のゲインを正則化付き最小二乗によりフィットする。各ゲインの範囲は±9 dBに制限する。アンカーと隣接ゲインのペナルティーはそれぞれ0.012と0.01である。主たる時間L1目的関数を改善した場合だけフィットを採用する。最適化の前に、代理モデルのPCMが実際のRustバンクと最大誤差0.0002以内で一致することを確認する。

ピアノでは硬さ、打鍵位置、減衰、ダンピング、剛性を探索する。ギターでは硬さ、ピック位置、減衰、ダンピングを探索する。バイオリンでは弓圧と弓速も探索する。この窓ではキーリリースを測定しないため、リリースは固定する。ボディーブレンドは0.65のままにする。フィットしたフィルターゲインと工場出荷時の制御値をRustに丸めて組み込み、最終測定には実際にコンパイルされた値を使う。プロジェクトで明示されたパラメーターは、新しいデフォルト値より常に優先される。

別のスカラーで、以前の工場出荷時ボイスに対する学習ノートのRMS中央値を保つ。レベルを自動化している場合も含め、これは楽器内部に適用し、公開レベルのデフォルト値は変更しない。ゲイン不変の目的関数には含めない。ベース、ベル、マレットの設定は変更しない。SoundFontのインポートと再生経路にも影響しない。

<a id="first-calibration-result"></a>
## 最初の較正結果

ベースラインはこの較正前の `5a6ccac` である。最終ツールで同じシードを使って学習探索を繰り返し、同じ結果を得た。24 kHzで実際のワーカーバイナリ2つを評価した。

完全な較正レポートとノートごとの検証レポートは、`tools/eval/references/physical-mel-calibration.json`、`physical-mel-validation.json`、`physical-mel-validation-48k.json` にコミットされている。

| 楽器 | 学習 前 → 後 | 検証 前 → 後 | 検証の減少率 | 改善ノート |
| --- | ---: | ---: | ---: | ---: |
| ピアノ | 0.258680 → 0.161204 | 0.234200 → 0.169513 | 27.6% | 10/12 |
| ギター | 0.183417 → 0.119994 | 0.211496 → 0.148237 | 29.9% | 42/44 |
| バイオリン | 0.114397 → 0.077856 | 0.124071 → 0.084689 | 31.7% | 35/39 |

同じ24 kHzの特徴空間にリサンプリングした別の48 kHzレンダーでも、検証は改善する。ピアノは0.234175 → 0.171541 (26.7%)、ギターは0.210189 → 0.148302 (29.4%)、バイオリンは0.126719 → 0.088016 (30.5%)である。この確認では、変更していないストリング／モーダルアルゴリズムを通じて以前のデフォルトとフィルターバンクを再構成する。その再構成を使う前に、凍結した24 kHzベースラインとのPCM一致を検証する。上の24 kHz表では、凍結したバイナリを直接使っている。今後オシレーターを変更する場合は、代わりに本番レートでワーカーを別途凍結する必要がある。

評価では決定的な試聴ファイルを出力する。ピアノ、ギター、バイオリンをそれぞれ**録音 / 前 / 後**の順に並べ、ノート全体のRMSと共通のピーク余裕をそろえる。MIDI 60/55/67に最も近いmf検証ノートをスコア計算前に選び、最も成績のよいノートを選ぶことはない。プロットでは各楽器の3パネルに同じカラースケールを使う。

クリックを防ぐため、試聴用の切り出しには境界に5 msのフェードを入れる。スコア計算には加工していないPCMを使う。

シンボリック測定は前後でバイト単位に同一である。通常のバランスを取った9つの作曲プリセットでは、Audiobox Aestheticsの結果は混在した。平均CEは5.938 → 5.887、PQは7.749 → 7.734である。ポップバンドとロックではCEが改善した一方、オーケストラとアンビエントではそれぞれ約0.36と0.43低下した。アンビエントのPQは約0.17低下した。したがって、孤立ノートのメル損失が小さくなっただけでは、すべての作曲や持続パッドの音がよくなったとは言えない。これらの学習済みスコアは補助的な確認であり、探索の目的ではない。

48 kHz、256フレーム、24ボイス保持の条件でリリースビルドを測定すると、5.333 msのコールバック予算に対し、ピアノは平均/p99が1.025/1.681 ms、ギターは0.083/0.123 ms、バイオリンは0.073/0.116 msだった。長く存続するピアノモードにより、CPU平均は直前の較正の約0.82 msから増えた。コールバック内のアロケーション検査は引き続き合格している。これらの時間は、このマシンとワークロードにおける値である。

最初の段階ではアタックと初期のサステイン／減衰を扱う。続く[持続音・軌跡の較正](physical-trajectories.ja.md)では、3秒／6秒のノート、ピッチ・表現ガイド付きの実バイオリングリッサンド、注釈付きリリースを追加する。一部のピアノEQゲインは±9 dBの境界に達しており、コンパクトな励振／放射構造が再構成を制限していることを示す。メル距離は位相を捨て、帯域内で倍音を解像するだけなので、試聴と音高・安定性のテストも引き続き必要である。

<a id="reproduce-and-continue"></a>
## 再現と継続

Pythonツールは `uv` で実行する。WindowsでPATHに `uv.exe` がない場合は、インストール先のパスを使う。ツールはスクリプト用環境を使うため、トレーナー環境やGPUは必要ない。

```sh
uv run tools/eval/fetch_physical_reference.py target/copy-synthesis/iowa
uv run tools/eval/prepare_physical_reference.py target/copy-synthesis/iowa \
  --lock tools/eval/references/iowa-notes.json
cargo build --release -p auris-synth --example physical_fit_render
# Freeze this executable before changing any audio code; use .exe on Windows.
cp target/release/examples/physical_fit_render target/copy-synthesis/before-renderer
uv run tools/eval/fit_physical_mel.py target/copy-synthesis/iowa \
  target/copy-synthesis/before-renderer target/copy-synthesis/fit.json
# Review held-out loss, apply candidate controls/gains and cumulative output normalization,
# then rebuild the example and compare the actual Rust builds:
uv run tools/eval/evaluate_physical_mel.py target/copy-synthesis/iowa \
  target/copy-synthesis/before-renderer target/release/examples/physical_fit_render \
  target/copy-synthesis/evaluation
```

アーカイブしたワーカーで最初の較正を繰り返すには、フィッターに `--profiles tools/eval/references/physical-body-before-copy.json` を渡す。48 kHzのクロスチェックでは、さらに `--production-rate 48000 --calibration target/copy-synthesis/fit.json` を使う。正確な抽出のため、記録済みの依存関係バージョンを固定する。NumPy 2.5.3、SciPy 1.18.1、SoundFile 0.14.0で、元の実行にはPython 3.12.14を使った。正規ノートマニフェストのSHA-256は `878f66a719dd47400ad6246d9a23c8ecb09676c26837bbfa569993099b76c4bc` である。

マニフェストの識別情報は、改行コードに依存せず、UTF-8でエンコードしたソート済みのコンパクトなJSONで定義する。レポートにはワーカーのハッシュ、デフォルト、範囲、段階の履歴、ノートごとの全エラーも記録する。元のDSPベースラインを再ビルドするには、独立したチェックアウトで `5a6ccac` のソースを使い、現在のワーカー例をそこへコピーして、`auris-synth` の開発依存関係に `serde_json` を追加する。独立したCargoターゲットディレクトリを使う。再ビルドした実行ファイルのハッシュはツールチェーンとビルドパスに依存するため、元のワーカーバイナリのハッシュではなくPCMとメトリクスを比較する。

```sh
uv run --with numpy --with scipy --with soundfile --with pytest pytest \
  tools/eval/test_physical_mel.py tools/eval/test_fit_physical_body.py tools/eval/test_physical_ab.py
uv run --with ruff ruff check tools/eval/*physical*.py
cargo test --workspace
cargo test -p auris-synth --example physical_fit_render
cargo clippy --workspace --all-targets
```

数値テストでは、無音または非有限の候補を拒否し、ゲインと極性の不変性を検証し、音高・倍音・アタック・減衰の変化を検出し、抽出とチューニングを確認し、固定した学習／検証分割を保護する。標準DSPテストでも、チューニング、損失、速度、制御、ライブレベル正規化、極端なノート、ゼロアロケーションを継続して確認する。
