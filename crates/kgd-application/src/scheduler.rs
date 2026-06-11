//! 定期実行ジョブの抽象と日報向けジョブ実装。

use std::sync::Arc;

use anyhow::Result;

use super::RunDiaryMaintenance;

/// 定期実行されるジョブ。
///
/// ランナーは一定間隔で全ジョブの [`ScheduledJob::tick`] を呼ぶだけの最小実装とし、
/// 「今実行すべきか」の判定は各ジョブが内部で行う。
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait ScheduledJob: Send + Sync {
    /// ログ出力用のジョブ名。
    fn name(&self) -> &'static str;

    /// 1 tick 分の処理。失敗してもランナーは継続する。
    async fn tick(&self) -> Result<()>;
}

/// 日報スレッドの自動クローズ確認を行うジョブ。
pub struct AutoCloseJob(pub Arc<RunDiaryMaintenance>);

#[async_trait::async_trait]
impl ScheduledJob for AutoCloseJob {
    fn name(&self) -> &'static str {
        "auto_close"
    }

    async fn tick(&self) -> Result<()> {
        self.0.check_auto_close().await
    }
}

/// 直近の日報スレッドの毎時同期を行うジョブ。
pub struct HourlySyncJob(pub Arc<RunDiaryMaintenance>);

#[async_trait::async_trait]
impl ScheduledJob for HourlySyncJob {
    fn name(&self) -> &'static str {
        "hourly_sync"
    }

    async fn tick(&self) -> Result<()> {
        self.0.check_hourly_sync().await
    }
}
