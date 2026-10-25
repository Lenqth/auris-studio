<a id="physical-model-experiments-against-real-recordings"></a>
# 実録音に対する物理モデル実験

[English](physical-extensions.md)

6つの因果放射共鳴、音域依存の接触・損失制御、非線形なバイオリン表情応答により、ホールドアウトした時間 log-mel 誤差はギターとバイオリンで約2.7%低下します。いずれも既に較正済みの楽器への段階的な変更です。基準コミットは `bb6b39502d502a38f85e7ab69ae5830ca2f0ef28` です。ピアノとペダルの試行結果も以下に示します。

<a id="objective-and-split"></a>
## 目的と分割

[短音](physical-copy-synthesis.ja.md) と [長音／軌跡](physical-trajectories.ja.md) のコホート、ハッシュ、分割は変更していません。Iowa は36個の原音（165ノート）、長音マニフェストは24個の TU-Note グリッサンドを含む212個の抜粋を提供します。

| モデル | 短音 学習／検証 | 長音 Iowa 学習／検証 | TU-Note 学習／検証 |
| --- | ---: | ---: | ---: |
| Piano | 6 / 12 | 12 / 24 | — |
| Guitar | 34 / 44 | 48 / 50 | — |
| Violin | 30 / 39 | 19 / 35 | 12 / 12 |

実際の Rust PCM は24 kHzで解析します。48 kHzの本番レートで別途レンダリングしたものは、解析前にポリフェーズでダウンサンプリングします。参照音と合成音には、それぞれ全音符の RMS を0.1にする正規化を1回だけ適用し、フレーム正規化とタイムワープは行いません。512／2048サンプルの Hann 窓、240サンプルのホップ、30 Hz〜10 kHzの64個の面積正規化 HTK メル帯域から `log(1 + 10000 * mel_power)` を得ます。主距離は平均絶対差です。短音のアタックとサステインには同じ重みを付けます。長音のアタック／初期／後期／遷移／リリースの重みは、空の位相を除いて0.15／0.15／0.40／0.20／0.10です。短音／長音／TU コホートの重みは0.3／0.5／0.2で、存在しないものがあれば再正規化します。ノートごとの位相、エンベロープ、ピッチ診断はすべて保持します。

バイオリン比較では録音されたピッチと表情のガイドを両側に同じ入力として与えます。これは条件付きコピー合成であり、奏者の弓の動きを自動再構成できることを示すものではありません。全音符の正規化は録音ゲインを除きますが、演奏された輪郭は残します。

係数は変更前のモデルを出発点として学習ノートだけで適合します。固定座標探索は、モードゲイン±1.5（範囲の0.15／0.06／0.02刻み、平均二乗ペナルティ0.001）、音域傾き±0.2／±0.8／±0.08（0.25／0.10／0.04刻み）、弓指数[0.5, 2]（0.20／0.08／0.03刻み）、自動スウェル量[0, 0.7]（0.25／0.10／0.04刻み）です。固定プロファイルを検証前に適用します。既存コホートは以前のモデル開発で使っているため、この最適化からはホールドアウトしていますが、未使用の最終テストセットではありません。検証結果は出荷判断にも使います。

対応のある95%ブートストラップ区間は2,000回再標本化し、シードは20261003、重複する抜粋は原録音ごとにクラスタ化します。多重比較補正をしない記述統計です。Iowa の多数のノートは、独立に録音された多数の楽器を意味しません。

<a id="instrument-changes"></a>
## 楽器の変更

120／240／480／960／1920／3840 Hz、Q=8の6つの平行な定ピーク帯域通過フィルターが、既存の広い音色を追従します。符号付きゲインで共鳴を加減し、寄与は Body Resonance に比例します（既定値0.65）。body=0では両バンクをバイパスします。フィルターは因果的で、楽器とともにリセットされます。ホストレートの0.45を超える周波数はバイパスします。これらの適合スペクトル事前分布には励振とマイクの色付けが含まれ、測定したブリッジアドミタンスや木製ボディの再構成ではありません。

音域には `x = clamp((pitch - center) / 24, -1.5, 1.5)` を使います。硬さとダンピングには加算傾きを、減衰には `exp(slope * x)` の乗算を適用し、既存の制御範囲内に収めます。ギターの中心は55、傾きは(-0.156, -0.624, 0.0624)、バイオリンの中心は69、傾きは(0, 0.224, 0)です。アタック、演奏中の損失変更、レガートの再ターゲットも同じ曲線を使います。バイオリンの CC11 は既存の弓／出力平滑化の前に `value^1.18` へ写像し、端点を保ちます。工場レベル、広域プロファイル、保存済み制御、記述ノートは変わりません。保存領域と係数はコールバック外で準備し、処理中に確保やロックは行いません。

<a id="held-out-results"></a>
## ホールドアウト結果

以下は本番 Rust PCM の48 kHz結果であり、代理予測だけではありません。

| モデル | 変更前 weighted mel L1 | 変更後 | 低下率 |
| --- | ---: | ---: | ---: |
| Piano | 0.156354 | 0.156354 | 0% |
| Guitar | 0.132132 | 0.128538 | 2.72% |
| Violin | 0.080924 | 0.078716 | 2.73% |

24 kHzではギターが0.132085から0.128505（2.71%）、バイオリンが0.078594から0.076219（3.02%）に改善します。バイオリン短音コホートは24 kHzで0.34%悪化し、48 kHzで0.22%改善します。どちらの短音区間も信頼区間は0を含みます。

| 48 kHzコホート | 低下率 | 改善／全数 | 変更後−変更前の平均の95%区間 |
| --- | ---: | ---: | ---: |
| Guitar short | 2.30% | 32 / 44 | [-0.004158, -0.002357] |
| Guitar long Iowa | 3.02% | 32 / 50 | [-0.007059, -0.001205] |
| Violin short | 0.22% | 19 / 39 | [-0.001872, 0.001469] |
| Violin long Iowa | 3.30% | 26 / 35 | [-0.005954, -0.001654] |
| Violin TU-Note | 6.87% | 12 / 12 | [-0.003868, -0.002533] |

長いギターのエンベロープ誤差は9.978→9.801 dB、長い Iowa バイオリンは1.960→1.923に改善しますが、TU-Note バイオリンは3.022→3.534に悪化します。スペクトルの改善は、フレージングや知覚品質の向上を保証しません。高調波構造の欠落を含め、実録音との差は依然として大きく残ります。

持続時間条件付きの弓試行は、両側に録音表情のない一般的な50 ms正弦スウェルガイドを使います。適合量0.175により、非重み付き検証メル誤差は0.095232から0.092425（2.95%）に低下し、86抜粋中65個が改善しました。エンベロープ誤差はわずかに悪化します。このアブレーションは出荷する CC11 応答とは別です。

ピアノの放射／音域プロトタイプは合計誤差を1.20%改善しますが、両コホートの区間は0を含み、短音は5/12、長音は11/24だけが改善し、長音エンベロープ誤差は9.517から10.158 dBへ悪化します。本番のピアノは変更しません。全アブレーションは [training](../tools/eval/references/physical-extension-training.json)、[24 kHz validation](../tools/eval/references/physical-extension-validation-24k.json)、[48 kHz validation](../tools/eval/references/physical-extension-validation-48k.json) にあります。

<a id="piano-pedal-trials-with-synchronized-real-midi"></a>
## 実 MIDI と同期したピアノペダル試行

[Saarland Music Data v2](https://www.audiolabs-erlangen.de/resources/MIR/SMD/midi) は、CC64を含む実ピアノ音声と同期を補正したMIDIを提供します。描画前に [v2 archive](https://zenodo.org/records/13753319) から3つの原ペアを選びます。Chopin Op.28/4を学習、Chopin Op.28/15と Beethoven Op.27/1 第3楽章を検証に使います。各抜粋は最初の12秒で、元のMIDIタイミングとペダル値を使い、アラインメント探索はしません。[cohort lock](../tools/eval/references/smd-pedal-captures.json) にハッシュと帰属を記録します。原音声はローカルの研究入力のままです。

開発例ではピアノ出力で駆動する88個のダンパー共鳴器を追加します。ペダル位置は15 msの平滑化で受動減衰半径を変えます。このフィードフォワード近似は双方向のブリッジ結合を解きません。ハーフペダル試行は既存リリースを `0.03 + 0.7 * pedal^2` に変更し、二値のキー保持ロジックを残します。完全なダンパー接触モデルではなく、リリース曲線の実験です。主距離は全抜粋の非重み付き log-mel L1です。

学習では共鳴ゲインを0/1/2/4/8/16から選び、上限の16が勝ちました。検証メル誤差は Chopin で0.88%、Beethovenで0.55%改善しましたが、エンベロープ誤差はいずれも改善しません。ハーフペダルは Chopin ではほぼ中立ですが Beethoven では0.81%悪化し、エンベロープ誤差は4.181→5.929 dBです。2曲と上限の最適値だけでは、この近似を採用する根拠として不十分です。全バリエーションは [report](../tools/eval/references/physical-pedal-validation.json) に残しています。独立したより大きな MIDI／音声コホートと、より完全なダンパー／ブリッジモデルがあれば、強い検証ができます。

<a id="reproduction-and-listening"></a>
## 再現と試聴

[physical-trajectories.md](physical-trajectories.ja.md) と同じ方法で固定参照を準備します。上記コミットの別チェックアウトで基準 `physical_fit_render` をビルドし、実行ファイルを保持します。以下のコマンドでは基準を `target/physical-experiments/before-renderer.exe` とします。Windows以外では `.exe` を省きます。実行ファイルのハッシュは使用したバイナリを識別し、コンパイラのビルドが違えば異なることがあります。基準を再ビルドした場合は新しい学習ファイルを適合して検証に使い、係数と損失を保存結果と比較します。

```powershell
cargo build --release -p auris-synth --example physical_fit_render --example physical_pedal_render
uv run tools/eval/fit_physical_extensions.py target/physical-experiments/references `
  target/physical-experiments/iowa target/physical-experiments/before-renderer.exe `
  target/physical-experiments/new-training.json --fit
uv run tools/eval/fit_physical_extensions.py target/physical-experiments/references `
  target/physical-experiments/iowa target/physical-experiments/before-renderer.exe `
  target/physical-experiments/new-validation.json `
  --profiles target/physical-experiments/new-training.json --rate 48000 `
  --candidate target/release/examples/physical_fit_render.exe `
  --audition target/physical-experiments/audition
uv run tools/eval/fetch_piano_pedal_reference.py target/physical-experiments/smd `
  --lock tools/eval/references/smd-pedal-captures.json
uv run tools/eval/evaluate_piano_pedal.py target/physical-experiments/smd `
  target/release/examples/physical_pedal_render.exe target/physical-experiments/new-pedal.json
```

評価は変更された学習レンダラーまたは参照ハッシュを拒否し、結果の上書きも拒否します。実験用代理も、これらの拡張曲線を適用する前に保存したワーカーを必要とします。最終ワーカーは `--candidate` のときだけ使い、二重適用を防ぎます。試聴ではメタデータから mf の3秒検証ノート（ピアノ C4、ギター G3、バイオリン G4付近）を選びます。各WAVは **録音、変更前、変更後** の順に250 ms間隔で再生します。全音符 RMS の一致と共通ピーク保護でレベルをそろえます。5 msフェードは試聴用コピーだけに適用します。PNGは共通の log-mel 色スケールを使います。これらは改善値で選んだ例ではありません。

```powershell
$env:PHYSICAL_RENDERER = Join-Path $PWD 'target/release/examples/physical_fit_render.exe'
uv run --with numpy==2.5.3 --with scipy==1.18.1 --with soundfile==0.14.0 --with pytest `
  pytest tools/eval/test_physical_extensions.py tools/eval/test_physical_mel.py `
  tools/eval/test_physical_trajectory.py tools/eval/test_fit_physical_body.py tools/eval/test_physical_ab.py
```

統合テストは、表情の移動、リリース、複数音域を含め、両レートの実PCMと因果代理を比較します。最大サンプル誤差は0.0002未満でなければなりません。本番メル損失は拡張予測と4e-9以内で一致します。Rustテストでは、コールバックの確保、持続安定性、ピッチ、減衰、レガート、ボディリセットを確認します。[system measurements](../tools/eval/references/physical-extension-system-evaluation.json) には、変更前後の同じプリセットスコア、記号ベースライン、コールバックベンチマークを記録します。

CRLF／LFを正規化すると記号測定値は完全一致します。Audiobox の9プリセット平均は CE 6.395→6.413、CU 7.518→7.525、PC 4.599→4.597、PQ 7.786→7.792に変化します。これらの単一シードの曲診断はほぼ横ばいで、主な測定上の利得は参照スペクトルの類似度です。48 kHz、24ボイス、256フレームブロックでは、平均コールバック時間はギター0.081→0.084 ms、バイオリン0.074→0.077 msに変化します。いずれも統計的に管理されたCPUベンチマークではなく、制御されていない開発ホスト負荷下での単一実行です。
