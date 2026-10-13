# bevy_looking_glass

[English](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/README.md) | 日本語

Bevyの3DシーンをLooking Glassへ表示する、**非公式・実験的なプラグイン**です。
小さく直接的な実装を目指す個人の研究プロジェクトであり、サポートは約束しません。
公式Looking Glass SDKではなく、全機種での動作を保証するものでもありません。

利用にはお使いのPC/MacにLooking Glass Bridgeがインストールされている必要があります。

このcrateのCPU側実装はsafe Rust、GPUシェーダーは埋め込みWESLです。
Looking Glass Bridge SDK、C/C++バインディング、JavaScriptをリンク・再配布しません。
任意の機器検出機能では、**別途インストールされたLooking Glass Bridge**の
ローカルHTTP APIを利用します。校正情報の取得後はBevyから直接描画し、
通常のライブ出力はGPU内で完結します。要求されたPNGキャプチャーに限って
非同期読み戻しとRustによるPNGエンコードを行います。

## できること

- off-axis投影による多視点描画、Texture Arrayからのインターレース、Quilt一覧プレビュー、
  撮影範囲・奥行きのライブ調整、選択モニターへの全画面配置。
- 任意の深度ベース視点生成、視点ごとのBevy Bloom/DoF連携、外部Quilt・Texture Array入力、
  要求時のみのQuilt PNG・連番キャプチャー。
- ハードウェア仕様カタログと単一視点のHLD出力。カタログはプリセットであり、
  **実機検証済み機種の一覧ではありません**。

一つのBevyアプリでこのプラグインが管理する出力は一つです。最終出力はSDRで、MSAAは無効です。
自動画質フォールバック、動画コーデック、視点をまたぐGPU Instancing、複数台への同時出力、
Unityのエディターツールは含みません。深度生成は不透明シーン向けに明示的に選ぶモードです。
透過や視点依存効果を正しく描く必要があるシーンでは、全視点の実描画を使用します。

## 対応バージョンと準備

| 項目 | 範囲 |
| --- | --- |
| crateのAPI | `0.1.x`。破壊的なAPI変更ではマイナーバージョンを上げます |
| Bevy | `0.20` |
| Rust | `1.99`以上。リポジトリのツールチェーン指定はstableです |
| 主な想定環境 | Windows・macOS。実際の検証範囲は後述します |
| ライトフィールド出力 | 接続したパネル、その個体の実校正情報、ネイティブピクセルでの映像出力 |
| 自動検出 | インストール・起動済みBridge、映像接続とUSBデータ接続 |

Bevyのウィンドウバックエンドと、シーン・効果に必要なfeatureは利用アプリが用意します。
最初はデフォルトfeatureを有効にした`bevy = "0.20"`が簡単です。
ライブラリ自体はBevyの全デフォルトfeatureを有効にしません。
選択したBevyのfeatureに応じた通常のビルド環境も必要です。

光学的に正しく表示するには、フレームバッファとパネルの物理ピクセルが1対1で対応する必要があります。
正しい解像度・向きの拡張デスクトップを使用してください。
縮小したウィンドウはプレビューであり、校正された光学出力ではありません。
利用アプリが校正データを直接渡す場合、描画にBridgeは不要です。

## 導入

crates.ioのcrateを追加します。

```toml
[dependencies]
bevy = "0.20"
bevy_looking_glass = "0.1.1"
```

対応する[GitHubソースリリース](https://github.com/matsuvr/bevy-looking-glass-rs/releases/tag/v0.1.1)
をGit依存で使うこともできます。

```toml
bevy_looking_glass = { git = "https://github.com/matsuvr/bevy-looking-glass-rs", tag = "v0.1.1" }
```

## 最初の実行

Bridgeを起動し、校正情報を取得できるパネルを接続して、リポジトリ内のモデル不要のexampleを実行します。
このexampleは最初に検出した機器を選択します。利用アプリではシリアル番号による選択も可能です。

```sh
git clone https://github.com/matsuvr/bevy-looking-glass-rs.git
cd bevy-looking-glass-rs
cargo run --example minimal -- --bridge
```

その個体の`visual.json`を直接渡すこともできます。

```sh
cargo run --example minimal -- /path/to/visual.json
```

ファイル指定では小さなプレビューウィンドウで開始します。
光学出力として使う際は正しいパネルへ移動し、全画面化してください。
`--bridge`指定ではネイティブ配置を要求します。
手前に立方体、焦点面上に球、奥に立方体が表示されます。
矢印キーで回転・前後移動、PageUp/PageDownで奥行きを調整します。
出力ウィンドウにフォーカスがあるとき、F10でQuilt表示、F11で全画面表示、Escapeで出力停止です。
メインウィンドウを閉じるとexampleの出力も閉じます。
これらは**example側の操作方針**であり、プラグインがアプリに強制するキー操作や終了処理ではありません。

自分のアプリへの組み込みは、
[完全な導入例とAPI概要 / Complete cube example and API overview](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/docs/api.md)
を参照してください。空のレンダラー設定だけでなく、シーンとライトも含めています。
同じページをrustdocのcrateトップとして使用し、`cargo test --doc`では
ウィンドウを開いたりBridgeへ接続したりせず、`no_run`の例をコンパイル検査できます。

## ドキュメント

| 内容 | English | 日本語 |
| --- | --- | --- |
| 組み込み、実行順序、効果、入力、キャプチャー、トラブル対処 | [Guide](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/docs/guide.md) | [利用ガイド](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/docs/guide.ja.md) |
| API導入例 | [Bilingual API overview](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/docs/api.md) | 同左 |
| 検証範囲と過去の記録 | [Validation](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/docs/validation.md) | 同ファイルに併記 |
| メンテナー向け公開手順 | [Publishing](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/docs/publishing.md) | 同ファイルに併記 |
| リリースごとの変更 | [Changelog](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/CHANGELOG.md) | 同ファイルに併記 |

APIリファレンスは`cargo doc --no-deps --lib --open`でローカル生成できます。
[docs.rs](https://docs.rs/bevy_looking_glass)は、レジストリ公開とdocs.rs側のビルド成功後に利用する掲載先です。
リンクの存在は公開完了を示すものではありません。

## 検証されている範囲

v0.1.0のWindowsレビューには、ライブラリテスト40件、example 3件のビルド、
Clippy、配布用アーカイブのコンパイル成功が記録されています。
それ以前のWindows・RTX 4090・Vulkanでの検証では、通常のデスクトップウィンドウ上で
生成視点、HDR Bloom/DoF、外部入力、PNG出力を確認しています。
PR #2には別途、macOS・Apple M4・Portraitでの配置と、実パネルにアバターが表示された記録があります。

いずれも**日付のある過去の報告**であり、このドキュメント変更で新たに実行した検証ではありません。
全機種での光学品質、Windowsでの実パネルへの配置、Linux実行、カメラ・NDIとの同時動作、
深度再投影による実測の高速化は、これらの記録では確立されていません。
詳細な範囲と出典は[検証記録](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/docs/validation.md)を参照してください。
過去のRusTuberVのフレームレート測定は、このcrateの性能保証ではありません。

## ライセンスとプロジェクトの方針

プロジェクトのソースは[MIT](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/LICENSE)です。
参照資料・依存関係のライセンスは
[第三者告知](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/THIRD_PARTY_NOTICES.md)を参照してください。
依存ライブラリと別途インストールするLooking Glassソフトウェアには、それぞれの条件が適用されます。
SDKバイナリー、工場出荷時の校正ファイル、モデル、フォントは同梱しません。
Looking Glassという名称は対象ハードウェアを示すもので、本プロジェクトはメーカーから独立しています。

Issue・PRは歓迎しますが、返信・修正・継続的な保守は約束しません。
変更は目的を絞り、純粋な計算とECS・GPU・I/Oの副作用を分離します。
アプリ全体のフレームワークや自動フォールバックは持ち込みません。
有用な改善を正確な説明とともに共有するために、全OS・全機種対応を先に完成させる必要はありません。
