//! Basic 認証ヘッダの検証。

use base64::{Engine as _, engine::general_purpose::STANDARD};

/// Basic 認証ヘッダが設定の資格情報と一致するか判定する。
///
/// 解釈に失敗したヘッダはすべて不一致として扱う。
pub fn is_valid_basic_auth(header: Option<&str>, username: &str, password: &str) -> bool {
    let Some(encoded) = header.and_then(|value| value.strip_prefix("Basic ")) else {
        return false;
    };
    let Ok(decoded) = STANDARD.decode(encoded.trim()) else {
        return false;
    };
    let Ok(text) = String::from_utf8(decoded) else {
        return false;
    };
    let Some((user, pass)) = text.split_once(':') else {
        return false;
    };

    user == username && pass == password
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 正しい資格情報の Basic ヘッダを受理することを確認する。
    #[test]
    fn is_valid_basic_auth_accepts_matching_credentials() {
        // "ekuinox:secret" の base64
        let header = "Basic ZWt1aW5veDpzZWNyZXQ=";
        assert!(is_valid_basic_auth(Some(header), "ekuinox", "secret"));
    }

    /// パスワードが異なる場合に拒否することを確認する。
    #[test]
    fn is_valid_basic_auth_rejects_wrong_password() {
        let header = "Basic ZWt1aW5veDpzZWNyZXQ=";
        assert!(!is_valid_basic_auth(Some(header), "ekuinox", "other"));
    }

    /// ヘッダが無い場合に拒否することを確認する。
    #[test]
    fn is_valid_basic_auth_rejects_missing_header() {
        assert!(!is_valid_basic_auth(None, "ekuinox", "secret"));
    }

    /// Basic 以外のスキームや壊れた base64 を拒否することを確認する。
    ///
    /// 外部に公開する口であり、解釈に失敗したものは通さない。
    #[test]
    fn is_valid_basic_auth_rejects_malformed_header() {
        assert!(!is_valid_basic_auth(
            Some("Bearer token"),
            "ekuinox",
            "secret"
        ));
        assert!(!is_valid_basic_auth(
            Some("Basic !!!!"),
            "ekuinox",
            "secret"
        ));
        assert!(!is_valid_basic_auth(Some("Basic"), "ekuinox", "secret"));
    }
}
