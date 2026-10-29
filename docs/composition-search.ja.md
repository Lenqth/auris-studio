<a id="bounded-composition-search"></a>
# 制約付き作曲検索

[English](composition-search.md)

作曲検索は曲を繰り返し生成し、書き出されたノート数を測定して、生成されたスコアのうち最良のものをそのまま保持します。音声デバイス、SoundFont、モデル、Python環境、UIは必要ありません。ヘッドレスで順番に実行され、`auris_session::composition_search::search_composition` として公開されています。

<a id="run-the-example"></a>
## 例を実行する

```sh
cargo run -p auris-session --example compose_search -- chiptune random 24 42 7 search-random
cargo run -p auris-session --example compose_search -- chiptune hill 24 42 7 search-hill
cargo run -p auris-session --example compose_search -- MySong.asong hill 32 42 7 search-song 16
```

引数は、プリセット名または `.asong` ファイル、アルゴリズム（`random` または `hill`）、試行回数、検索シード、作曲シード、新しい出力ディレクトリ、そして任意の目標ノート数（既定値は `12`）です。この例は、1つのパートの密度と、最初に演奏されるセクションの強度を、範囲 `[0, 1]`、山登り法のステップ `0.1` で検索します。少なくとも1つの演奏セクションに影響する密度を持つメロディーパートを優先し、そのようなパートがなければ強度だけを検索します。暗黙のパート密度は、実行前に明示的な `0.5` になります。出力にはこの準備済みの基準値が記録されるため、開始設定を確認できます。

新しいディレクトリには `base.asong`、候補が成功した場合の勝者 `best.asong`、`report.json` が含まれます。レポートにはシード、範囲、目標、候補の結果（順序付き）、失敗数、終了理由、勝者候補の評価が記録されます。候補の項目は `base.asong` に対して検索した値を記録し、それ以外のフィールドは固定されます。端末には保持されたスコアのアレンジ概要が表示されます。返されたスコアは実行中メモリに保持され、`best.asong` への書き込みでは生成済みノートではなく入力値が記録されます。

<a id="metric-and-search-space"></a>
## 指標と検索空間

評価器は全トラックの書き込みノートイベントを数え、末尾を含むスコアの実際の小節数で割ります。和音の構成音とドラムノートは、それぞれ個別のイベントとして数えます。評価対象はパフォーマンス変換前の書き込みスコアです。タイミングのヒューマナイズ、音源の選択、レンダリング音声、セッションが後から実体化する歌声ノートは目的関数に含まれません。

```text
notes_per_bar = written_note_count / score_length_in_bars
fitness = -abs(notes_per_bar - target_notes_per_bar)
```

適合度は最大化します。0 は完全一致で、0 に近いほど良い結果です。目標は正の有限値でなければならず、実行中は変わりません。測定した密度と目標は各評価に添えられます。単位を隠す重み付けや正規化は行わず、目標との一致だけを測ります。近い結果が示すのはアレンジの密度であり、普遍的な音楽品質ではありません。特定の空間では目標に到達できない場合があります。保持を決める前に結果を確認し、聴いてください。

`SearchSpace` は `PartDensity`、`SectionIntensity`、またはその両方を受け付けます。それぞれ既存のパートまたは演奏セクションを名前で指定し、`ParameterBounds { min, max, step }` を持ちます。範囲は有限で `[0, 1]` 内にあり、`min < max` でなければなりません。ステップは正で、範囲より広くなく、`f32` の値を変更できる大きさである必要があります。基準値も範囲内に入っていなければなりません。選択したパートの密度は明示的に指定します。

既存のリズム上書き、セクションのパート一覧、セクションごとの密度調整は引き続き適用されます。制御したいフレーズに影響するパラメータを選んでください。

選択していない設定フィールドはすべて固定されます。各候補は同じ明示的な作曲シードを使うため、パラメータ変更と別のランダムテイクを切り分けられます。独立した検索シードは候補を提案する順序を制御します。

<a id="reusable-command"></a>
## 再利用できるコマンド

```rust
use auris_session::composition_search::{
    DensityTarget, ParameterBounds, PartDensity, SearchMethod, SearchSpace,
    SongSearchRequest, search_composition,
};
use auris_session::prelude::SongSpec;

let mut base = SongSpec::default();
base.parts[0].density = Some(0.5);
let part_name = base.parts[0].name.clone();
let request = SongSearchRequest {
    base,
    space: SearchSpace {
        part_density: Some(PartDensity {
            part: part_name,
            bounds: ParameterBounds { min: 0.1, max: 0.9, step: 0.1 },
        }),
        section_intensity: None,
    },
    attempt_budget: 24,
    search_seed: 42,
    composition_seed: 7,
    algorithm: SearchMethod::HillClimb,
    evaluator: DensityTarget { target_notes_per_bar: 12.0 },
};
let result = search_composition(&request, || false).unwrap();
// Keep result.best's exact score; do not compose its recipe again to retrieve the winner.
```

セッションモジュールは検索APIを再エクスポートするため、フロントエンドは `auris-session` だけに依存できます。実装は純粋なコンポーザーと同じ `auris-compose` にあります。`Composer` はスコアを作り、`Evaluator` はそれを測定し、`SearchAlgorithm` は `ask` でパラメータを提案し、`tell` で結果を受け取ります。通常の `run_search` 関数が反復、予算の計上、キャンセル、履歴、最良スコアの保持を担当します。具体的な曲検索アルゴリズムに依存せず、カスタムのコンポーザーや評価器も同じランナーを利用できます。`run_search_with_progress` と `search_composition_with_progress` は、記録した各試行の後に同期オブザーバーも呼び出します。フロントエンドは、このワーカーコールバックから軽量な進捗を通知できます。キャンセル、順位付け、保持の意味は変わりません。

ランダム検索は各試行で、範囲内のパラメータを独立にサンプリングします。山登り法は最初に基準値を提案し、その後は現在の値の周囲で許可されたパラメータを1つずつ変更します。成功した結果が適合度を厳密に改善した場合だけ現在値を置き換え、同点と失敗では保持します。ステップが範囲を越える場合は境界値に丸めます。境界値から外向きに進むときは、追加の乱数を引かず内向きに変えます。成功した現在値が得られるまでは、次の候補をサンプリングします。山登り法は局所最適に落ち着く可能性があるため、同じ予算で複数の検索シードを使い、ランダム検索とも比較してください。

<a id="results-failure-and-reproducibility"></a>
## 結果、失敗、再現性

予算が0の場合や評価器の設定が不正な場合を含め、無効なリクエストは検索開始前にエラーになります。検証、作曲、評価の失敗を含め、提案された各候補は1回の試行として消費されます。すべての結果は候補ID、完全なパラメータ、作曲シードとともにアルゴリズムへ渡され、記録されます。有限でない適合度や指標の診断値は、順位付けやフィードバックの前に評価失敗になります。

ランナーは適合度が同点の場合、最初の候補を残し、生成された `Composition`、候補入力、評価をそのまま保持します。結果には順序付き履歴、失敗数、明示的な終了理由が含まれます。すべて失敗した場合、最良スコアはありません。キャンセルは試行の間に確認され、部分的な結果を返します。現在の同期コンポーザーには内部キャンセルフックがないため、進行中の作曲が終わってから次の確認を行います。カスタムアルゴリズムは `ask` から `None` を返して終了することもできます。

同じビルドの現在の決定的なコンポーザーでは、同じ設定とシードから同じ順序の結果とスコアが得られます。この保証は、コンポーザー、指標、乱数生成器の実装が変わった場合には及びません。アレンジを保持するときは生成済みノートを保存してください。既存のセッションは `Session::compose_without_balance` で保持したスコアを採用し、結果のプロジェクトを保存できます。ここでは評価済みのスコアを直接使います。保存した `.asong` だけではレシピにすぎず、新しいビルドでは異なる結果になる可能性があります。

重点テストでは、実際のコンポーザーの順位付けと再現性、フィードバックに依存する山登り法の変更、変更不可のフィールドと範囲、同点、失敗段階、有限でない結果、予算、早期終了、キャンセル、全失敗検索を確認します。これらは最適化の動作を検証するものです。音楽的・音響的な評価は、[evaluation.md](evaluation.ja.md) に記載した試聴と測定のワークフローで行います。
