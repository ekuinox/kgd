//! OwnTracks から受け取るメッセージのドメイン型と変換。
//!
//! IO を伴わない純粋関数として実装し、JSON は引数で受ける。

use chrono::{DateTime, TimeZone as _, Utc};
use serde_json::Value;

/// OwnTracks から受け取った 1 件のメッセージ。
///
/// レポートで使う項目は型付きで持ち、元の JSON は `payload` に丸ごと残す。
/// 後から別の項目が必要になったときに再取り込みを不要にするため。
#[derive(Debug, Clone, PartialEq)]
pub struct OwnTracksMessage {
    /// 端末が名乗るユーザー識別子 (X-Limit-U)
    pub user_id: String,
    /// 端末が名乗るデバイス識別子 (X-Limit-D)
    pub device_id: String,
    /// メッセージ種別 (_type)
    pub msg_type: String,
    /// 端末が位置を取得した時刻 (tst)
    pub tst: Option<DateTime<Utc>>,
    /// 緯度
    pub lat: Option<f64>,
    /// 経度
    pub lon: Option<f64>,
    /// 水平精度 (メートル)
    pub acc: Option<i32>,
    /// 高度 (メートル)
    pub alt: Option<i32>,
    /// 速度 (km/h)
    pub vel: Option<i32>,
    /// バッテリー残量 (%)
    pub batt: Option<i16>,
    /// 送信のきっかけ (t)
    pub trigger_type: Option<String>,
    /// モーション判定の先頭要素 (motionactivities[0])
    pub motion: Option<String>,
    /// 受け取った JSON 全体
    pub payload: Value,
}

/// 識別子として安全な文字だけを残す。空になった場合は `fallback` を返す。
pub fn sanitize_identifier(raw: Option<&str>, fallback: &str) -> String {
    let kept: String = raw
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();

    if kept.is_empty() {
        fallback.to_string()
    } else {
        kept
    }
}

/// JSON から [`OwnTracksMessage`] を組み立てる。
///
/// オブジェクトでない、または `_type` を持たない JSON は `None` を返す。
pub fn parse_owntracks_message(
    user_id: &str,
    device_id: &str,
    payload: Value,
) -> Option<OwnTracksMessage> {
    let object = payload.as_object()?;
    let msg_type = object.get("_type")?.as_str()?.to_string();

    let tst = object
        .get("tst")
        .and_then(Value::as_i64)
        .and_then(|seconds| Utc.timestamp_opt(seconds, 0).single());

    let motion = object
        .get("motionactivities")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .and_then(Value::as_str)
        .map(str::to_string);

    Some(OwnTracksMessage {
        user_id: user_id.to_string(),
        device_id: device_id.to_string(),
        msg_type,
        tst,
        lat: object.get("lat").and_then(Value::as_f64),
        lon: object.get("lon").and_then(Value::as_f64),
        acc: rounded_i32(object.get("acc")),
        alt: rounded_i32(object.get("alt")),
        vel: rounded_i32(object.get("vel")),
        batt: rounded_i32(object.get("batt")).and_then(|v| i16::try_from(v).ok()),
        trigger_type: object.get("t").and_then(Value::as_str).map(str::to_string),
        motion,
        payload: payload.clone(),
    })
}

/// 数値を i32 に丸める。整数でも小数でも受け取れるようにする。
fn rounded_i32(value: Option<&Value>) -> Option<i32> {
    let number = value?.as_f64()?;
    if number.is_finite() {
        Some(number.round() as i32)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// location メッセージの主要な項目が型付きのフィールドへ取り出され、
    /// payload には元の JSON がそのまま残ることを確認する。
    #[test]
    fn parse_owntracks_message_extracts_location_fields() {
        let payload = json!({
            "_type": "location",
            "tst": 1789266896,
            "lat": 34.710452,
            "lon": 135.471914,
            "acc": 8,
            "alt": 2,
            "vel": 4,
            "batt": 89,
            "t": "t",
            "motionactivities": ["walking"]
        });

        let message =
            parse_owntracks_message("ekuinox", "ohtori", payload.clone()).expect("should parse");

        assert_eq!(message.user_id, "ekuinox");
        assert_eq!(message.device_id, "ohtori");
        assert_eq!(message.msg_type, "location");
        assert_eq!(message.tst, Some(Utc.timestamp_opt(1789266896, 0).unwrap()));
        assert_eq!(message.lat, Some(34.710452));
        assert_eq!(message.lon, Some(135.471914));
        assert_eq!(message.acc, Some(8));
        assert_eq!(message.alt, Some(2));
        assert_eq!(message.vel, Some(4));
        assert_eq!(message.batt, Some(89));
        assert_eq!(message.trigger_type, Some("t".to_string()));
        assert_eq!(message.motion, Some("walking".to_string()));
        assert_eq!(message.payload, payload);
    }

    /// tst や位置を持たないメッセージ (waypoints など) でも、
    /// _type さえあれば取りこぼさずに保持できることを確認する。
    #[test]
    fn parse_owntracks_message_accepts_message_without_position() {
        let payload = json!({ "_type": "waypoints", "waypoints": [] });

        let message = parse_owntracks_message("ekuinox", "ohtori", payload).expect("should parse");

        assert_eq!(message.msg_type, "waypoints");
        assert_eq!(message.tst, None);
        assert_eq!(message.lat, None);
    }

    /// _type を持たない JSON は取り込まないことを確認する。
    ///
    /// 種別が分からないとレポートの集計対象を決められないため。
    #[test]
    fn parse_owntracks_message_rejects_payload_without_type() {
        assert!(parse_owntracks_message("u", "d", json!({ "lat": 1.0 })).is_none());
    }

    /// 配列やスカラなどオブジェクトでない JSON を弾くことを確認する。
    #[test]
    fn parse_owntracks_message_rejects_non_object() {
        assert!(parse_owntracks_message("u", "d", json!([1, 2, 3])).is_none());
    }

    /// acc が小数で届いても整数へ丸めて取り込むことを確認する。
    ///
    /// OwnTracks は端末やバージョンによって精度を小数で送ることがあるため。
    #[test]
    fn parse_owntracks_message_rounds_fractional_accuracy() {
        let payload = json!({ "_type": "location", "acc": 12.6 });

        let message = parse_owntracks_message("u", "d", payload).expect("should parse");

        assert_eq!(message.acc, Some(13));
    }

    /// 識別子に使えない文字を落とし、空になった場合は代替値を使うことを確認する。
    ///
    /// ヘッダの値がそのままテーブルへ入るため、制御文字や記号を持ち込まない。
    #[test]
    fn sanitize_identifier_strips_unsafe_characters() {
        assert_eq!(sanitize_identifier(Some("ekuinox"), "unknown"), "ekuinox");
        assert_eq!(sanitize_identifier(Some("oh/tori\n"), "unknown"), "ohtori");
        assert_eq!(sanitize_identifier(Some("///"), "unknown"), "unknown");
        assert_eq!(sanitize_identifier(None, "unknown"), "unknown");
    }
}
