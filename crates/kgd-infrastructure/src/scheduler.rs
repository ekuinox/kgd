//! 登録されたジョブを一定間隔で実行するスケジューラランナー。

use std::{sync::Arc, time::Duration};

use tracing::error;

use kgd_application::ScheduledJob;

/// 一定間隔で登録ジョブを順に tick するランナー。
pub struct Scheduler {
    /// tick の間隔
    interval: Duration,
    /// 登録されたジョブ
    jobs: Vec<Arc<dyn ScheduledJob>>,
}

impl Scheduler {
    /// 新しい Scheduler を作成する。
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            jobs: Vec::new(),
        }
    }

    /// ジョブを登録する。
    pub fn register(&mut self, job: Arc<dyn ScheduledJob>) {
        self.jobs.push(job);
    }

    /// tokio interval のループで全ジョブを定期実行する。
    ///
    /// このメソッドは戻らないため、呼び出し側で spawn すること。
    pub async fn run(self) {
        let mut interval_timer = tokio::time::interval(self.interval);

        loop {
            interval_timer.tick().await;
            run_jobs(&self.jobs).await;
        }
    }
}

/// 全ジョブを 1 tick 分実行する。ジョブが失敗してもほかのジョブは実行する。
async fn run_jobs(jobs: &[Arc<dyn ScheduledJob>]) {
    for job in jobs {
        if let Err(error) = job.tick().await {
            error!(error = %error, job = job.name(), "Scheduled job failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// 常に失敗するテスト用ジョブ。
    struct FailingJob;

    #[async_trait::async_trait]
    impl ScheduledJob for FailingJob {
        fn name(&self) -> &'static str {
            "failing"
        }

        async fn tick(&self) -> anyhow::Result<()> {
            anyhow::bail!("boom")
        }
    }

    /// 実行回数を記録するテスト用ジョブ。
    struct CountingJob(Arc<AtomicUsize>);

    #[async_trait::async_trait]
    impl ScheduledJob for CountingJob {
        fn name(&self) -> &'static str {
            "counting"
        }

        async fn tick(&self) -> anyhow::Result<()> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    /// 先頭の失敗ジョブと後続のカウントジョブを登録して run_jobs を実行し、
    /// 1 つのジョブが失敗しても後続ジョブが実行される (カウントが 1 になる) ことを確認する。
    #[tokio::test]
    async fn run_jobs_continues_after_failure() {
        let count = Arc::new(AtomicUsize::new(0));
        let jobs: Vec<Arc<dyn ScheduledJob>> =
            vec![Arc::new(FailingJob), Arc::new(CountingJob(count.clone()))];

        // 先頭のジョブが失敗しても後続のジョブが実行される
        run_jobs(&jobs).await;

        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
}
