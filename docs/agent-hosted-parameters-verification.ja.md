# 外部プラグインのエージェント操作検証

検証日: 2026-10-26（Windows）

## 検証経路

音源は `inspect_instrument` → `set_instrument_params`、エフェクトは
`mixer_graph` の `parameters` → `set_parameter` を使用した。
デスクトップのライブエージェントが使うフラットな引数からセッションコマンドへの変換と、
MCPが使う保存プロジェクト用のディスパッチを両方通した。
モデルが自然言語から適切な操作を選ぶ能力や、音楽的な改善の良否は評価していない。

実プラグインごとに次を確認した。

- 実際のパラメータ名・キー・範囲・値の形式・読み取り専用フラグ・ステップ数の取得。
- 範囲外の値を拒否し、プロジェクトを変更しないこと。
- 値の変更、独立した再取得、レンダリング音声への反映。
- 同じ変更を再試行してもUndoが増えず、Undo／Redoが機能すること。
- 保存・再読込後の値と音声の復元、保存済みプロジェクト用ツールによる再変更。
- 保存済みの同一値を再設定してもファイルを書き直さないこと。

## 実プラグインの結果

| 形式 | プラグイン | 操作 | 左チャンネルRMS（変更前 → 変更後） | 結果 |
| --- | --- | --- | --- | --- |
| VST3音源 | Vital | Volume: 0.73965603 → 0 | 0.12513068 → 0 | 成功 |
| VST3エフェクト | DDSP Effect | Output Gain: 1 → 0 | 0.03922334 → 0.00003922 | 修正後成功 |
| CLAP音源 | Surge XT | Global Volume: 0.957797 → 0 | 0.06997900 → 0.00144989 | 成功 |
| CLAPエフェクト | Surge XT Effects | FX Type: 0 → 0.1 | 0.01181732 → 0.05914568 | 成功 |

RMSは音声の変化を示す数値であり、音楽的な品質評価ではない。
Surge XTの値は測定するインスタンスによって多少変動する。
CLAPも、プラグイン自身が0〜1の範囲を公開する場合がある。

## 修正した問題

### 外部VST3エフェクトが処理を開始していなかった

グラフは、呼び出し元が用意したエフェクトを準備済みとして扱い、
`Effect::prepare` を呼んでいなかった。
CLAPは生成時に有効化されるが、VST3の独立したレンダラーは `prepare` で処理を開始する。
そのため、VST3エフェクトは読み返した値が変わっても、音声をそのまま通していた。

グラフ側で、外部から渡されたエフェクトにも `prepare` を呼ぶよう修正した。
DDSP Effectは修正前にOutput Gainを変更してもRMSが0.01768309のままだった。
修正後は表のとおり約60 dB減衰し、保存後の復元も通った。
準備されて初めて入力を減衰するテスト用エフェクトでも回帰テストを追加した。

### 読み取り専用の項目に書き込めた

CLAP／VST3の読み取り専用フラグが共通のパラメータ情報に渡されず、
変更できない出力項目にも値を保存し、エージェントへ成功を返せた。
フラグを保持し、静的な値の変更とオートメーションへの書き込みを拒否するよう修正した。
音源のバッチは全項目を変更前に検証するため、後半に不正な項目があっても途中まで変更しない。
テスト用CLAP音源・エフェクトで修正前の失敗を確認し、回帰テストにした。

### VST3の値の単位と離散ステップが不明瞭だった

VST3の値は0〜1の正規化値だが、表示単位のHz／dBがそのまま応答に出ていた。
音源・エフェクト・オートメーションの応答で `value_domain` と `display_unit` を分け、
`steps`、`read_only`、既定値も返すようにした。
`steps=N>1` の有効値は `min+i*(max-min)/(N-1)`（`i=0..N-1`）。
VST3の物理単位への自動変換はこの検証の対象に含めていない。

## 再実行

通常の回帰テストは `cargo test --workspace` で実行する。
実プラグイン検証はインストール環境に依存するため、明示実行するテストとして追加した。

```powershell
$env:AURIS_AGENT_SMOKE_PLUGIN = 'C:\Program Files\Common Files\VST3\Vital.vst3'
$env:AURIS_AGENT_SMOKE_CONTROL = 'Volume'
cargo test -p auris-toolbox --test external_hosted_parameters -- --ignored --nocapture
```

各ケースを別のPowerShellプロセスで実行し、次の設定に置き換える。

| プラグイン | `AURIS_AGENT_SMOKE_CONTROL` | 追加環境変数 |
| --- | --- | --- |
| DDSP Effect.vst3 | Output Gain | `AURIS_AGENT_SMOKE_EFFECT=1` |
| Surge XT.clap | Global Volume | なし |
| Surge XT Effects.clap | FX Type | `AURIS_AGENT_SMOKE_EFFECT=1`, `AURIS_AGENT_SMOKE_VALUE=0.1` |

`AURIS_AGENT_SMOKE_PLUGIN` には各ファイルの絶対パスを指定する。
必要なら `AURIS_AGENT_SMOKE_SETUP` に `[{"param":"exact.key","value":0.5}]` のJSON配列を渡し、
測定前の状態を同じエージェント用ツールで設定できる。

## 品質確認

- `cargo fmt --all -- --check`: 成功。
- `cargo test --workspace --no-fail-fast`: 4,697件成功、失敗0件、明示実行用など51件スキップ。
- `cargo clippy --workspace --all-targets -- -D warnings`: 成功。
- 上記とは別に、表の実プラグイン4ケースを明示実行して成功。

初回の全体テストでは、並行ビルドによるWindowsの実行ファイルロックで
セッションテストが起動できなかった。最終版の全体再実行は成功した。
