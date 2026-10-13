# Looking Glass output for Bevy / BevyからのLooking Glass出力

An unofficial, experimental plugin for Bevy 0.20, using safe Rust and embedded
WESL. One app manages one output. Optional discovery uses separately installed
Bridge over HTTP; rendering does not link the Bridge SDK. This personal research
project is provided without support commitments.

Bevy 0.20向けの非公式・実験的プラグインです。CPU側はsafe Rust、GPU側は埋め込みWESLで実装しています。
一つのアプリで一つの出力を管理します。任意の機器検出は別途インストールしたBridgeのHTTP APIを使い、
描画にBridge SDKをリンクしません。個人の研究プロジェクトとして、サポートを約束せず提供します。

[English guide](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/docs/guide.md) ·
[日本語ガイド](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/docs/guide.ja.md) ·
[Validation / 検証範囲](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/docs/validation.md)

## Quick start / 最小の組み込み例

Use Rust 1.99 or newer and `bevy = "0.20"` with default features in a desktop
application. The [README](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/README.md)
explains the registry dependency and the matching Git source release.
Install/start Bridge and connect the panel's video and USB data cables.
Discovery below runs before the event loop; in an interactive application, run
this synchronous query on a worker instead of blocking an update system.

Rust 1.99以上と、デフォルトfeatureを有効にした`bevy = "0.20"`を使うデスクトップアプリの例です。
レジストリ指定と対応するGit版は[日本語README](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/README.ja.md)
を参照してください。Bridgeをインストール・起動し、映像ケーブルとUSBデータケーブルを接続します。
ここではイベントループ開始前に同期的に検出します。動作中のアプリではワーカーで実行してください。

This complete example displays a lit cube at the focus plane, with an ordinary
preview camera and a Looking Glass output. The first discovered display is used;
select by serial when your app needs a specific unit. Output-start errors are
reported through `LookingGlassStatus::error`, not the return value of `start()`.

以下は焦点面上の立方体とライト、通常画面のプレビューカメラ、Looking Glass出力を含む完全な例です。
最初に検出した機器を使います。特定の個体を使うアプリではシリアル番号で選んでください。
出力開始のエラーは`start()`の戻り値ではなく、`LookingGlassStatus::error`に入ります。

```rust,no_run
use bevy::{camera::visibility::RenderLayers, prelude::*};
use bevy_looking_glass::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let display = bridge::discover()?
        .into_iter()
        .next()
        .ok_or("no calibrated Looking Glass is connected")?;
    let quilt = display.quilt.as_ref().ok_or("Bridge returned no default quilt")?;
    let layout = ViewLayout::new(
        quilt.columns,
        quilt.rows,
        quilt.width.checked_div(quilt.columns).ok_or("zero quilt columns")?,
        quilt.height.checked_div(quilt.rows).ok_or("zero quilt rows")?,
    )?;
    let calibration = parse_calibration(display.calibration.as_bytes())?;
    let mut config = LookingGlassConfig::new(
        LookingGlassOutput::LightField {
            calibration,
            layout,
            depth_scale: 1.0,
        },
        RenderLayers::layer(0), // Scene / シーン
        1, // Output-only layer / 出力専用レイヤー
    );
    config.display = Some(display);
    let mut control = LookingGlassControl::default();
    control.start(config);

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(LookingGlassPlugin)
        .insert_resource(control)
        .insert_resource(bevy::winit::WinitSettings::continuous())
        .insert_resource(LookingGlassView {
            pose: Some(ViewPose {
                transform: Transform::from_xyz(0.0, 0.0, 5.0),
                focus_distance: 5.0,
                fov_y: 45_f32.to_radians(),
                near: 0.1,
                far: 100.0,
            }),
        })
        .add_systems(Startup, setup_scene)
        .run();
    Ok(())
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 0.0, 5.0),
        RenderLayers::layer(0),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.2, 0.8, 0.4))),
    ));
    commands.spawn((
        PointLight {
            intensity: 2_000_000.0,
            ..default()
        },
        Transform::from_xyz(2.0, 4.0, 4.0),
    ));
}
```

The application owns keyboard controls and exit policy; close both windows when
finished, or use `examples/minimal.rs` for coordinated closing and interactive
controls. Publish a new `LookingGlassView::pose` when moving the reference camera.
Do not put the output layer on the scene or ordinary preview cameras.

キー操作と終了処理は利用アプリの責務です。この例を終える際は両ウィンドウを閉じるか、
終了を連動させる実装と操作キーがある`examples/minimal.rs`を利用してください。
基準カメラを動かすときは`LookingGlassView::pose`も更新します。
出力専用レイヤーはシーン用・通常プレビュー用カメラに含めません。

## API map / APIの役割

| API | English | 日本語 |
| --- | --- | --- |
| `bridge::discover`, `parse_calibration` | Acquire/parse per-device calibration | 個体別校正の取得・解析 |
| `LookingGlassConfig`, `LookingGlassControl` | Configure, start/replace, stop output | 設定、開始・置換、停止 |
| `LookingGlassStatus` | Read output entity, preview state, and errors | 出力Entity、プレビュー状態、エラーの参照 |
| `ViewPose`, `LookingGlassView` | Supply the world-space reference camera | ワールド座標で基準カメラを指定 |
| `HologramVolume`, `view_pose_from_volume` | Frame a focal-plane center and height | 焦点面の中心と高さから撮影範囲を指定 |
| `LookingGlassOptics` | Change depth without recreating assets | リソース再作成なしで奥行きを変更 |
| `LookingGlassViewCamera`, `LookingGlassSyncFocus` | Attach per-view effects and synchronize DoF | 視点ごとの効果追加とDoF焦点の同期 |
| `LookingGlassFrame`, `LookingGlassSource`, `QuiltMetadata` | Access completed views or supply external images | 描画結果へのアクセス、外部画像入力 |
| `ViewGeneration`, `plan_view_generation` | Explicitly select full rendering or depth generation | 全描画・深度生成を明示的に選択 |
| `QuiltCapture`, `CaptureRequest`, `QuiltCaptureResult` | Request and receive asynchronous captures | 非同期キャプチャーの要求と結果受信 |
| `CapturedQuilt` | Encode/write a captured image when the host asks | 利用アプリの要求で画像をエンコード・保存 |

## Scheduling and ownership / 実行順序と所有範囲

In `Update`, produce start/stop/preview requests **before**
`LookingGlassSystems::Control`, then inspect status or attach effects **after** it.
Publish world-space poses before `LookingGlassSystems::SyncViews` in `PostUpdate`;
that set runs after transform propagation. Custom `Core3d` passes that must appear
in output run before `LookingGlassRenderSet::Capture` in the render sub-app.
Send capture requests from `Update` to precede their `PostUpdate` consumer.

`Update`での開始・停止・プレビュー要求は`LookingGlassSystems::Control`より**前**、
状態参照と効果の追加は同setより**後**に実行します。
ワールド座標の姿勢は`PostUpdate`の`LookingGlassSystems::SyncViews`より前に渡します。
このsetはTransform伝播後に動きます。出力に含める独自`Core3d`パスはrender sub-appの
`LookingGlassRenderSet::Capture`より前に実行します。
キャプチャー要求を`Update`で送れば、`PostUpdate`の消費処理より前になります。

`start()` replaces the current output; changing poses or live depth does not.
`stop()` releases plugin-owned entities and handles, not images retained by the
host. `pose: None` hides scene output; external image sources do not need a pose.
The plugin does not control the host's main window, animation, file dialogs,
settings, or application exit.

`start()`は現在の出力を置き換えます。姿勢やライブ奥行きの変更では置き換えません。
`stop()`はプラグイン所有のEntityとハンドルを解放しますが、利用アプリが保持する画像まで破棄しません。
`pose: None`ではシーン出力を隠します。外部画像入力に姿勢は不要です。
メインウィンドウ、アニメーション、ファイルダイアログ、設定、アプリ終了は制御しません。

## Pure planning example / 純粋な計算の例

This example needs neither a GPU nor Bridge. It does not predict a speedup.
GPUもBridgeも不要な例です。高速化率を予測するものではありません。

```rust
use bevy_looking_glass::{plan_view_generation, ViewGeneration};

let plan = plan_view_generation(
    48,
    ViewGeneration::DepthReprojection { rendered_views: 8 },
)?;
assert_eq!(plan.rendered.len(), 8);
assert_eq!(plan.generated.len(), 40);
# Ok::<(), bevy_looking_glass::LookingGlassError>(())
```

## Per-view effects / 視点ごとの効果

With Bevy's post-processing plugins enabled, call this helper **before** sending
`config` to `LookingGlassControl::start`. `DefaultPlugins` includes those plugins
when the corresponding Bevy features are enabled. Effects run before view capture,
not on the interlaced panel. See the guide for reprojection limitations.

Bevyの後処理プラグインを有効にし、`config`を`LookingGlassControl::start`へ渡す**前**に
次のヘルパーを呼びます。対応featureを有効にした`DefaultPlugins`には後処理プラグインも含まれます。
効果は各視点のキャプチャー前に適用し、インターレース後のパネル画像にはかけません。
深度再投影との組み合わせの制限は利用ガイドを参照してください。

```rust,no_run
use bevy::{
    post_process::{bloom::Bloom, dof::DepthOfField},
    prelude::*,
};
use bevy_looking_glass::{
    LookingGlassConfig, LookingGlassSyncFocus, LookingGlassSystems, LookingGlassViewCamera,
};

fn add_effects(app: &mut App, config: &mut LookingGlassConfig) {
    config.view_render.hdr = true;
    app.add_systems(Update, effects.after(LookingGlassSystems::Control));
}

fn effects(mut commands: Commands, cameras: Query<Entity, Added<LookingGlassViewCamera>>) {
    for camera in &cameras {
        commands.entity(camera).insert((
            Bloom::default(),
            DepthOfField::default(),
            LookingGlassSyncFocus,
        ));
    }
}
```
