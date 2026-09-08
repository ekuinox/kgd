//! 現在の日報エントリを引く共通ロジック。
//!
//! 早出し（一日の始まりより前に次の日報日を開始すること）で作られた日報を
//! 転記・再開の対象にするため、複数のユースケースで同じ引き方を共有する。

use anyhow::Result;
use chrono::{DateTime, Utc};

use kgd_domain::{DiaryCalendar, DiaryEntry};

use crate::ports::DiaryRepository;

/// 現在の日報エントリを取得する。
///
/// 早出しで次の日報日の日報が既にあればそちらを返し、無ければ今の日報日のものを返す。
/// どちらも無ければ `None` を返す。
pub(crate) async fn find_current_entry(
    repo: &dyn DiaryRepository,
    calendar: &DiaryCalendar,
    now: DateTime<Utc>,
) -> Result<Option<DiaryEntry>> {
    if let Some(next_day) = calendar.early_next_day(now)
        && let Some(entry) = repo.get_by_date(next_day).await?
    {
        return Ok(Some(entry));
    }

    repo.get_by_date(calendar.today(now)).await
}
