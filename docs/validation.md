# Validation record / 検証記録

[English README](../README.md) · [日本語README](../README.ja.md)

These are preserved, dated reports from the original README and linked pull
requests, not tests rerun by the documentation update. A later documentation edit
does not renew a hardware or performance claim. Read the environment and scope
alongside each result.

元のREADMEとリンク先PRにある、日付付きの検証報告を整理したものです。
ドキュメント更新時に再実行した検証ではありません。
文章を後日更新しても実機・性能の確認日が新しくなるわけではありません。
結果とともに環境・範囲を確認してください。

## Current evidence summary / 現時点の根拠の要約

| Environment / 環境 | Recorded evidence / 記録されている確認 | Not established / 確立されていない範囲 |
| --- | --- | --- |
| Windows, Rust 1.99.0, MSVC | 40 library tests, 3 example builds, Clippy, packaged compilation / テスト40件、example 3件、Clippy、配布物コンパイル | Physical-panel placement in that review / 当該レビューでの実パネル配置 |
| Windows, RTX 4090, Vulkan | Synthetic-calibration rendering, generated/HDR captures, exact external-input readbacks / 合成校正での描画、生成・HDR撮影、外部入力の画素一致 | Optical panel quality or measured speedup / 実パネルの光学品質・高速化率 |
| macOS, Apple M4, Portrait | PR #2 reports native placement, restart/fullscreen transitions, visible avatar / PR #2でネイティブ配置、再起動・全画面遷移、アバター表示を報告 | Full optical-quality assessment, all effects/devices / 光学品質の全面評価、全効果・全機種 |
| Other devices and Linux / その他の機種・Linux | No runtime evidence recorded here / この記録には実行確認なし | Compatibility is not inferred from catalog entries / カタログから動作確認を推定しない |

Camera/NDI concurrency, broad GPU comparisons, and reprojection performance gains
are not established by these records. Single-panel visibility is not proof of
correct optics on every model. Device presets document specifications only.

カメラ・NDIとの同時動作、幅広いGPU比較、再投影による性能向上は、これらの記録では確立されていません。
一つの実機で表示できたことは、全モデルの光学的な正しさを示しません。
デバイスプリセットが示すのは仕様です。

## Provenance / 抽出元

The initial implementation was extracted from
[RusTuberV a198c1b9](https://github.com/matsuvr/RusTuberV/tree/a198c1b9) under MIT,
preserving its array/copy/interlace path. The old roughly-60-FPS RusTuberV report
concerned a particular Windows workload with synthetic calibration, not a plugin
benchmark or panel-optics measurement. See the
[consumer integration record](https://github.com/matsuvr/RusTuberV/blob/main/docs/looking-glass.md).

初期実装はMITの[RusTuberV a198c1b9](https://github.com/matsuvr/RusTuberV/tree/a198c1b9)から、
配列・コピー・インターレース経路を維持して抽出しました。
過去の約60fpsという報告は、Windowsの特定負荷・合成校正でのRusTuberVの測定です。
独立プラグインのベンチマークや実パネルの光学測定ではありません。
[利用側の統合記録](https://github.com/matsuvr/RusTuberV/blob/main/docs/looking-glass.md)も参照してください。

## 2026-10-11 — Extraction / 独立crate化

**English.** Windows / RTX 4090 / Vulkan: 21 library tests and all-target Clippy
passed outside the RusTuberV workspace with registry, unpatched `bevy_window`.
A separate minimal consumer used synthetic calibration to check Quilt display,
camera movement, interlacing, black output for `pose: None`, and stop/exit.
Screenshots were inspected; camera movement changed Quilt pixels. Bevy logged
late events for the closed window; the consumer exited with code 0.
`cargo publish --dry-run` compiled the packaged crate with normal dependencies;
WESL and device JSON were present. Physical optics, macOS execution, and
cross-device performance were unverified at that stage.

**日本語。** Windows・RTX 4090・Vulkanで、RusTuberVワークスペース外の未改変registry版
`bevy_window`を用い、テスト21件とall-target Clippyが成功しました。
別の最小利用アプリで合成校正を使い、Quilt、カメラ移動、インターレース、
`pose: None`時の黒表示、停止・終了を確認しました。スクリーンショットを確認し、
カメラ移動によるQuilt画素の変化も確認しました。終了済みウィンドウへの遅延イベントのログはありましたが、
終了コードは0でした。`cargo publish --dry-run`は通常依存で配布物をコンパイルし、
WESL・機器JSONの同梱を確認しました。この時点では実光学、macOS実行、機種間性能は未確認でした。

## 2026-10-12 — Bridge discovery / Bridge検出

**English.** All 25 library tests and all-target Clippy with warnings denied
passed. Tests covered unavailable Bridge, orchestration cleanup after parse
errors, multiple serials, and preservation of raw optical cells/unknown model
identifiers. The relocated HTTP module retrieved a connected Portrait's calibration
and 3360-square, 8-by-6 default Quilt. Its source passed an isolated
`aarch64-apple-darwin` check. A full Bevy cross-check from Windows stopped at
`blake3` because a Mac C compiler was unavailable. This was not a macOS runtime test.

**日本語。** テスト25件と警告をエラー扱いにしたall-target Clippyが成功しました。
Bridge不在、解析失敗後のorchestration終了、複数シリアル、生の光学セル・未知モデル識別子の保持を検査しました。
移動したHTTPモジュールで、接続したPortraitの校正と3360四方・8列6行の推奨Quiltを取得しました。
同ソースは単独の`aarch64-apple-darwin`チェックを通過しましたが、WindowsからのBevy全体の
クロスチェックはMacのCコンパイラーがなく`blake3`で停止しました。macOSでの実行テストではありません。

## 2026-10-12 — Rendering features / 描画機能追加

**English.** 36 library tests, all-target Clippy with warnings denied, formatting,
whitespace checks, and `cargo publish --dry-run` passed. Windows / RTX 4090 /
NVIDIA 617.14 / Vulkan produced four PNG frames for full rendering, three-anchor
generation, HDR Bloom/DoF, and combined HDR/generation. All three anchors retained
custom projections and received Bevy DoF uniforms. The combined sequence produced
four distinct files; their SHA256 values matched another run at the same frame times.

External Quilt and array consumers with five active views in a 3-by-2 grid at
11-by-9 pixels passed exact RGBA readback comparisons, including top/right padding
and an inactive tile, without a scene pose. Frame/time labels were retained.
Generated/HDR PNGs were inspected. This used synthetic calibration and desktop
windows, not physical optics, connected-panel placement, macOS, or speedup testing.
Reprojection's disocclusion and transparency limitations still apply.

**日本語。** テスト36件、警告をエラー扱いにしたall-target Clippy、整形・空白検査、
`cargo publish --dry-run`が成功しました。Windows・RTX 4090・NVIDIA 617.14・Vulkanで、
全視点描画、3基準視点からの生成、HDR Bloom/DoF、HDRと生成の併用についてPNGを各4枚出力しました。
3基準視点すべてでカスタム投影を維持したままBevyのDoF uniformを確認しました。
併用モードの4枚はそれぞれ異なり、同じ時刻で再実行したファイルとSHA256が一致しました。

外部Quilt・配列入力では、11×9ピクセル・3列2行・有効5視点のケースについて、
姿勢なしで上端・右端の余白と無効枠を含むRGBA読み戻しの完全一致を確認しました。
frame/timeラベルも保持されました。生成・HDRのPNGを目視確認しましたが、
合成校正と通常ウィンドウでの検証であり、実光学、実パネル配置、macOS、速度向上の検証ではありません。
再投影の遮蔽解除・透過の制限は残ります。

## PR #2 — macOS Portrait placement / macOSでのPortrait配置

Source: [PR #2](https://github.com/matsuvr/bevy-looking-glass-rs/pull/2).

**English.** An Apple M4 with a 2x Retina primary display and a 1x Portrait
reproduced initial fullscreen on the wrong monitor. The fix waits for native
window creation before requesting fullscreen. The PR reports 40 regression tests
with both registry Bevy and the consumer's existing window-lifetime patch.
On the physical Mac, Winit monitor selection, fullscreen state, position `(1470, 0)`,
and the 1536-by-2048 framebuffer were checked after start, stop/fresh discovery/restart,
and windowed/fullscreen transitions. The primary window retained its geometry;
the probe exited 0. The owner confirmed a local VRM avatar visible on the panel.
Windows runtime was not retested there because the Windows host was offline.
Optical avatar quality and camera/NDI concurrency were outside that placement check.

**日本語。** Apple M4で2倍Retinaのメイン画面と1倍のPortraitを使用し、
最初の全画面表示が誤ったモニターに出る問題を再現しました。
修正ではネイティブウィンドウ作成後に全画面化します。
PRにはregistry版Bevyと利用側の既存window-lifetimeパッチの両方で回帰テスト40件成功の報告があります。
実Macで、開始、停止・再検出・再開、ウィンドウ・全画面の切り替え後について、
Winitのモニター選択、全画面状態、位置`(1470, 0)`、1536×2048のフレームバッファを確認しました。
メイン画面の形状は維持され、プローブの終了コードは0でした。
所有者がローカルVRMアバターの実パネル表示を確認しています。
Windows端末はオフラインだったため、そのPRではWindows実行を再検証していません。
アバターの光学品質とカメラ・NDI同時動作は配置検証の対象外です。

## 2026-10-13 — Windows review and source packaging / Windowsレビューと配布物

Sources: [PR #2 follow-up](https://github.com/matsuvr/bevy-looking-glass-rs/pull/2),
[PR #3](https://github.com/matsuvr/bevy-looking-glass-rs/pull/3),
[v0.1.0 source release](https://github.com/matsuvr/bevy-looking-glass-rs/releases/tag/v0.1.0).
The source release timestamp is October 12 UTC / October 13 JST.

**English.** Rust 1.99.0 / `x86_64-pc-windows-msvc`: 40 library tests, three example
builds, locked all-target Clippy with warnings denied, formatting, and whitespace
checks passed. Regressions included deferred window creation, monitor removal,
stopped output, late/duplicate events, and primary-window isolation.
`cargo package --locked` compiled the 30-file source archive with normal
dependencies; licenses, notices, WESL, and the device catalog were included.
`cargo-deny 0.20.2` accepted the locked all-feature dependency licenses without
exceptions. This is a record for that lockfile, not future resolutions or a patent
assessment. Physical output, concurrency, and performance were not rerun.

**日本語。** Rust 1.99.0・`x86_64-pc-windows-msvc`で、テスト40件、example 3件のビルド、
lockedかつ警告をエラー扱いにしたall-target Clippy、整形・空白検査が成功しました。
遅延ウィンドウ作成、モニター取り外し、停止済み出力、遅延・重複イベント、メイン画面との分離を検査しました。
`cargo package --locked`は30ファイルのソース配布物を通常依存でコンパイルし、
ライセンス、告知、WESL、機器カタログの同梱を確認しました。
`cargo-deny 0.20.2`はlocked・全featureの依存ライセンスを例外指定なしで受け入れました。
このlockfileの記録であり、将来の依存解決や特許評価ではありません。
実機出力、同時動作、性能は再検証していません。

## Adding new records / 記録を追加するとき

Record the commit, date, commands, environment, and what was observed. Distinguish
build/tests, desktop pixel inspection, and physical optics. Do not carry a previous
success forward as if it ran again. A small reproducible check is useful; an
all-device certification process is not required.

コミット、日付、コマンド、環境、観察した内容を記録します。
ビルド・テスト、通常画面での画素確認、実パネルの光学確認を区別し、
過去の成功を新しい実施結果として扱わないでください。
小さく再現できる確認で役立ちます。全機種認証のような工程は求めません。
