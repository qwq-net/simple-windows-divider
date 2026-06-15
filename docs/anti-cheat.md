# アンチチート安全性

このアプリの設計で最も重視しているのは、ゲームと併用しても主要なアンチチート（EAC / BattlEye など）に弾かれないことです。そのために、文書化済みのユーザーモード Win32 API だけで構成し、次のものは一切使いません。

- 低レベル入力フック（`WH_*_LL`）
- 他プロセスへの DLL インジェクション
- 他プロセスのメモリ操作（`ReadProcessMemory` / `WriteProcessMemory`）
- カーネルドライバ

## 使う API と使わない API

| 使う（正規ツールでも一般的） | 使わない（弾かれる原因になりうる） |
|---|---|
| `RegisterHotKey`（フック不要） | 低レベル入力フック `WH_*_LL` |
| `SetWinEventHook`（out-of-context、注入なし） | 他プロセスへの DLL インジェクション |
| `SetWindowPos` / `EnumWindows` など | `ReadProcessMemory` / `WriteProcessMemory` |
| `QueryFullProcessImageNameW`（限定情報のみ） | カーネルドライバ |

プロセス名の取得は `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` と `QueryFullProcessImageNameW` の範囲に留めています。

## 介入可否の判定

能動的なウィンドウ操作（機能 B・機能 C のどちらも）は、必ず `win::guard::should_intervene` を通します。次のいずれかに当たるウィンドウには一切触れません。

- 無効なウィンドウハンドル。
- フルスクリーン・排他状態（`skip_when_fullscreen` が有効なとき）。判定には `SHQueryUserNotificationState`（全画面 D3D や BUSY 状態）と、「モニタ全体を覆う矩形かどうか」の両方を使います。
- 除外プロセスに含まれる実行ファイルのウィンドウ。既定で競技系のゲームを同梱しています（[configuration.md](configuration.md) の `[exclusions]`）。

昇格ウィンドウは、事前判定が不確実なため、この関門では弾きません。実際に操作したときの `SetWindowPos` の失敗（ACCESS_DENIED）をログに残して握り潰す方針です。
