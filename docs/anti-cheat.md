# アンチチート安全性

このアプリで一番気をつけているのは、ゲームと一緒に使っても主要なアンチチート（EAC / BattlEye / Vanguard / Ricochet など）に誤検知されないことです。その方針と根拠、そして「触ってはいけないゲームのウィンドウ」を取りこぼさず避ける考え方をまとめます。技術的な裏づけは末尾の[出典](#出典)に挙げた一次情報と解析で確認しています。

## 基本原則：境界線は「注入」

調べた範囲では、アンチチートが嫌うのは一貫してゲームへのコード注入とメモリ操作です。Bungie も、Destiny 2 で弾くのは「クライアントにコードを挿入する手法」であり、外からのキャプチャや常駐の監視ツールは許容すると説明しています。このアプリはゲームのプロセス空間・メモリ・入力経路に一切踏み込みません。やるのは「自分のプロセスでホットキーを受け取り、`SetWindowPos` で外からウィンドウを動かす」ことだけで、次の API や手法は使いません。

- 低レベル入力フック（`WH_*_LL`）
- 他プロセスへの DLL インジェクション
- 他プロセスのメモリ操作（`ReadProcessMemory` / `WriteProcessMemory` と、それに必要な `PROCESS_VM_*` 権）
- カーネルドライバ
- 入力の合成・ブロードキャスト（`SendInput` でキーやマウスを送る類）

## 使う API とその根拠

能動的に使う Win32 API は、どれも正規ツールで一般的なものに絞っています。

| API | 安全側と判断する根拠 |
|---|---|
| `RegisterHotKey` | フック不要のホットキー登録。入力を合成も傍受もしません。トリガーボット検知が見るのは入力のタイミングで、ホットキー登録そのものは対象外です。 |
| `SetWindowPos` / `ShowWindow` / `EnumWindows` | 外からウィンドウを動かす・列挙するだけで、相手のプロセスには触れません。`SWP_NOACTIVATE` を付けてアクティブ化もしません。 |
| `GetWindowThreadProcessId` | HWND から所有プロセスの PID を得ます。ハンドルは開きません。 |
| `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` + `QueryFullProcessImageNameW` | 実行ファイルのパスを読むだけです。メモリ読み書き権を含まない最小権限です（次節）。 |
| `SHQueryUserNotificationState` | フルスクリーン・排他状態を調べる非特権のシェル API。昇格もフックも要りません。 |

### `PROCESS_QUERY_LIMITED_INFORMATION` で十分な理由

プロセス名の取得には最小権限の `PROCESS_QUERY_LIMITED_INFORMATION`（0x1000）だけを使います。`QueryFullProcessImageNameW` はこの権限で動き、保護プロセス（protected process）に対して拒否される権限の一覧にも入っていません（広い `PROCESS_QUERY_INFORMATION` は入ります）。アンチチートが嫌うメモリアクセスは別名の独立した権限（`PROCESS_VM_READ` / `WRITE` / `OPERATION`）で、クエリ専用のハンドルにはその能力が一切付きません。BattlEye のカーネルドライバがゲームへのハンドルから剥がすのもメモリ権で、クエリ専用ハンドルは対象外と解析されています。

「クエリ専用ハンドルはアンチチートのホワイトリストに入っている」という説もありますが、出典をたどると AVG アンチウイルスの自己防衛ドライバの解析で、別物でした。ここでは「メモリ権を持たない最小権限であり、これが BAN を招いたという一次情報は見つからない」に留めます。念のため、取得したハンドルは直後に `CloseHandle` して保持時間を最小にしています。

## アンチチートが見ているもの

ユーザーモードでの主な観測経路は次のとおりです（arXiv:2408.00500 ほかの解析より）。

- **ウィンドウの列挙と属性照合**：EAC も BattlEye も可視ウィンドウを列挙し、ウィンドウ名（BattlEye はクラス名・スタイル・サイズも）をチート名と突き合わせます。`SetWindowPos` で動かすこと自体は対象外で、気をつけるべきは自分のウィンドウ名・クラス名です。なお BattlEye には、列挙結果が 2 件未満だと API がフックされたとみなして報告する挙動も報告されていますが、環境要因で起きうる誤検知で、こちらから対策できるものではありません。
- **プロセスの列挙とパス照合**：既知チートの名前・パス・署名のブロックリストと突き合わせます。未知で非注入のツールが名前照合だけで弾かれることは基本的にありません。
- **ハンドルの監視**：BattlEye がサーバへ報告するのは `VM_WRITE` / `VM_READ` 権付きのハンドルに限られ、クエリ専用のハンドルは対象外です。
- **メモリの文字列署名スキャン**：2024 年 11 月、Ricochet が RAM をチート語の固定文字列（`Trigger Bot` など）でスキャンしていたことが悪用され、その文字列を私信で受け取っただけの人が BAN される事件が起きました（後に復旧）。実行ファイルやタイトル、設定、ログにチートを連想させる文字列を埋め込まない理由です。

誤検知の実例は確度を分けて見ます。因果まで確認できているのは上の Ricochet の件（と、Destiny 2 の手動審査ミスによる大量 BAN）です。MW2 / Warzone 2 で RGB 制御ソフトが BAN されたという話は相関の報告どまりで因果は未確認ですが、「ゲーム実行中に、ゲームへ干渉しているように見える動きは避ける」方針の傍証にはします。WoW でマウスホバーのアクティブ化機能が自動化と誤認されたという逸話からは、「プログラムによる急なフォーカス・アクティブ化の連発は自動化に見えうる」と読み取れます。このアプリは `SWP_NOACTIVATE` でアクティブ化しないので、この型には当てはまりません。

## ゲームのウィンドウを判定する（介入可否の関門）

ゲームにはそもそもウィンドウスナップの需要がないので、迷ったら触らないのが正解です。能動的なウィンドウ操作（矢印ホットキーの機能 B）は必ず `win::guard::should_intervene` を通し、次のどれかに当たるウィンドウには一切触れません。ハンドルを開かずに済む条件を前に置き、名前リストへの依存は最後に回します。

1. **フルスクリーン・排他状態**（`skip_when_fullscreen`）：`SHQueryUserNotificationState` が `QUNS_RUNNING_D3D_FULL_SCREEN`（排他 D3D）か `QUNS_BUSY`（フルスクリーンアプリ全般）を返すかを見ます。ただしフルスクリーン最適化・DX12・ボーダーレスでは排他のゲームでも `QUNS_BUSY` しか返らないことがあり、この API だけでは確定できないため、「対象ウィンドウがモニタ全体を覆う矩形か」も併用します。
2. **ウィンドウのスタイル**（`skip_non_tileable`）：普通のデスクトップアプリはタイトルバー（`WS_CAPTION`）かリサイズ枠（`WS_THICKFRAME`）を持ち、排他・ボーダーレスのゲーム画面やオーバーレイはどちらも持たないことが多い、という特徴で絞ります。ブロックリストではないので、名前リストに無い未知のゲームも避けられます。判定はわざと緩め（どちらか一方でも持てば対象）にして、自前タイトルバーの最近のアプリ（Electron 系など）を取りこぼさないようにしています。判定ロジックは `window_style::is_tileable` に置き、Win32 非依存で単体テストしています。
3. **プロセス名で除外**（最後の保険）：既定で代表的な競技系ゲームの実行ファイルを除外リストに同梱しています（`[exclusions]`、[configuration.md](configuration.md)）。この段で初めて `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` を使うため、1 と 2 を前に置くほどゲームのプロセスへハンドルを開く回数が減ります。

名前リストだけに頼ると、知っているゲームしか避けられず、適用漏れも起きます。実際 PowerToys の FancyZones には、除外アプリの設定がドラッグでは効くのにショートカット経路では素通りする不具合がありました（0.15.0 で修正）。介入経路が必ず同じ関門を通る構成にしているのはこのためです。昇格ウィンドウだけは事前の判定が当てにならないため、ここでは弾かず、`SetWindowPos` の失敗（ACCESS_DENIED）をログに残して握り潰します（非昇格のまま動かす前提を保つため）。

## 文字列衛生

Ricochet 型のメモリ文字列スキャンや、EAC / BattlEye のウィンドウ名照合にひっかからないための運用ルールです。実行ファイル名、ウィンドウのクラス名やタイトル、ログ出力、設定キーに、チートを連想させる語（`trigger`、`aimbot`、`cheat`、`inject`、`hack` など）を入れません。可視ウィンドウを増やすときも同じ命名規則を守ります。

## 出典

反証検証で確証が取れた主なものです（2024〜2026 時点）。

- Microsoft Learn: Process Security and Access Rights（`PROCESS_QUERY_LIMITED_INFORMATION` の位置づけと保護プロセスの拒否一覧）
- Microsoft Learn: `QueryFullProcessImageNameW` / `GetWindowThreadProcessId` / `SHQueryUserNotificationState` / `QUERY_USER_NOTIFICATION_STATE`
- Dorner & Klausner, "If It Looks Like a Rootkit and Deceives Like a Rootkit"（arXiv:2408.00500, ARES 2024） / secret.club "BattlEye Anti-Cheat: Analysis"（2019） / reversingthread.info "BattlEye window detection"（2024） / "Battling The Eye"（ACM, 2025）
- TechCrunch（2024-11-07）: Ricochet の文字列署名悪用による誤検知
- Bungie Help: Destiny 2 とサードパーティアプリ／キャプチャの扱い
- Microsoft PowerToys: FancyZones の除外アプリが `Win` + 矢印で素通りした不具合（0.15.0 で修正）
