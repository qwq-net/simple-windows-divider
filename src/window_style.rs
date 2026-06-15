//! ウィンドウスタイルから「スナップ対象にしてよい普通のウィンドウか」を判定する（Win32 非依存）。
//!
//! スタイルビットは Win32 と同じ数値を自前定義しているため、Windows 側は `GetWindowLongPtrW(GWL_STYLE)`
//! の戻り値をそのまま渡せる。本モジュール自体は `windows` クレートに依存しない。

// --- Win32 と同じ数値のウィンドウスタイル（WS_*） ---
const WS_CHILD: u32 = 0x4000_0000;
const WS_CAPTION: u32 = 0x00C0_0000; // WS_BORDER | WS_DLGFRAME（タイトルバー）
const WS_THICKFRAME: u32 = 0x0004_0000; // サイズ変更枠

/// `style`（`GWL_STYLE` の値）が「スナップ／グリッド配置の対象にしてよい普通のアプリウィンドウ」を表すか。
///
/// 真を返すのは、子ウィンドウ（`WS_CHILD`）でなく、かつタイトルバー（`WS_CAPTION`）か
/// リサイズ枠（`WS_THICKFRAME`）の少なくとも一方を持つ場合。
///
/// 偽になるのは、タイトルバーもリサイズ枠も持たない素のポップアップ（ボーダーレス全画面ゲーム・
/// オーバーレイ・HUD・スプラッシュ等）や子ウィンドウ。誤って正規アプリを除外しないよう判定は緩めにし、
/// 排他／全画面ゲームの主な捕捉は呼び出し側のフルスクリーン判定に任せる前提。
pub fn is_tileable(style: u32) -> bool {
    style & WS_CHILD == 0 && style & (WS_CAPTION | WS_THICKFRAME) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const WS_POPUP: u32 = 0x8000_0000;
    const WS_VISIBLE: u32 = 0x1000_0000;
    const WS_SYSMENU: u32 = 0x0008_0000;

    #[test]
    fn normal_app_window_is_tileable() {
        // タイトルバー＋リサイズ枠を持つ一般的なウィンドウ。
        let style = WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_VISIBLE;
        assert!(is_tileable(style));
    }

    #[test]
    fn caption_only_dialog_is_tileable() {
        // 固定サイズのダイアログ（タイトルバーのみ）も対象にする。
        assert!(is_tileable(WS_CAPTION | WS_SYSMENU));
    }

    #[test]
    fn thickframe_only_custom_chrome_is_tileable() {
        // 自前タイトルバーの最近のアプリ（リサイズ枠のみ）を取りこぼさない。
        assert!(is_tileable(WS_THICKFRAME | WS_POPUP | WS_VISIBLE));
    }

    #[test]
    fn bare_popup_is_not_tileable() {
        // タイトルバーもリサイズ枠も無い素のポップアップ（ボーダーレス全画面・オーバーレイ等）。
        assert!(!is_tileable(WS_POPUP | WS_VISIBLE));
    }

    #[test]
    fn child_window_is_not_tileable() {
        // 子ウィンドウは、見かけ上スタイルを持っていても対象外。
        assert!(!is_tileable(WS_CHILD | WS_CAPTION | WS_THICKFRAME | WS_VISIBLE));
    }
}
