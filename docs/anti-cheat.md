# アンチチート安全性

このアプリで一番気をつけているのは、ゲームと一緒に使っても主要なアンチチート（EAC / BattlEye / Vanguard / Ricochet など）に誤検知されないことです。ここでは、その方針と、なぜ安全だと判断しているのかの根拠、そして「触ってはいけないゲームのウィンドウ」を取りこぼさず避けるための考え方をまとめます。技術的な裏づけは、末尾の[出典](#出典)に挙げた一次情報（Microsoft Learn）と査読論文・解析記事で確認しています。

## 基本原則：境界線は「注入」

調べた範囲では、アンチチートが嫌うのは一貫してゲームへのコード注入とメモリ操作です。ゲーム開発元の Bungie も、Destiny 2 で弾くのは「サードパーティがクライアントにコードを挿入する手法であり、それがチーターの手口と同じだから」と説明しています。逆に、外からの画面・ウィンドウキャプチャやホットキー録画、常駐の監視ツールは許容するとしています。

このアプリは、ゲームのプロセス空間・メモリ・入力経路には一切踏み込みません。やるのは「自分のプロセス内で OS のイベントを受け取り、`SetWindowPos` で外からウィンドウを動かす」ことだけです。次の API や手法は使いません。

- 低レベル入力フック（`WH_*_LL`）
- 他プロセスへの DLL インジェクション（`WINEVENT_INCONTEXT` を含む）
- 他プロセスのメモリ操作（`ReadProcessMemory` / `WriteProcessMemory` と、それに必要な `PROCESS_VM_*` 権）
- カーネルドライバ
- 入力の合成・ブロードキャスト（`SendInput` でキーやマウスを送る類）

## 使う API とその根拠

能動的に使う Win32 API は、どれも正規ツールで一般的なものに絞っています。

| API | 安全側と判断する根拠 |
|---|---|
| `RegisterHotKey` | フック不要のホットキー登録。入力を合成も傍受もしません。トリガーボット検知が見るのは入力のタイミング（人間の反応下限 150〜200ms など）で、ホットキー登録そのものは対象外です。 |
| `SetWinEventHook`（`WINEVENT_OUTOFCONTEXT`） | out-of-context では、コールバックが呼び出し側（自分）のプロセス内で動き、イベント発生元のプロセスには DLL を一切マップしません。注入が起きるのは `WINEVENT_INCONTEXT` の側だけです。Microsoft 自身も、ウィンドウやフルスクリーンの監視手段としてこの方式を案内しています。 |
| `SetWindowPos` / `ShowWindow` / `EnumWindows` | 外からウィンドウを動かす・列挙するだけで、相手のプロセスには触れません。`SWP_NOACTIVATE` を付けてアクティブ化もしません。 |
| `GetWindowThreadProcessId` | HWND から所有プロセスの PID を得ます。ハンドルは開きません。 |
| `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` + `QueryFullProcessImageNameW` | 実行ファイルのパスを読むだけです。`PROCESS_QUERY_LIMITED_INFORMATION` はメモリ読み書き権を含まない最小権限です（次節）。 |
| `SHQueryUserNotificationState` | フルスクリーン・排他状態を調べる非特権のシェル API。昇格もフックも要りません。 |
| `SHGetPropertyStoreForWindow` | ウィンドウのプロパティ（AUMID）を読むシェル API。対象プロセスを開かず、注入もメモリ操作もしません。非昇格で動きます。学習キーで PWA と通常ブラウザを分けるために使います。 |
| `SetWinEventHook`（`EVENT_SYSTEM_MOVESIZEEND`） | 生成監視と同じ out-of-context 方式で、注入はありません。ユーザーのドラッグ・リサイズ完了時だけ発火し、自分の `SetWindowPos` では発火しません。学習配置からの離脱（所属解除）の判定に使い、ウィンドウは動かしません。 |

逆に使わない API は前節のとおりです。とくに `ReadProcessMemory` / `WriteProcessMemory` と `PROCESS_VM_*` 権、`OpenProcess(PROCESS_QUERY_INFORMATION)` や `PROCESS_ALL_ACCESS` のような広い権限は、便利でも持ち込みません。

### `PROCESS_QUERY_LIMITED_INFORMATION` で十分な理由

プロセス名の取得には最小権限の `PROCESS_QUERY_LIMITED_INFORMATION`（0x1000）だけを使います。これを安全側と判断する根拠は次のとおりです。

- `QueryFullProcessImageNameW` は `PROCESS_QUERY_INFORMATION` か `PROCESS_QUERY_LIMITED_INFORMATION` のどちらでも動きます。低いほうの後者で足ります。
- この権限は「`PROCESS_QUERY_INFORMATION` の情報の一部だけにアクセスするため」に導入されたもので、保護プロセス（protected process）に対して拒否される権限の一覧に入っていません。広いほうの `PROCESS_QUERY_INFORMATION` は拒否一覧に含まれます。
- アンチチートが嫌うメモリアクセス権は、別名の独立した権限です（`ReadProcessMemory` 用の `PROCESS_VM_READ`、`WriteProcessMemory` 用の `PROCESS_VM_WRITE`、`PROCESS_VM_OPERATION`）。`PROCESS_QUERY_LIMITED_INFORMATION` だけのハンドルには、これらの能力は一切付きません。Microsoft も「操作に必要な最小限の権限だけを要求せよ」と勧めています。
- BattlEye のカーネルドライバ（BEDaisy.sys）は、ゲームへのハンドルから `PROCESS_VM_READ` / `WRITE` / `OPERATION` を剥がす実装が解析されています。剥がす対象はメモリ権で、クエリ専用のハンドルは対象外です。

ひとつ注意しておくと、「クエリ専用ハンドルはアンチチートのホワイトリストに入っているから安全」という説がネット上にあります。ただし出典をたどるとアンチチートではなく AVG アンチウイルスの自己防衛ドライバの解析で、別物でした。ここでは「`PROCESS_QUERY_LIMITED_INFORMATION` はメモリ権を持たず、保護プロセスの拒否一覧にも載らない最小権限であり、クエリ専用ハンドルが BAN を招くという一次情報は見つからない」という程度に留めます。念のため、取得したハンドルは直後に `CloseHandle` して保持時間を最小にしています。

## アンチチートが見ているもの

正規ツールが弾かれないようにするには、アンチチートがユーザーモードで何を観測しているかを知っておく必要があります。最新の解析（arXiv:2408.00500 ほか）からわかっている主な経路を挙げます。

ウィンドウの列挙と属性照合は、EAC も BattlEye も行います。可視ウィンドウを列挙してウィンドウ名を取り、チート名に一致したものを報告します。BattlEye はさらにクラス名やスタイル（`TOPMOST` などのフラグ）、サイズも見ます。`SetWindowPos` でウィンドウを動かすこと自体は検知の対象ではありません。むしろ気をつけるべきは、このアプリ自身のウィンドウ名やクラス名がチートを連想させないことです。

BattlEye には、ウィンドウ列挙の結果が 2 件未満だと「自分の API がフックされて偽の結果を返された」とみなして報告する挙動も報告されています。これは特定のアプリの動作というより環境要因で起きうる誤検知で、こちらから対策できるものではありませんが、頭には入れておきます。

プロセスの列挙とパス照合もよく使われます。`NtQuerySystemInformation` などで全プロセスを列挙し、既知のチートツールを名前・パス・署名のブロックリストと突き合わせます。逆に言えば、未知で非注入のツールが名前照合だけで弾かれることは基本的にありません。

ハンドルの監視もあります。ゲームへ開かれたハンドルを見るのですが、BattlEye がサーバへ報告するのは、一次情報（secret.club）によると `VM_WRITE` / `VM_READ` 権の付いたハンドルに限られます。クエリ専用のハンドルは、この報告の対象外です。

実害が出た例として、メモリの文字列署名スキャンがあります。2024 年 11 月、Call of Duty の Ricochet が RAM をチート語の固定文字列（たとえば `Trigger Bot`）でスキャンしていたことが悪用され、その文字列を私信で送るだけで受け取った側が BAN される事件が起きました（Activision は後に復旧）。文脈を見ない署名検知は、こういう巻き込みを生みます。ここから得る教訓は、実行ファイルやウィンドウタイトル、設定値、ログにチートを連想させる文字列を埋め込まないことです。

### 誤検知の実例は確度を分けて見る

誤検知の事例はネットに多いのですが、因果がはっきりしているものと、そうでないものを混ぜないようにします。

確認が取れているのは、さきほどの Ricochet の文字列署名による巻き込みです。Destiny 2 で過去に起きた大量 BAN も、別系統の挙動検知での手動審査ミスだったと整理されています。

相関の報告どまりで、因果が確認できていないものもあります。MW2 / Warzone 2 で RGB 制御ソフト（Razer Synapse、Corsair iCUE、MSI Afterburner、Logitech G-Hub など）が「unauthorized software」として BAN されたという話がそれです。複数の報道が「RGB ソフトが原因と断定はできない」と明記していますし、「Razer のクラウドからのプロファイル取得が注入に見えた」という説明も、公式ではなくフォーラムの憶測です。前提にはしませんが、「ゲーム実行中に、ゲームへ干渉しているように見える動きは避ける」という方針の傍証にはします。

単発の逸話として、WoW の例もあります。Windows の「マウスを重ねるとウィンドウをアクティブ化する」アクセシビリティ機能が、Warden に入力ブロードキャストや自動化と誤認され、マルチボックス扱いで BAN されたという報告です。Blizzard は「マルチボックスを自動化・効率化するソフト」を罰則の対象とし、特定ツールの安全性は明言しない方針をとっています。ここからは「プログラムによる急なフォーカスやアクティブ化の連発は自動化に見えうる」と読み取れます。このアプリは `SWP_NOACTIVATE` でアクティブ化しないので、この型には当てはまりません。

## ゲームのウィンドウを判定する

ゲームにはそもそもウィンドウスナップの需要がありません。なので、迷ったら触らないのが正解です。判定は三段構えにして、どれかに当たれば介入しません。ハンドルを開かずに済む条件ほど前に置き、名前リストへの依存は最後に回します。

1. フルスクリーン・排他状態か（プロセスを開かずに判定）
   `SHQueryUserNotificationState` が `QUNS_RUNNING_D3D_FULL_SCREEN`（排他 D3D）か `QUNS_BUSY`（フルスクリーンアプリ全般）を返すかを見ます。ただしこの API には限界があります。フルスクリーン最適化や DX12、ボーダーレスでは、排他フルスクリーンのゲームでも `QUNS_RUNNING_D3D_FULL_SCREEN` ではなく `QUNS_BUSY` が返ることがあり、しかも `QUNS_BUSY` はゲーム以外のフルスクリーンも含みます。つまりこの API だけではフルスクリーンの「ゲーム」を確定できません。そこで「対象ウィンドウがモニタ全体を覆う矩形か」も併用します。ボーダーレスフルスクリーンは、この矩形条件で拾えます。

2. ウィンドウのスタイルで判定（プロセスを開かず・名前にも依存しない）
   普通のデスクトップアプリのウィンドウは、タイトルバー（`WS_CAPTION`）かリサイズ枠（`WS_THICKFRAME`）を持ちます。排他・ボーダーレスのゲーム画面やオーバーレイ、HUD はどちらも持たないことが多く、Microsoft もフルスクリーン判定の手段として「ウィンドウ矩形がモニタと一致し、かつ `WS_OVERLAPPEDWINDOW` がないこと」を挙げています。このアプリは `skip_non_tileable`（既定で有効）として、タイトルバーもリサイズ枠も持たない素のウィンドウには介入しません。ブロックリストではなく「普通のアプリウィンドウか」という特徴で絞るので、名前リストにない未知のゲームも避けられます。判定はわざと緩め（どちらか一方でも持てば対象）にして、自前タイトルバーの最近のアプリ（Electron 系など）を取りこぼさないようにしています。判定ロジック自体は `window_style::is_tileable` に置き、Win32 非依存で単体テストしています。

3. プロセス名で除外（最後の保険）
   既定で代表的な競技系ゲームの実行ファイルを除外リストに同梱しています（`config.toml` の `[exclusions]`、[configuration.md](configuration.md)）。アンチチートが厳しいタイトルを明示的に外す保険で、1 と 2 を補います。この段で初めて `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` を使うため、1 と 2 を前に置くほど、ゲームのプロセスへハンドルを開く回数が減ります。

名前リストだけに頼るのは危ういです。知っているゲームしか避けられませんし、適用漏れも起きます。実際 Microsoft PowerToys の FancyZones では、除外アプリの設定がドラッグでは効くのに `Win` + 矢印のショートカット経路では素通りし、除外したはずのアプリがゾーンに割り当てられる不具合がありました（0.15.0 で修正）。除外やフルスクリーンの判定は、すべての介入経路（ホットキーの機能 B と、イベントの機能 C の両方）で漏れなく通すのが肝心です。

## 介入可否の関門（実装）

能動的なウィンドウ操作は、機能 B も機能 C も、必ず `win::guard::should_intervene` を通します。次のいずれかに当たるウィンドウには一切触れません。

- 無効なウィンドウハンドル
- フルスクリーン・排他状態（`skip_when_fullscreen` が有効なとき）。`SHQueryUserNotificationState` と「モニタ全体を覆う矩形か」の両方で判定します。
- タイトルバーもリサイズ枠も持たないウィンドウ（`skip_non_tileable` が有効なとき）。ボーダーレス全画面やオーバーレイを、プロセスを開かずに避けられます。
- 除外プロセスに含まれる実行ファイルのウィンドウ

順番にも意味があります。ハンドルを開かずに済む条件（無効・フルスクリーン・スタイル）を先に並べ、`OpenProcess` を伴うプロセス名照合を最後にすることで、ゲームのプロセスへハンドルを開く回数を抑えています。機能 B と機能 C が同じ関門を共有するので、片方の経路だけ素通りする FancyZones 型の漏れも起きません。

昇格ウィンドウだけは、事前の判定が当てにならないので、ここでは弾きません。実際に操作して `SetWindowPos` が失敗（ACCESS_DENIED）したら、ログに残して握り潰します。非昇格のまま動かす前提を保つためです。

### 文字列衛生

Ricochet 型のメモリ文字列スキャンや、EAC / BattlEye のウィンドウ名照合にひっかからないための運用ルールです。

実行ファイル名、ウィンドウのクラス名やタイトル、ログ出力、設定キーに、チートを連想させる語（`trigger`、`aimbot`、`cheat`、`inject`、`hack` など）を入れません。いまのところ自前のウィンドウはメッセージ専用ウィンドウとトレイだけですが、可視ウィンドウを増やすときも同じ命名規則を守ります。

## 出典

反証検証で確証が取れた主なものです（2024〜2026 時点）。

- Microsoft Learn: `SetWinEventHook` / Out-of-Context Hook Functions（`WINEVENT_OUTOFCONTEXT` は注入なし）
- Microsoft Learn: Process Security and Access Rights（`PROCESS_QUERY_LIMITED_INFORMATION` の位置づけと保護プロセスの拒否一覧）
- Microsoft Learn: `QueryFullProcessImageNameW` / `GetWindowThreadProcessId` / `SHQueryUserNotificationState` / `QUERY_USER_NOTIFICATION_STATE`
- Raymond Chen, "Using accessibility to monitor windows as they come and go"（out-of-context フックの作法）
- Dorner & Klausner, "If It Looks Like a Rootkit and Deceives Like a Rootkit"（arXiv:2408.00500, ARES 2024） / secret.club "BattlEye Anti-Cheat: Analysis"（2019） / reversingthread.info "BattlEye window detection"（2024） / "Battling The Eye"（ACM, 2025）
- TechCrunch（2024-11-07）: Ricochet の文字列署名悪用による誤検知
- Bungie Help: Destiny 2 とサードパーティアプリ／キャプチャの扱い
- Microsoft PowerToys: FancyZones の除外アプリが `Win` + 矢印で素通りした不具合（0.15.0 で修正）
