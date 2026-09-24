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

    // 通常の `==` は不一致が見つかった時点で早期リターンするため、
    // 資格情報の長さや内容が実行時間差から漏れうる (タイミング攻撃)。
    // このエンドポイントは Cloudflare Access を意図的に使わず Basic 認証のみが
    // 防御線のため、ユーザー名・パスワードの双方を必ず最後まで比較したうえで
    // ビット AND で結合する。`&&` に「簡略化」しないこと。
    let user_ok = ct_eq(user, username);
    let pass_ok = ct_eq(pass, password);
    user_ok & pass_ok
}

/// 早期リターンなしで 2 つの文字列を比較する。
///
/// 長さの違いは早期に分かってしまうが、これは標準的な定数時間比較ライブラリ
/// でも同様であり許容する。守りたいのは内容差による時間差の漏えい。
fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
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
