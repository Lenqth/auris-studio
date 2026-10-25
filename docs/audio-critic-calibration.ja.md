<a id="audio-critic-calibration"></a>
# 音声批評モデルの較正

[English](audio-critic-calibration.md)

`listen` は、単一抜粋の診断と匿名比較を分離します。診断の `focus` には説明的な質問を指定できます。比較には `compare_to` と、`balance`、`phrasing`、`ensemble`、`sound`、`musical_coherence` のいずれか1つの `aspect` が必要で、`focus` は拒否されます。批評モデルに届くのは評価基準とランダム順のA/B音声だけで、プロジェクトメタデータや編集内容の説明は渡しません。呼び出し元にはリクエスト後にA/B対応が返ります。両WAVは共通の保護減衰を保持します。

本番のレビューは短い自由記述です。応答はそのまま保持し、好み、確信度、タイムスタンプは抽出しません。送信に成功しても、モデルが音楽を認識したことは確定しません。Music Flamingoは、ペアになった抜粋を独立に記述します。較正コレクターはこれらの回答に`manual_review_required`を付けます。担当者が記述内容と両方の提示順を確認する必要があります。過去の構造化レポートは元の結果を保持しますが、新しい自由記述を判定として解析することはありません。

開発ワーカーは、次の本番経路をそのまま使います。

```powershell
cargo build -p auris-session --example critic_calibration
uv run tools/eval/audio_critic_calibration.py --worker target/debug/examples/critic_calibration.exe --output target/critic-calibration
```

設定した音声サーバーとモデルは事前に利用可能である必要があります。接続設定には `listen` と同じ `AURIS_AUDIO_*` 変数を使います。最初の送信スモークテストには `--limit 2` を使えますが、較正完了を意味しません。リクエストのネットワーク上限は本番と同じ5分です。

固定した合成コホートには短い調性フレーズ3つと、各フレーズ4種類の制御（同一音声、無音、ハードクリッピング、順序を崩したピッチ）があります。すべてのペアを両方の順序で実行し、判定は24件です。クリッピングとピッチの変種は基準のRMSに合わせ、リミッターや個別ピーク正規化で差を隠しません。WAVハッシュ、ワーカーハッシュ、生応答、配信失敗、結果を保存します。

同一ペアで好みが生じてはいけません。`cannot_judge`、無効な応答、転送失敗は別の結果であり、較正成功ではありません。障害制御の好みを解釈する前に、2つの提示順を比較してください。無音、歪み、不協和は意図的な音楽にもなり得るため、この合成コホートの好みは一般的な音楽判断を証明しません。判定だけでなく観察も確認してください。

[2026-10-24の試行](../tools/eval/references/audio-critic-calibration-2026-10-24.json)では、記録済みモデルダイジェストを使ったローカル `gemma4:e2b` を使用しました。24件すべてで音声を送信し、15件は `cannot_judge`、8件は引き分け、1件は必須フィールドを欠きました。障害に対する好みは示されませんでした。構造化された応答の送信が成功しても、このモデルが音楽編集を選ぶ用途に適するとはいえません。生応答と正確なWAVハッシュは基準レポートに保存され、スクリプトでWAVを再生成できます。

代表的な音楽で安定した識別と人間との一致が示されるまで、観察は助言にとどまります。コントローラーは批評モデルの好みだけで編集を自動選択してはいけません。実際の音楽的改善には、[composition-quality.md](composition-quality.ja.md) の盲検人間比較を使ってください。

```sh
uv run --with pytest pytest tools/eval/test_audio_critic_calibration.py
uv run --with ruff ruff check tools/eval/audio_critic_calibration.py tools/eval/test_audio_critic_calibration.py
```
