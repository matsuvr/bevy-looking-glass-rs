# 利用ガイド

[English](guide.md) | 日本語 | [README](../README.ja.md) | [API導入例](api.md)

Bevy 0.20向けの`0.1.x` APIについて説明します。
実機で確認した範囲は[検証記録](validation.md)を参照してください。
[API導入例](api.md)にはrustdocでコンパイル検査できる完全なコード例があります。

## 機器検出と校正

`bridge::discover() -> Result<Vec<bridge::Display>, bridge::Error>`は、
`http://localhost:33334/`にあるBridgeのローカルHTTP APIへの同期問い合わせです。
ウィンドウには依存しません。アプリ起動前、または対話型アプリのワーカーで呼び出します。
各リクエストのタイムアウトは5秒で、HTTPプロキシを経由しません。検出全体では複数のリクエストを行います。
orchestrationを開始し、`available_output_devices`を要求し、解析失敗時も含めてorchestrationを終了します。
Bridgeのインストール・起動、校正ファイルの保存、ウィンドウ作成は行いません。

| 結果 | 意味 |
| --- | --- |
| `Err(bridge::Error::Unavailable)` | ローカルBridgeへのリクエストで利用可能なサービスに到達できなかった |
| `Ok(vec![])` | Bridgeは応答したが、校正情報を持つライトフィールド機器が見つからなかった |
| `Err(bridge::Error::Response(_))` | HTTP・プロトコル・機器データのエラー |
| `Ok(displays)` | 通常は安定したシリアル番号で対象を選択する |

`Display`には未改変の校正文字列、シリアル、名前、ハードウェア識別子、任意の推奨Quilt、
Bridge側のindex、デスクトップ上の位置が入ります。通常のモニターと校正のない機器は除外します。
未知のモデル名は受け入れます。**BridgeのindexとOSのモニターindexは別物です。**

校正文字列のバイト列を`parse_calibration`へ渡します。
利用アプリが`visual.json`を読み、そのバイト列を直接渡す方法もあります。
解析と光学値の正規化はI/Oを行いません。取得後の描画にBridgeの常駐は不要です。
exampleには実校正情報が必要です。テスト用の合成値は、実パネルの校正の代わりにはなりません。

## プラグインと利用アプリの責務

Bevyのasset・render・windowプラグインの後に`LookingGlassPlugin`を追加します。
ライブラリは必要な描画APIのみを有効にし、Bevyの全デフォルトfeatureは有効にしません。
ウィンドウバックエンド、PBRなどのシーン用プラグイン、後処理のfeatureは利用アプリが用意します。
LUTを必要とするトーンマッパーには、対応するBevyのLUT featureも必要です。
最初は導入例どおり`DefaultPlugins`を使うと簡単です。

`LookingGlassControl::start(config)`は、一つの管理対象出力を開始・置換します。
同一フレームの開始・停止要求では最後の要求を採用します。
設定が拒否された場合は既存出力を維持し、`LookingGlassStatus::error`に報告します。
要求が成功すると、そのエラーは消えます。`stop()`は所有するウィンドウ、子カメラ、
アセットハンドルを解放します。出力ウィンドウを閉じると子Entityも解放します。
利用アプリがフレーム画像のハンドルを保持している場合、その画像は保持されます。

シーンのアニメーション、ファイル、ダイアログ、設定保存、キー入力、メインウィンドウの動作、
アプリ終了は利用アプリが管理します。プラグインは他のカメラ、タスクプール、
共有`Core3d`実行器を変更しません。exampleのショートカットや連動終了もプラグインには含みません。

`scene_layers`は各視点カメラに映す内容を選びます。
`display_layer`はインターレース用メッシュの専用レイヤーとし、シーンや他のカメラのレイヤーと重ねません。
これにより、出力画像そのものがシーンに映り込むことを避けます。

## 実行順序

| 利用アプリの処理 | 実行位置 |
| --- | --- |
| 開始・停止・Quiltプレビュー切り替え | `Update`の`LookingGlassSystems::Control`より前 |
| 状態参照、新規視点カメラへの効果追加 | `Update`の`LookingGlassSystems::Control`より後 |
| ワールド座標のカメラ姿勢を渡す | `PostUpdate`の`LookingGlassSystems::SyncViews`より前 |
| 出力に含める独自の視点別描画 | render sub-appの`Core3d`で`LookingGlassRenderSet::Capture`より前 |
| Quiltキャプチャー要求 | `Update`が簡単。消費処理は`PostUpdate`の`SyncViews`後 |
| キャプチャー完了の受信 | 利用アプリのsystemで`QuiltCaptureResult`メッセージを読む |

`SyncViews`はTransform伝播後、カメラ・可視性の更新前に実行します。
キャプチャーは後処理とアップスケーリング後、次のカメラが一時描画先を使う前に、視点ごとに実行します。
視点ごとの独自コピーを全カメラの描画後まで遅らせないでください。
その時点では一時描画先が別の視点で上書きされています。

## 撮影範囲とライブ操作

`LookingGlassView::pose`に`ViewPose`を設定します。
ローカル負Z方向を向くワールド座標Transform、正の焦点距離、ラジアン単位の垂直画角、
near/farクリップ距離を指定します。Transformは単位スケールと正規化済み回転を使用し、
`0 < fov_y < pi`、`0 < near < far`とします。`far`には正の無限大も使えます。
シーン投影のアスペクト比はタイルやプレビューウィンドウではなく、パネルの比率です。

`pose: None`ではシーンカメラを無効化し、残っているシーン画像も隠します。
不正な姿勢は`LookingGlassStatus::pose_error`に入り、正しい姿勢へ直すと再起動なしで再開します。
外部画像入力には姿勢は不要です。

`view_pose_from_volume(&HologramVolume, fov_y)`は純粋な計算です。
焦点面のTransformで表示空間の中心・向きを、`height`でその面での表示高さを指定します。
カメラ距離は`height / (2 * tan(fov_y / 2))`です。near/farはカメラからのクリップ距離のままです。
戻り値を姿勢に代入すれば、出力リソースを再作成せずに撮影範囲を変更できます。

`LookingGlassOptics::set_depth_scale(value)`は視点間隔の倍率をライブで上書きします。
0で視差なし、1で校正された視野角になります。`reset_depth_scale()`で現在の設定の初期値に戻ります。
レイアウト、入力ソースなどの構造的な設定変更には、改めて`start(config)`を呼びます。

## ネイティブ出力への配置

自動配置には`config.display = Some(selected_display)`を指定します。
`plan_output_window`は、Bridgeが返す物理デスクトップ座標とネイティブ解像度を
Bevyの`MonitorInfo`と照合します。負の座標にも対応します。
一致するモニターがない・複数あって曖昧な場合は、別のモニターを選ぶのではなくエラーを報告します。

Windows・macOSでは、計画したネイティブサイズでウィンドウを作り、`WindowCreated`の後で
対象モニターへのボーダーレス全画面表示を要求します。この遷移時にはmodeだけを変更します。
混在DPIのデスクトップで、作成時の全画面化によりメインモニターへ出てしまう問題を避けるためです。
作成完了前に対象モニターが消えた場合は、保留中の出力を閉じてエラーを返します。
定期的なホットプラグ監視や自動再接続は行いません。

`config.display = None`なら、利用アプリの`config.window`をそのまま使います。
タイトル、位置、モニター・表示モード、サイズ、表示タイミングを指定できます。
コンストラクターはスケール係数1のネイティブ物理ピクセルを指定します。
光学出力では正しい解像度・向きの拡張デスクトップを使い、ミラーリングや拡大縮小は使いません。
ウィンドウ表示・サイズ変更した表示はプレビュー用途です。

## 視点描画、後処理、描画結果へのアクセス

`ViewRenderSettings`は所有する視点カメラの露出、トーンマッピング、ディザリング、
ライトクラスタリング、直接・間接描画、HDR中間バッファ、クリア色を設定します。
保存・出力される最終視点はSDRで、MSAAは無効です。
HLDの背景はライトフィールドのクリア色設定によらず白です。

実際にシーンを描くカメラには`LookingGlassViewCamera { output, index }`、
所有元には`LookingGlassFrame { views, layout, metadata }`が付きます。
両方ともrender worldへ抽出されます。`views`は光学的な視点順に並ぶsRGB SDRの配列画像です。
`metadata.view_count`は有効視点数、`layout.count()`はQuiltの枠数です。
外部の部分Quiltでは、枠数が有効視点数より多い場合があります。

`Control`より後に公開カメラマーカーへBevyの効果を追加します。
[完全なBloom/DoFヘルパー](api.md)を参照してください。
Bloomには`config.view_render.hdr`とBevyの後処理プラグインを有効にします。
`LookingGlassSyncFocus`はDoFの焦点を基準姿勢の焦点距離に同期します。
効果は各視点に適用し、最終的なインターレース画像へぼかしをかけません。

Bevy 0.20のDoF extractorはカスタム投影を受け付けないため、
透視投影の画角を持つ設定専用の子Entityを用い、Bevy自身のDoF uniformを光学カメラへ渡します。
off-axis投影とカリング用frustumは維持します。DoFを外すと子Entityも外します。
独自ぼかしエンジンやBevyへのパッチは不要です。

## 明示的な深度生成

既定の`ViewGeneration::Full`では全視点を描きます。
`ViewGeneration::DepthReprojection { rendered_views: n }`では、両端を含む等間隔の`n`視点を実描画します。
範囲は`2 <= n <= 出力視点数`です。純粋関数`plan_view_generation`が基準視点と隣接参照を決めます。

基準視点の色は後処理後に取得し、不透明深度は一時描画先が再使用される前にコピーします。
WESLで各画素を一定深度の面として不足視点へ前方投影し、
reverse-Zの深度テストで左右の基準視点を合成します。
基準視点の色と深度には別の配列を使い、同一配列への読み書き競合を避けます。
生成用深度バッファとuniformは再利用します。

これは**不透明シーンの再投影**であり、完全なシーン描画や時間方向のフレーム補間ではありません。
透過面は一般に対応する深度を書きません。反射・鏡面反射の変化、形状をまたぐぼかし、
両基準視点から隠れた面ではアーティファクトが出ることがあります。埋まらない画素は黒く残ります。
用途に応じて`Full`または多めの基準視点を明示的に選びます。
材質の自動分類や自動フォールバックはありません。
再投影自体もGPU負荷を加えるため、描画カメラ数の削減はFPS向上の保証ではなく、
記録されている検証でも高速化率は確立されていません。

## 外部Quiltと配列画像

`config.source`に`LookingGlassSource::Quilt { image, metadata }`または
`LookingGlassSource::ViewArray { image, metadata }`を指定します。既定値は`Scene`です。
外部入力ではシーンカメラを作らず、基準姿勢も不要です。
視点生成はシーン入力だけに適用するため、外部入力では`ViewGeneration::Full`を使います。

画像が`Assets<Image>`に存在する状態で開始します。
画像は単一サンプル・単一mip・`Rgba8UnormSrgb`の2Dテクスチャで、render worldに利用可能である必要があります。
Quiltには`COPY_SRC`、配列には`COPY_SRC | TEXTURE_BINDING`と、
layer・mipとも0から始まる`D2Array`ビューが必要です。サイズ・レイヤー数はメタデータと整合させます。
ストリーミングでは同じ画像ハンドルを保持して内容を更新します。動画デコードは利用アプリの責務です。

`QuiltMetadata`には画像全体のサイズ、列数、行数、有効`view_count`、元の`view_aspect`を持たせます。
タイルサイズは整数除算で求めます。内容のアスペクト比はタイルのピクセル比と独立です。
視点0は左下で、余りピクセルは上端・右端にあります。無効な枠はサンプリングしません。
元の比率を保ってパネルに収めます。QuiltはGPUでタイルごとにコピーし、
外部配列は直接サンプリングします。Quiltプレビューの余白・無効枠は黒になります。

```sh
cargo run --example quilt_input -- visual.json quilt.png 8 6 48 0.75
cargo run --example quilt_input -- visual.json quilt.png 8 6 48 0.75 --array
```

`--array`のexampleでは、配列への取り込みを一度だけCPUで行います。
外部GPUレンダラーが配列を渡す場合、この取り込みは不要です。
F9でキャプチャー、F10でQuiltプレビュー、Escapeで出力停止です。

## Quilt PNGと固定時刻の連番

`PostUpdate`の消費処理より前に
`QuiltCapture::request(CaptureRequest { output, frame, time })`を呼びます。
Bevyが別スレッドで描画をパイプライン実行している場合も、要求は同じmain-worldフレームで抽出されます。
GPUアセットが未準備なら、完了画像を取得できるフレームまで延期します。
`frame`と`time`は**利用アプリが付けるラベル**であり、進み続けるシミュレーションを過去の指定時刻で
撮影する保証ではありません。固定時刻の連番では、撮影完了までシミュレーションの姿勢を保持します。

`QuiltCaptureResult`の`Result<CapturedQuilt, String>`を受信します。
出力停止・置換では、まだ描画されていない保留要求を取り消します。GPUマッピング失敗はエラーになります。
Quiltと読み戻しバッファの確保は要求時だけで、通常出力ではQuilt合成も読み戻しも行いません。

有効レイヤーを左下始まりのQuiltへコピーして非同期読み戻しします。
CPU側の出力は左上始点の隙間なく詰めたsRGB RGBA8で、GPU行アラインメントのパディングは除去します。
無効枠・余りピクセルは0です。`CapturedQuilt::write_png`は利用アプリのwriterへエンコードし、
`save_png`は指定どおりのパスへ保存します。
エンコード・ファイルI/Oは必要に応じてワーカーで行い、レンダラーの毎フレーム処理へ入れません。
`metadata.filename_suffix()`は`_qs8x6a0.75`形式の名前を返します。
この接尾辞には有効視点数がないため、部分Quiltの`view_count`は別に保存します。

```sh
cargo run --example capture_sequence -- visual.json output-directory
cargo run --example capture_sequence -- visual.json output-directory --generated
cargo run --example capture_sequence -- visual.json output-directory --hdr
cargo run --example capture_sequence -- visual.json output-directory --generated --hdr
```

exampleは各姿勢を撮影完了まで保持し、描画・読み戻しにかかった実時間とは独立に、
30Hzのフレーム時刻で先へ進みます。ワーカーでPNGを4枚保存し、エンコード完了後に終了します。
`--generated`では8視点中3視点を実描画し、`--hdr`でBloom/DoFを有効にします。
動画エンコーダーやFFmpegへの依存はありません。

## 描画構成と機器カタログ

平行なoff-axisカメラは小さな一時描画先を共有します。
最初のカメラはattachment確立のためBevyの出力blitを残し、以降はそのblitを省きます。
描画完了した視点を配列の各レイヤーへコピーします。
インターレースと仮想Quiltプレビューは配列を直接参照し、ライブ経路には
Quiltテクスチャ、合成カメラ、タイル用spriteはありません。HDR中間画像は保存前にSDR化します。
生の校正値をCPUで正規化し、シェーダーはRGBストライプとセルパターンモード0〜4を扱います。

`device_presets()`は仕様出典と調査日を含む`src/devices.json`を読みます。
`DevicePreset::light_field_output(calibration)`は基準グリッドを使用し、ネイティブパネル寸法の一致を要求します。
任意解像度には`ViewLayout`を使います。カタログにはGo、Portrait、旧8.9/15.6/Pro、
新旧16/27/32/65インチの各モデル、HLD 16/27/86を含みます。
これは仕様プリセットであって、その機種での実機検証の証拠ではありません。

`hld_output()`はカタログの安全表示領域を使う縦向き単一視点出力で、レンズ校正を必要としません。
HLDはレンチキュラーレンズによる多視点出力ではありません。
musubiはカタログ情報のみで、ライブのデスクトップ出力は提供しません。

## トラブル対処

| 症状 | 確認する点 |
| --- | --- |
| Bridgeが見つからない・機器一覧が空 | Bridgeの起動、映像・USBデータの両接続を確認。通信エラーと空の結果は区別する |
| 出力ウィンドウが作られない | `Control`後の`LookingGlassStatus::error`を確認。画像読み込み、メタデータ、モニター一致を確認 |
| シーン出力が黒い | 有効な姿勢を渡し、`pose_error`、シーンのレイヤー、内容、照明を確認 |
| 通常プレビューだけが動く | 利用アプリのカメラとともに`LookingGlassView::pose`も更新する |
| 奥行き・インターレースがおかしい | 個体に合う校正、ネイティブ解像度、向き、拡張デスクトップを確認。Quiltプレビューを終了する |
| 生成視点に乱れが出る | `Full`または基準視点増加を明示的に選ぶ。不透明シーン向けの制限を参照 |
| Bloomが出ない | 開始前にHDR設定とBevyの効果プラグイン・featureを有効にする |
| キャプチャーが終わらない | フレーム・メッセージ処理を続ける。アセット準備と出力状態を確認。Bevyのイベントループ自体は止めない |

Issueにはcrate・Bevy版、OS・GPU、機種、入力・生成モード、エラー、小さな再現例があると役立ちます。
必須の提出チェックリストではありません。返信や修正も約束しません。

## 参考資料

- [Quiltの規約](https://lfdocs.lookingglassfactory.com/keyconcepts/quilts)
- [Bridge SDKの範囲](https://lfdocs.lookingglassfactory.com/software/looking-glass-bridge-sdk)
- [公式Bridge.js HTTPプロトコル](https://github.com/Looking-Glass/Bridge.js/tree/main/src/library)
- [Windowsのネイティブ表示設定](https://lfdocs.lookingglassfactory.com/software/looking-glass-bridge/display-settings-on-windows)
- [校正ファイルの案内](https://lfdocs.lookingglassfactory.com/software/index/unity-plugin-4.0-alpha)
- [校正の参照リビジョン](https://github.com/Looking-Glass/looking-glass-webxr/blob/b44ed27fea197f28fbe4cb669bc350d141a9f8b1/src/LookingGlassConfig.ts)
- [Bevy 0.20のカスタム投影例](https://github.com/bevyengine/bevy/blob/v0.20.0/examples/camera/custom_projection.rs)

ライセンスと由来は[第三者告知](../THIRD_PARTY_NOTICES.md)を参照してください。
