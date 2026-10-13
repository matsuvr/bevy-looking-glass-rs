# Publishing / 公開手順

[English README](../README.md) · [日本語README](../README.ja.md)

## English

This is a short maintainer workflow, not a CI policy or a requirement for
enterprise support. The documentation PR does not upload anything to crates.io,
change ownership, create a release, or move the existing `v0.1.0` tag.

### Prepare the release

The existing `v0.1.0` tag identifies the GitHub source release. Documentation edits
made after that tag belong to a different commit. For the first registry release
containing those edits, choose a new patch version such as `0.1.1`; do not move the
old tag. Update `Cargo.toml`, its lockfile package version, and `CHANGELOG.md`
together. This guide does not reserve that version or claim it was published.

Prepare both READMEs' installation sections for the selected release: the registry
entry becomes the normal installation method and the Git source release remains
an alternative. Remove the pre-publication explanation as part of that release
preparation. Do not announce a successful upload or add success badges before the
upload actually succeeds. Check that the repository and release links name the
right version. `0.1.x` patch releases preserve the existing public API; breaking
changes move to `0.2.0` or a later minor version.

`Cargo.toml`'s `include` list must retain `README.md`, `README.ja.md`, `docs/**`,
`CHANGELOG.md`, `LICENSE`, and `THIRD_PARTY_NOTICES.md`, as well as the existing
sources/examples/shaders/catalog. `docs/api.md` is included by `src/lib.rs` and is
needed even when the package is compiled without building its documentation.
Do not ship personal factory calibration files or example model/font assets.

### Check the documentation and package

Run these from the release checkout with its stable toolchain (Rust 1.99 or newer):

```sh
cargo fmt --all -- --check
cargo test --doc --locked
cargo doc --no-deps --lib --locked
cargo package --list --locked
cargo publish --dry-run --locked
```

Open the generated crate index at
`target/doc/bevy_looking_glass/index.html` and follow the English/Japanese guide
links. `cargo test --doc` compiles the two `no_run` examples without contacting
Bridge or opening windows and runs the pure view-planning example.
`cargo publish --dry-run` also builds the extracted package with normal
dependencies, which helps detect missing bundled documentation/assets and hidden
reliance on dev-dependencies. It does not upload a crate or reserve its name.

The existing code checks can also be run when preparing a release:

```sh
cargo test --lib --locked
cargo clippy --all-targets --locked
```

Record the results actually obtained. A docs.rs-hosted build, a cross-platform
build, and a physical-panel test are different checks; do not claim them from a
local `cargo doc` success. Check versions and dependency availability using the
actual release environment. No new CI service, security framework, or all-hardware
verification program is required by this project.

### Upload deliberately

After reviewing and committing the release contents, the owner can log in to Cargo
interactively and upload:

```sh
cargo login
cargo publish --locked
```

These commands are instructions for the release owner, **not commands executed by
the documentation update**. An existing Cargo login can be reused. After a
successful upload, verify the selected version on
[crates.io](https://crates.io/crates/bevy_looking_glass) and check the
[docs.rs build](https://docs.rs/bevy_looking_glass). Create a new tag for the actual
released commit/version; leave `v0.1.0` unchanged. Do not treat a local `.crate`
archive or a GitHub release as proof of registry publication.

### Sources

[Cargo publishing](https://doc.rust-lang.org/cargo/reference/publishing.html),
[rustdoc tests](https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html),
[Cargo version requirements](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html),
[docs.rs builds](https://docs.rs/about/builds).

## 日本語

メンテナー向けの短い手順です。CIの規則やエンタープライズ向けサポートの要件ではありません。
ドキュメント整備PRでは、crates.ioへのアップロード、所有者変更、リリース作成、
既存の`v0.1.0`タグの移動は行いません。

### リリースの準備

既存の`v0.1.0`タグはGitHubでのソース公開を指します。
その後のドキュメント変更は別コミットになるため、それを含む初回レジストリ公開には
`0.1.1`など新しいパッチ版を選び、古いタグは動かしません。
`Cargo.toml`、lockfile内のパッケージバージョン、`CHANGELOG.md`をまとめて更新します。
この手順書は、そのバージョンを予約したり公開済みと宣言したりするものではありません。

両READMEの導入欄を選んだリリース向けに整え、レジストリ指定を通常の導入方法、
Git版を代替方法にします。公開準備の変更として未公開段階の説明を外します。
アップロードが実際に成功するまでは、成功の告知やバッジは付けません。
リポジトリ・リリースへのリンクが正しい版を示すことも確認します。
`0.1.x`のパッチ版では既存公開APIとの互換性を保ち、破壊的変更は`0.2.0`以降の別マイナー版にします。

`Cargo.toml`の`include`には、既存ソース・example・シェーダー・カタログとともに、
`README.md`、`README.ja.md`、`docs/**`、`CHANGELOG.md`、`LICENSE`、
`THIRD_PARTY_NOTICES.md`を残します。
`docs/api.md`は`src/lib.rs`から取り込むため、ドキュメントを生成しない通常コンパイルでも必要です。
個人の実校正ファイルやexample用のモデル・フォント資産は配布物に入れません。

### ドキュメントと配布物の確認

リリース用チェックアウトで、stableツールチェーン（Rust 1.99以上）を使って実行します。

```sh
cargo fmt --all -- --check
cargo test --doc --locked
cargo doc --no-deps --lib --locked
cargo package --list --locked
cargo publish --dry-run --locked
```

`target/doc/bevy_looking_glass/index.html`を開き、英語・日本語ガイドへのリンクを確認します。
`cargo test --doc`はBridge接続やウィンドウ作成をせずに二つの`no_run`例をコンパイルし、
純粋な視点計画の例を実行します。
`cargo publish --dry-run`は展開した配布物を通常依存でビルドするため、
ドキュメント・アセットの同梱漏れやdev-dependenciesへの隠れた依存を見つけるのにも使えます。
アップロードや名前の予約は行いません。

リリース準備では既存のコード検査も実行できます。

```sh
cargo test --lib --locked
cargo clippy --all-targets --locked
```

実際に得られた結果を記録してください。ローカルの`cargo doc`成功は、
docs.rsでのビルド成功、別OSでのビルド成功、実パネル検証を意味しません。
バージョンや依存の取得可否は、実際に公開する環境で確認します。
このプロジェクトで新たなCIサービス、セキュリティ基盤、全機種検証体制を導入する必要はありません。

### 公開操作

リリース内容を確認・コミットした後、所有者がCargoへ対話的にログインして公開します。

```sh
cargo login
cargo publish --locked
```

これは公開担当者への案内であり、**ドキュメント更新作業が実行するコマンドではありません**。
既存のCargoログインも使えます。成功後に[crates.io](https://crates.io/crates/bevy_looking_glass)の
対象バージョンと[docs.rsのビルド](https://docs.rs/bevy_looking_glass)を確認します。
実際に公開したコミット・バージョンの新しいタグを作り、`v0.1.0`はそのまま残します。
ローカルの`.crate`やGitHubリリースだけで、レジストリ公開済みとは判断しないでください。

### 出典

[Cargoの公開手順](https://doc.rust-lang.org/cargo/reference/publishing.html)、
[rustdocのテスト](https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html)、
[Cargoのバージョン指定](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html)、
[docs.rsのビルド](https://docs.rs/about/builds)。
