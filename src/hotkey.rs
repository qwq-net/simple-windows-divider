//! ホットキーの中立ドメイン型と文字列パース（Win32 非依存）。
//!
//! 仮想キーコード（VK）と修飾フラグは Win32 と同じ数値を自前定義しているため、Windows 側は
//! `RegisterHotKey` にそのまま渡せる。本モジュール自体は `windows` クレートに依存しない。

use std::fmt;

// --- Win32 と同じ数値の修飾フラグ（MOD_*）。RegisterHotKey にそのまま渡せる ---
const MOD_ALT: u32 = 0x0001;
const MOD_CONTROL: u32 = 0x0002;
const MOD_SHIFT: u32 = 0x0004;
const MOD_WIN: u32 = 0x0008;

/// 修飾キーの集合。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

impl Modifiers {
    /// Win32 の `RegisterHotKey` 用の修飾ビット（`MOD_CONTROL | MOD_ALT | …`）を返す。
    /// 立っているフラグだけを OR する。どれも立っていなければ 0。
    pub fn win32_bits(&self) -> u32 {
        let mut bits = 0;
        if self.ctrl {
            bits |= MOD_CONTROL;
        }
        if self.alt {
            bits |= MOD_ALT;
        }
        if self.shift {
            bits |= MOD_SHIFT;
        }
        if self.win {
            bits |= MOD_WIN;
        }
        bits
    }
}

/// 1 つのホットキー（修飾の組み合わせ＋単一の主キー）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hotkey {
    pub mods: Modifiers,
    /// Win32 仮想キーコード（例: `VK_LEFT` = 0x25, `A` = 0x41）。
    pub vk: u16,
}

/// [`parse`] の失敗理由。
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ParseError {
    /// 実質的に空（トークンが 1 つも無い）。
    Empty,
    /// 修飾でも既知のキー名でもないトークン。元の表記を保持する。
    UnknownToken(String),
    /// 修飾だけで主キーが無い。
    NoKey,
    /// 主キーが 2 つ以上指定された。
    MultipleKeys,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => write!(f, "empty hotkey string"),
            ParseError::UnknownToken(t) => write!(f, "unknown hotkey token: {t}"),
            ParseError::NoKey => write!(f, "hotkey has modifiers but no main key"),
            ParseError::MultipleKeys => write!(f, "hotkey specifies more than one main key"),
        }
    }
}

impl std::error::Error for ParseError {}

/// `"Ctrl+Alt+Left"` のようなホットキー文字列を [`Hotkey`] に解釈する。
///
/// - トークンは `+` 区切り、各トークンの前後空白は無視、**大文字小文字を区別しない**。
/// - 修飾の別名: `Ctrl`/`Control`、`Alt`、`Shift`、`Win`/`Windows`/`Meta`/`Super`/`Cmd`。順不同。
/// - 主キーは厳密に 1 つ必要。1 文字英字（`A`→0x41）・数字（`1`→0x31）・`F1`..`F24`・
///   矢印（`Left`/`Right`/`Up`/`Down`）・`Enter`(=`Return`)・`Space`/`Tab`/`Esc`/`Home`/`End`/
///   `Delete`(=`Del`)/`Insert`(=`Ins`)/`PageUp`(=`PgUp`)/`PageDown`(=`PgDn`)/`Backspace` を解釈。
/// - エラー: 実質空 → [`ParseError::Empty`]、未知トークン → [`ParseError::UnknownToken`]、
///   主キー無し → [`ParseError::NoKey`]、主キー複数 → [`ParseError::MultipleKeys`]。
///
/// ```
/// use windows_divider::hotkey::{parse, Modifiers};
/// let hk = parse("ctrl+ALT+Left").unwrap();
/// assert_eq!(hk.mods, Modifiers { ctrl: true, alt: true, shift: false, win: false });
/// assert_eq!(hk.vk, 0x25); // VK_LEFT
/// ```
pub fn parse(s: &str) -> Result<Hotkey, ParseError> {
    let mut mods = Modifiers::default();
    let mut vk: Option<u16> = None;
    let mut any = false;

    for raw in s.split('+') {
        let tok = raw.trim();
        if tok.is_empty() {
            continue;
        }
        any = true;
        let up = tok.to_ascii_uppercase();
        match up.as_str() {
            "CTRL" | "CONTROL" => mods.ctrl = true,
            "ALT" => mods.alt = true,
            "SHIFT" => mods.shift = true,
            "WIN" | "WINDOWS" | "META" | "SUPER" | "CMD" => mods.win = true,
            _ => match key_to_vk(&up) {
                Some(_) if vk.is_some() => return Err(ParseError::MultipleKeys),
                Some(code) => vk = Some(code),
                None => return Err(ParseError::UnknownToken(tok.to_string())),
            },
        }
    }

    if !any {
        return Err(ParseError::Empty);
    }
    match vk {
        Some(vk) => Ok(Hotkey { mods, vk }),
        None => Err(ParseError::NoKey),
    }
}

/// 大文字化済みトークンを Win32 仮想キーコードに対応付ける。修飾・未知なら `None`。
fn key_to_vk(up: &str) -> Option<u16> {
    if up.len() == 1 {
        let c = up.as_bytes()[0];
        if c.is_ascii_alphabetic() || c.is_ascii_digit() {
            return Some(c as u16); // 'A'..'Z' = 0x41.., '0'..'9' = 0x30..
        }
    }
    if let Some(n) = up.strip_prefix('F').and_then(|d| d.parse::<u16>().ok()) {
        if (1..=24).contains(&n) {
            return Some(0x70 + (n - 1)); // VK_F1 = 0x70
        }
    }
    let vk = match up {
        "LEFT" => 0x25,
        "RIGHT" => 0x27,
        "UP" => 0x26,
        "DOWN" => 0x28,
        "ENTER" | "RETURN" => 0x0D,
        "SPACE" => 0x20,
        "TAB" => 0x09,
        "ESC" | "ESCAPE" => 0x1B,
        "BACKSPACE" | "BACK" => 0x08,
        "DELETE" | "DEL" => 0x2E,
        "INSERT" | "INS" => 0x2D,
        "HOME" => 0x24,
        "END" => 0x23,
        "PAGEUP" | "PGUP" => 0x21,
        "PAGEDOWN" | "PGDN" => 0x22,
        _ => return None,
    };
    Some(vk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win32_bits_ors_set_flags() {
        let m = Modifiers { ctrl: true, alt: true, shift: false, win: false };
        assert_eq!(m.win32_bits(), MOD_CONTROL | MOD_ALT);
        assert_eq!(Modifiers::default().win32_bits(), 0);
        let all = Modifiers { ctrl: true, alt: true, shift: true, win: true };
        assert_eq!(all.win32_bits(), MOD_ALT | MOD_CONTROL | MOD_SHIFT | MOD_WIN);
    }

    #[test]
    fn parses_ctrl_alt_arrow() {
        let hk = parse("Ctrl+Alt+Left").unwrap();
        assert_eq!(hk.mods, Modifiers { ctrl: true, alt: true, shift: false, win: false });
        assert_eq!(hk.vk, 0x25);
    }

    #[test]
    fn is_case_insensitive_and_trims() {
        assert_eq!(parse("  ctrl + ALT + left  ").unwrap(), parse("Ctrl+Alt+Left").unwrap());
    }

    #[test]
    fn order_independent() {
        assert_eq!(parse("Alt+Ctrl+Right").unwrap(), parse("Ctrl+Alt+Right").unwrap());
    }

    #[test]
    fn win_aliases() {
        for s in ["Win+Up", "Meta+Up", "Super+Up", "Cmd+Up", "Windows+Up"] {
            let hk = parse(s).unwrap();
            assert!(hk.mods.win, "{s} should set win");
            assert_eq!(hk.vk, 0x26);
        }
    }

    #[test]
    fn letters_digits_and_fkeys() {
        assert_eq!(parse("Ctrl+Shift+A").unwrap().vk, 0x41);
        assert_eq!(parse("Ctrl+1").unwrap().vk, 0x31);
        assert_eq!(parse("Ctrl+F5").unwrap().vk, 0x74); // VK_F1(0x70)+4
        assert_eq!(parse("F24").unwrap().vk, 0x87);
    }

    #[test]
    fn enter_return_alias() {
        assert_eq!(parse("Ctrl+Alt+Enter").unwrap().vk, parse("Ctrl+Alt+Return").unwrap().vk);
        assert_eq!(parse("Enter").unwrap().vk, 0x0D);
    }

    #[test]
    fn errors() {
        assert_eq!(parse(""), Err(ParseError::Empty));
        assert_eq!(parse("   "), Err(ParseError::Empty));
        assert_eq!(parse("Ctrl+Alt"), Err(ParseError::NoKey));
        assert_eq!(parse("Ctrl+Foo"), Err(ParseError::UnknownToken("Foo".to_string())));
        assert_eq!(parse("Left+Right"), Err(ParseError::MultipleKeys));
    }
}
