<a id="composing-from-an-idea"></a>
# アイデアから作曲する

[English](composition-intent.md)

| サウンド | 音階 | 自動ムードの例 |
| --- | --- | --- |
| Bright | メジャー | Bright |
| Subdued | ナチュラルマイナー | Dark |
| Dark with warmth | ドリアン | Epic |
| Floating | リディアン | Dreamy |
| Bright and mellow | ミクソリディアン | Funky |
| Tense and dark | フリジアン | Tense |

```toml
style = "pop-band"
mood = "dark"
tonality = "minor"
pace = "slow"
```

`tonality` は `auto`、`major`、`minor` を受け付けます。`pace` は `auto`、`slow`、`moderate`、`fast`（それぞれ80、120、160 BPM）を受け付けます。ムードだけを指定して省略した場合は `auto` になります。`sound` は `auto`、`major`、`minor`、`dorian`、`lydian`、`mixolydian`、`phrygian` を受け付けます。たとえば `mood = "dark"` と `sound = "dorian"` を指定すると、暗い演奏設定を保ちながらドリアンのハーモニーを使います。完全なリクエストは `examples/dorian-song.asong` を参照してください。明示的な `sound` は `tonality` より優先され、`tonality` はムードによる音階選択より優先されます。明示的な `key` または `scale` はその両方より優先され、明示的な `tempo` は `pace` より優先されます。`chords` では必要に応じて進行を明示できます。

<a id="auditioning-chords-before-composing"></a>
## 作曲前にコードを試聴する

同じレンダラーでヘッドレスWAVを作るには、次を実行します。

```sh
cargo run -p auris-session --example chord_preview -- examples/dorian-song.asong verse target/chords.wav
```

<a id="how-harmony-is-generated"></a>
## ハーモニーの生成方法

`auris-compose/src/progression.rs` の生成器は3段階で動作します。

1. セクションを最大4小節の均衡したフレーズに分割します。
2. 音階度数の完全なフレーズを選び、選択した音階の3度を積み重ねてコードを作ります。メジャーとマイナーには調性フレーズがあります。4つのモーダルサウンドは、各モードで2つの度数だけを選ぶ6種類のフレーズ形を共有します。トニックを軸にし、ドリアンではIV、リディアンとフリジアンではII、ミクソリディアンではVIIという特徴的なコードを訪れます。冒頭フレーズは隔フレーズで繰り返し、その間のフレーズを変化させます。
3. より動きのある調性フレーズでは、小節内でメジャーならii、マイナーならivを使ってVを準備できます。

フレームプランナーはコードカラーを加え、セクションに到着感が必要か判断します。モーダルハーモニーは選択した音階内に拡張音を収め、特徴的なコードでトニックへ向かいます。調性ドミナントを挿入せず、モードを保ちます。キー変更のために明示したドミナントの導入はそのまま尊重されます。調性マイナーではメジャードミナントを使えます。すべての楽器が完成したプランを読み取ります。短いセクションでも各フレーズの開始と到達先を保持し、2小節の場合もモーダルフレーズの特徴的なコードを残します。奇数長のセクションも全体を覆います。シードはビルド内で再現可能なテイクを選びます。
