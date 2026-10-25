# 強化前後の作曲比較・試聴結果

同じ仕様で作ったインストと歌ものを、A/Bを匿名化して聴き比べるための記録です。音源・編集プロジェクト・検査証跡は `C:/Users/uthree/Music/Auris Studio/Composition A-B 2026-10-25` に保存し、以下の `private/` はその配布フォルダ内を指します。

- 実施試行: 24 / 24
- 生成・納品成功（正式ready）: 23 / 24
- 参考を含む音源数: 24 / 24
- 試聴ペア: 11 / 12
- 正式比較可能ペア: 11 / 12
- 1ケース2反復の探索的比較。LLMの乱数seedは固定していません。

## ペア一覧（匿名A/B）

| ペア | ジャンル | 種別 | BPM | 拍子 | A status | B status |
|---|---|---|---:|---|---|---|
| pair-001 | アコースティック／フォーク | インスト | 96 | 6/8 | ready | ready |
| pair-002 | シンセポップ | 歌もの | 120 | 4/4 | ready | ready |
| pair-003 | アコースティック／フォーク | インスト | 96 | 6/8 | ready | ready |
| pair-004 | ポップバンド | インスト | 112 | 4/4 | ready | ready |
| pair-005 | ポップバンド | 歌もの | 112 | 4/4 | ready | ready |
| pair-006 | シンセポップ | インスト | 120 | 4/4 | ready | ready |
| pair-007 | ポップバンド | インスト | 112 | 4/4 | ready | ready |
| pair-008 | アコースティック／フォーク | 歌もの | 96 | 6/8 | ready | delivery_failed |
| pair-009 | シンセポップ | インスト | 120 | 4/4 | ready | ready |
| pair-010 | ポップバンド | 歌もの | 112 | 4/4 | ready | ready |
| pair-011 | シンセポップ | 歌もの | 120 | 4/4 | ready | ready |
| pair-012 | アコースティック／フォーク | 歌もの | 96 | 6/8 | ready | ready |

## 実験設定

- モデル/backend: `gpt-6.1-sol` / `codex`（全suite同一）
- prompt profile: `legacy`、candidate tools: `False`
- 編曲ハーネス: off。各試行は作曲と納品の2ターンで、1ターンあたりの上限は900秒。LLM seedは固定していません。
- runner記録のcontext_tokens: `65536`（実効context固定ではありません）

## 音声設定

- VOICEVOX: `0.26.0` / `ずんだもん / ノーマル` / seed `0`
- 各auditのvoice config hash一致: `True`。`voice-config.json` は記録用コピーです。

## 固定したケース

| ジャンル | 種別 | BPM | 拍子 | キー | 楽器構成 |
|---|---:|---:|---:|---|---|
| ポップバンド | インスト | 112 | 4/4 | C major | Lead piano / Bass / Drums / Pad |
| ポップバンド | 歌もの | 112 | 4/4 | C major | Vocal / Piano / Bass / Drums |
| アコースティック／フォーク | インスト | 96 | 6/8 | G major | Tin whistle / Piano / Strings / Drums |
| アコースティック／フォーク | 歌もの | 96 | 6/8 | G major | Vocal / Guitar / Piano / Strings / Drums |
| シンセポップ | インスト | 120 | 4/4 | E minor | Lead chiptune / Bass chiptune / Drums / Pad |
| シンセポップ | 歌もの | 120 | 4/4 | E minor | Vocal / Hook / Bass / Drums / Pad |

## 24試行の自動検査

| ペア | ラベル | 状態 | 正式ready | 参考音源 | 失敗理由 | 音数 | 音域(semitone) | 休符候補数 | 最大休符(beat) | IOI一致率 |
|---|---|---|---:|---:|---|---:|---:|---:|---:|---:|
| pair-001 | A | ready | True | True |  | 58 | 13 | 18 | 6.65 | 0.143 |
| pair-001 | B | ready | True | True |  | 59 | 14 | 11 | 6.562 | 0.386 |
| pair-002 | A | ready | True | True |  | 81 | 15 | 22 | 1 | 0.367 |
| pair-002 | B | ready | True | True |  | 81 | 13 | 10 | 1.5 | 0.557 |
| pair-003 | A | ready | True | True |  | 59 | 14 | 9 | 7 | 0.474 |
| pair-003 | B | ready | True | True |  | 53 | 13 | 11 | 7.5 | 0.392 |
| pair-004 | A | ready | True | True |  | 85 | 17 | 55 | 1.7 | 0.096 |
| pair-004 | B | ready | True | True |  | 69 | 15 | 36 | 2.25 | 0.209 |
| pair-005 | A | ready | True | True |  | 81 | 16 | 9 | 1.25 | 0.418 |
| pair-005 | B | ready | True | True |  | 81 | 16 | 8 | 1 | 0.443 |
| pair-006 | A | ready | True | True |  | 56 | 5 | 55 | 5 | 0 |
| pair-006 | B | ready | True | True |  | 46 | 7 | 22 | 8 | 0 |
| pair-007 | A | ready | True | True |  | 71 | 15 | 34 | 2 | 0.304 |
| pair-007 | B | ready | True | True |  | 90 | 15 | 24 | 1.6 | 0.239 |
| pair-008 | A | ready | True | True |  | 87 | 14 | 7 | 0.75 | 0.459 |
| pair-008 | B | delivery_failed | False | True | Real-model turn timed out | 87 | 10 | 7 | 0.625 | 0.235 |
| pair-009 | A | ready | True | True |  | 50 | 7 | 29 | 2.5 | 0.083 |
| pair-009 | B | ready | True | True |  | 67 | 7 | 29 | 9 | 0.046 |
| pair-010 | A | ready | True | True |  | 81 | 14 | 18 | 1.25 | 0.481 |
| pair-010 | B | ready | True | True |  | 81 | 14 | 12 | 1.5 | 0.456 |
| pair-011 | A | ready | True | True |  | 81 | 15 | 8 | 1.5 | 0.506 |
| pair-011 | B | ready | True | True |  | 81 | 13 | 12 | 1.5 | 0.481 |
| pair-012 | A | ready | True | True |  | 87 | 14 | 10 | 0.75 | 0.4 |
| pair-012 | B | ready | True | True |  | 87 | 10 | 13 | 0.5 | 0.318 |

これらは保存されたLeadまたはVocal譜面の音数、音域、休符候補などの検査値です。発音、息継ぎ、覚えやすさ、編曲の好みを採点するものではありません。

参考音源は、監査と3本の音源が揃っていても、transportまたは納品に失敗した試行を含みます。途中生成物は正式readyや比較成功を回復しません。失敗理由は匿名A/Bごとに表示しています。

## 実行条件と由来

- 強化前のソース: `3aa05ca7`
- 強化後のソース: `1708a2f9`
- Codexに要求したコンテキスト長: 65536 tokens（実効値はこの記録だけでは保証しません）
- コンテキスト条件: runner は context_tokens=65536 を記録しますが、Codex backend へ model context-window override は送信しません。両条件は backend-default reasoning effort を使用します。
- 比較可能ペア: 11
- 追加フィードバック変更: Gate のフィードバックは収集中に別の target/feedback build でソースとインストール済みスキルへ反映しました。比較対象は元の凍結 executable・DLL・prompt-condition files を使用しており、後続 checkout のメタデータはバイナリの build provenance ではありません。
- 凍結ビルドのsource commit: `1708a2f9af99f9b7c693074cb5d9a6e7f899d7cd`
- 凍結ビルドのsource diff SHA-256: `033f0a24b53fe1f533dd9ed1544d7c5120272fdbdcf9d01ede7c09cf8213c523`（完全な差分は `private/frozen-runtime-provenance.json` に保存）
- 同一条件の2反復で、LLMの乱数seedは固定していません。

## 試聴

[匿名A/B試聴ページ](<C:/Users/uthree/Music/Auris Studio/Composition A-B 2026-10-25/listening/listening.html>) で、ミックス・主旋律または歌声・伴奏を切り替えられます。

A/Bの条件割り当てはペアごとに異なります。先行試聴の6組は、最終版とのラベルと全聴取範囲のWAVハッシュ一致を確認しました。対応表は `private/preview-integrity.json`、強化前後の答え合わせは `private/condition-mapping.json` にあります。

比較対象は24小節のデモです。和声進行、旋律、伴奏、音色設定と内蔵エフェクトは生成に任せているため、指示パッケージ全体の比較であり、個別の規則や旋律だけの因果効果を切り分ける実験ではありません。

保存プロジェクトには実際の歌唱テイクを含め、生成時のミキサー設定を保っています。保護ゲインとペア間の音量合わせは試聴WAVへの別処理なので、プロジェクトをそのまま再レンダーした音量とは異なります。処理前のfloat WAVとゲインの検査記録も保存しています。主旋律・伴奏の分離音源はそれぞれmaster処理を通る参考音源であり、足し合わせによる完全再構成は保証しません。

編集用プロジェクトは1曲1フォルダで内部のAudioを保持しています。音声設定やSoundFontの外部参照は元のパスを残した同じPC向けのコピーで、外部依存先と存在確認はprivate/package-audit.jsonに記録しています。

## 判定の扱い

この環境ではエージェント自身が音源を直接聴いていないため、自然さ、覚えやすさ、息継ぎ、編曲の優劣は判定していません。譜面統計はLeadまたはVocalの音域、音数、休符候補などを記録するだけで、値が大きいほど良いという意味ではありません。

## 人によるメモ

感想は試聴したペアと聴取要素に限定して記録します。フォーク歌ものの旋律・歌と伴奏の好みは分けたまま保持し、総合評価へ置き換えません。自然さ・覚えやすさ・息継ぎの明示的な比較評価はまだ得ていないため、今回の結果からこれらの改善を認定しません。

- pair-007: 大きな差はないけど、Bのほうが好みかな...
- pair-005: 歌ものも聞いてみた。歌ものは明確にBのほうが好きかなって感じだね。主旋律のリズム感に優れているように思う。
- pair-012: メロディ・歌はAのほうが好き。伴奏はBのほうが好き。

編集可能なプロジェクト: `C:\Users\uthree\Music\Auris Studio\Composition A-B 2026-10-25\private\projects`
