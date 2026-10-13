# Changelog / 変更履歴

## 0.1.1 — 2026-10-13 JST

Documentation only: English/Japanese READMEs and integration guides, a bilingual
rustdoc landing page with complete examples, separated validation history,
publication instructions, and Cargo documentation/package-inclusion metadata.
No runtime implementation, public API, dependency versions, or license changes.
Prepared for the first crates.io release and matching GitHub source release;
the existing `v0.1.0` tag is unchanged.

ドキュメントのみ：日英README・利用ガイド、完全な例を含む日英併記のrustdocトップ、
独立した検証履歴、公開手順、Cargoのドキュメント・同梱設定を整備しました。
実行時実装、公開API、依存バージョン、ライセンスは変更しません。
初回のcrates.io公開と対応するGitHubソースリリースに向けて整備しました。
既存の`v0.1.0`タグは変更しません。

## 0.1.0 — 2026-10-13 JST (2026-10-12 UTC)

[GitHub source release](https://github.com/matsuvr/bevy-looking-glass-rs/releases/tag/v0.1.0),
not a crates.io release. Bevy 0.20 / Rust 1.99+.

Initial independent plugin: Bridge HTTP discovery, calibrated off-axis view
arrays and direct interlacing, selected-monitor placement with deferred
fullscreen, live framing/depth, optional opaque-depth reprojection, per-view
Bloom/DoF integration, external Quilt/array input, and PNG/sequence capture.
Includes the device specification catalog and HLD output.

MIT provenance and third-party notices accompany the source archive. See
[validation](docs/validation.md) for dated results and remaining limitations.

[GitHubのソースリリース](https://github.com/matsuvr/bevy-looking-glass-rs/releases/tag/v0.1.0)であり、
crates.ioリリースではありません。Bevy 0.20・Rust 1.99以上向けです。

独立プラグインの初版です。Bridge HTTP検出、校正済みoff-axis視点配列と直接インターレース、
作成後の全画面化による選択モニター配置、撮影範囲・奥行きのライブ調整、
任意の不透明深度再投影、視点別Bloom/DoF、外部Quilt・配列入力、PNG・連番撮影を含みます。
機器仕様カタログとHLD出力も含みます。

ソース配布物にMITの由来と第三者告知を同梱しています。
日付ごとの結果と残る制限は[検証記録](docs/validation.md)を参照してください。
