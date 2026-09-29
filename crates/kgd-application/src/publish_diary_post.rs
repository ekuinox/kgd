//! bot が作った内容を日報日のスレッドと Notion ページへ載せるユースケース。

use std::sync::Arc;

use anyhow::{Context as _, Result, ensure};
use chrono::NaiveDate;
use tracing::{info, warn};

use kgd_domain::{
    DiaryCalendar, DiaryPost, DiaryPostRecord, image_block_json, paragraph_block_json,
};

use super::ports::{Clock, DiaryPostRepository, DiaryRepository, DiscordGateway, NotionApi};

/// 日報への投稿の結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    /// 今回の呼び出しで載せ終えた
    Published,
    /// 以前に載せ終えていた、またはスキップ済みだった
    AlreadyDone,
    /// 日報が無いためスキップとして記録した
    NoDiary,
}

/// bot が作った内容を日報日のスレッドと Notion ページへ載せるユースケース。
///
/// Notion、スレッドの順に載せ、段ごとに完了を記録する。途中で失敗しても、
/// 次の呼び出しで残りの段だけをやり直す。
pub struct PublishDiaryPostUseCase {
    /// 日報リポジトリポート
    diary: Arc<dyn DiaryRepository>,
    /// 投稿の記録ポート
    posts: Arc<dyn DiaryPostRepository>,
    /// Notion API ポート
    notion: Arc<dyn NotionApi>,
    /// Discord ポート
    discord: Arc<dyn DiscordGateway>,
    /// 時刻ポート
    clock: Arc<dyn Clock>,
    /// 日報日の区切り方
    calendar: DiaryCalendar,
}

impl PublishDiaryPostUseCase {
    /// 新しい PublishDiaryPostUseCase を作成する。
    pub fn new(
        diary: Arc<dyn DiaryRepository>,
        posts: Arc<dyn DiaryPostRepository>,
        notion: Arc<dyn NotionApi>,
        discord: Arc<dyn DiscordGateway>,
        clock: Arc<dyn Clock>,
        calendar: DiaryCalendar,
    ) -> Self {
        Self {
            diary,
            posts,
            notion,
            discord,
            clock,
            calendar,
        }
    }

    /// キーの投稿がもう何もしなくてよい状態かを返す。
    ///
    /// 呼び出し側が、重い内容の生成を省くために使う。
    pub async fn is_done(&self, key: &str) -> Result<bool> {
        Ok(self
            .posts
            .get(key)
            .await?
            .is_some_and(|record| record.is_done()))
    }

    /// 投稿を日報日のスレッドと Notion ページへ載せる。
    pub async fn publish(&self, post: DiaryPost) -> Result<PublishOutcome> {
        let record = self.posts.get(&post.key).await?;
        if record.as_ref().is_some_and(DiaryPostRecord::is_done) {
            return Ok(PublishOutcome::AlreadyDone);
        }

        let Some(entry) = self
            .diary
            .get_by_date(self.calendar.start_of(post.date))
            .await?
        else {
            info!(key = %post.key, date = %post.date, "No diary entry for the post, skipping");
            self.posts
                .mark_skipped(&post.key, post.date, self.clock.now())
                .await?;
            return Ok(PublishOutcome::NoDiary);
        };

        if !record.as_ref().is_some_and(DiaryPostRecord::notion_done) {
            self.append_to_notion(&entry.page_id, &post).await?;
            self.posts
                .mark_notion_posted(&post.key, post.date, self.clock.now())
                .await?;
        }

        if !record.as_ref().is_some_and(DiaryPostRecord::thread_done) {
            let message_id = self.post_to_thread(entry.thread_id, &post).await?;
            self.record_thread_post(&post.key, post.date, message_id)
                .await?;
        }

        Ok(PublishOutcome::Published)
    }

    /// 本文の段落と画像のブロックを Notion ページの末尾へ追加する。
    async fn append_to_notion(&self, page_id: &str, post: &DiaryPost) -> Result<()> {
        let mut children = vec![paragraph_block_json(&post.text)];
        for image in &post.images {
            let upload_id = self
                .notion
                .upload_file(&image.filename, &image.content_type, image.bytes.clone())
                .await
                .context("Failed to upload diary post image to Notion")?;
            children.push(image_block_json(&upload_id));
        }
        self.notion
            .append_blocks(page_id, children)
            .await
            .context("Failed to append diary post to Notion")?;
        Ok(())
    }

    /// スレッドへ投稿し、メッセージ ID を返す。
    ///
    /// クローズ済みなら再開してから投稿し、投稿の成否にかかわらずクローズへ戻す。
    /// 戻せなかった場合は警告に留める。失敗を返すと、次の呼び出しで二重に投稿してしまうため。
    async fn post_to_thread(&self, thread_id: u64, post: &DiaryPost) -> Result<u64> {
        let state = self
            .discord
            .thread_state(thread_id)
            .await?
            .with_context(|| format!("Diary thread {thread_id} is not accessible"))?;

        let reopened = state.is_closed();
        if reopened {
            ensure!(
                self.discord.reopen_thread(thread_id).await?,
                "Failed to reopen diary thread {thread_id}"
            );
        }

        let sent = self
            .discord
            .send_text_with_images(thread_id, &post.text, &post.images)
            .await;

        if reopened && let Err(error) = self.discord.close_thread(thread_id).await {
            warn!(
                ?error,
                thread_id, "Failed to close diary thread again after posting"
            );
        }

        sent
    }

    /// スレッドへの投稿を記録する。
    async fn record_thread_post(&self, key: &str, date: NaiveDate, message_id: u64) -> Result<()> {
        self.posts
            .mark_thread_posted(key, date, message_id, self.clock.now())
            .await
    }
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use mockall::Sequence;

    use kgd_domain::{DiaryPostImage, ThreadState};

    use crate::{
        ports::{MockDiaryPostRepository, MockDiaryRepository, MockDiscordGateway, MockNotionApi},
        test_support::{entry, fixed_clock, utc},
    };

    use super::*;

    const THREAD_ID: u64 = 10;

    fn calendar() -> DiaryCalendar {
        DiaryCalendar::new(chrono_tz::Asia::Tokyo, 8)
    }

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 28).unwrap()
    }

    fn post() -> DiaryPost {
        DiaryPost {
            key: "location-report:2026-09-28".to_string(),
            date: date(),
            text: "位置ログ 2026-09-28".to_string(),
            images: vec![DiaryPostImage {
                filename: "location-2026-09-28.png".to_string(),
                content_type: "image/png".to_string(),
                bytes: vec![1, 2, 3],
            }],
        }
    }

    fn record(notion: bool, thread: bool) -> DiaryPostRecord {
        let at = utc(2026, 9, 28, 23, 0);
        DiaryPostRecord {
            key: post().key,
            diary_date: date(),
            notion_posted_at: notion.then_some(at),
            thread_message_id: thread.then_some(1),
            thread_posted_at: thread.then_some(at),
            skipped_at: None,
        }
    }

    fn thread_state(closed: bool) -> ThreadState {
        ThreadState {
            is_public_thread: true,
            archived: closed,
            locked: closed,
        }
    }

    /// その日の日報エントリを返す DiaryRepository。
    fn diary_with_entry() -> MockDiaryRepository {
        let mut diary = MockDiaryRepository::new();
        let expected = calendar().start_of(date());
        diary
            .expect_get_by_date()
            .withf(move |d| *d == expected)
            .returning(move |d| Ok(Some(entry(THREAD_ID, d))));
        diary
    }

    /// 画像 1 枚のアップロードと追記に成功する NotionApi。
    fn notion_ok() -> MockNotionApi {
        let mut notion = MockNotionApi::new();
        notion
            .expect_upload_file()
            .withf(|filename, content_type, _| {
                filename == "location-2026-09-28.png" && content_type == "image/png"
            })
            .times(1)
            .returning(|_, _, _| Ok("upload-1".to_string()));
        notion
            .expect_append_blocks()
            .withf(|page_id, children| {
                page_id == format!("page-{THREAD_ID}")
                    && children.len() == 2
                    && children[0]["type"] == "paragraph"
                    && children[1]["type"] == "image"
            })
            .times(1)
            .returning(|_, _| Ok(vec!["b1".to_string(), "b2".to_string()]));
        notion
    }

    fn use_case(
        diary: MockDiaryRepository,
        posts: MockDiaryPostRepository,
        notion: MockNotionApi,
        discord: MockDiscordGateway,
    ) -> PublishDiaryPostUseCase {
        PublishDiaryPostUseCase::new(
            Arc::new(diary),
            Arc::new(posts),
            Arc::new(notion),
            Arc::new(discord),
            Arc::new(fixed_clock(utc(2026, 9, 28, 23, 1))),
            calendar(),
        )
    }

    /// 開いているスレッドへは再開もクローズもせずに投稿し、Notion、スレッドの順に記録することを確認する。
    #[tokio::test]
    async fn publish_posts_to_notion_then_thread_and_records_both() {
        let mut seq = Sequence::new();
        let mut posts = MockDiaryPostRepository::new();
        posts.expect_get().returning(|_| Ok(None));
        posts
            .expect_mark_notion_posted()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_, _, _| Ok(()));
        let mut discord = MockDiscordGateway::new();
        discord
            .expect_thread_state()
            .returning(|_| Ok(Some(thread_state(false))));
        discord.expect_reopen_thread().times(0);
        discord.expect_close_thread().times(0);
        discord
            .expect_send_text_with_images()
            .withf(|channel_id, content, images| {
                *channel_id == THREAD_ID && content == "位置ログ 2026-09-28" && images.len() == 1
            })
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_, _, _| Ok(555));
        posts
            .expect_mark_thread_posted()
            .withf(|key, _, message_id, _| {
                key == "location-report:2026-09-28" && *message_id == 555
            })
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_, _, _, _| Ok(()));

        let outcome = use_case(diary_with_entry(), posts, notion_ok(), discord)
            .publish(post())
            .await
            .unwrap();

        assert_eq!(outcome, PublishOutcome::Published);
    }

    /// 完了済みの記録があれば、何も載せずに AlreadyDone を返すことを確認する。
    #[tokio::test]
    async fn publish_returns_already_done_without_side_effects() {
        let mut posts = MockDiaryPostRepository::new();
        posts
            .expect_get()
            .returning(|_| Ok(Some(record(true, true))));
        let mut diary = MockDiaryRepository::new();
        diary.expect_get_by_date().times(0);

        let outcome = use_case(
            diary,
            posts,
            MockNotionApi::new(),
            MockDiscordGateway::new(),
        )
        .publish(post())
        .await
        .unwrap();

        assert_eq!(outcome, PublishOutcome::AlreadyDone);
    }

    /// 日報が無い日はスキップとして記録し、どこにも載せないことを確認する。
    #[tokio::test]
    async fn publish_marks_skipped_when_no_diary() {
        let mut posts = MockDiaryPostRepository::new();
        posts.expect_get().returning(|_| Ok(None));
        posts
            .expect_mark_skipped()
            .withf(|key, date, _| key == "location-report:2026-09-28" && *date == self::date())
            .times(1)
            .returning(|_, _, _| Ok(()));
        let mut diary = MockDiaryRepository::new();
        diary.expect_get_by_date().returning(|_| Ok(None));

        let outcome = use_case(
            diary,
            posts,
            MockNotionApi::new(),
            MockDiscordGateway::new(),
        )
        .publish(post())
        .await
        .unwrap();

        assert_eq!(outcome, PublishOutcome::NoDiary);
    }

    /// Notion だけ済んでいる記録からは、スレッドへの投稿だけをやり直すことを確認する。
    ///
    /// 途中で失敗した後の再試行で Notion に二重に載せないため。
    #[tokio::test]
    async fn publish_resumes_from_thread_when_notion_is_done() {
        let mut posts = MockDiaryPostRepository::new();
        posts
            .expect_get()
            .returning(|_| Ok(Some(record(true, false))));
        posts.expect_mark_notion_posted().times(0);
        posts
            .expect_mark_thread_posted()
            .times(1)
            .returning(|_, _, _, _| Ok(()));
        let mut discord = MockDiscordGateway::new();
        discord
            .expect_thread_state()
            .returning(|_| Ok(Some(thread_state(false))));
        discord
            .expect_send_text_with_images()
            .times(1)
            .returning(|_, _, _| Ok(555));

        let outcome = use_case(diary_with_entry(), posts, MockNotionApi::new(), discord)
            .publish(post())
            .await
            .unwrap();

        assert_eq!(outcome, PublishOutcome::Published);
    }

    /// クローズ済みのスレッドは再開してから投稿し、投稿後にクローズへ戻すことを確認する。
    #[tokio::test]
    async fn publish_reopens_closed_thread_and_closes_it_again() {
        let mut seq = Sequence::new();
        let mut posts = MockDiaryPostRepository::new();
        posts
            .expect_get()
            .returning(|_| Ok(Some(record(true, false))));
        posts
            .expect_mark_thread_posted()
            .returning(|_, _, _, _| Ok(()));
        let mut discord = MockDiscordGateway::new();
        discord
            .expect_thread_state()
            .returning(|_| Ok(Some(thread_state(true))));
        discord
            .expect_reopen_thread()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_| Ok(true));
        discord
            .expect_send_text_with_images()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_, _, _| Ok(555));
        discord
            .expect_close_thread()
            .withf(|thread_id| *thread_id == THREAD_ID)
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_| Ok(()));

        let outcome = use_case(diary_with_entry(), posts, MockNotionApi::new(), discord)
            .publish(post())
            .await
            .unwrap();

        assert_eq!(outcome, PublishOutcome::Published);
    }

    /// 投稿に失敗してもクローズへ戻し、スレッドの完了は記録しないことを確認する。
    ///
    /// 開いたまま残すと、閉じたはずの日報に人が書き込めてしまうため。
    #[tokio::test]
    async fn publish_closes_thread_even_when_send_fails() {
        let mut posts = MockDiaryPostRepository::new();
        posts
            .expect_get()
            .returning(|_| Ok(Some(record(true, false))));
        posts.expect_mark_thread_posted().times(0);
        let mut discord = MockDiscordGateway::new();
        discord
            .expect_thread_state()
            .returning(|_| Ok(Some(thread_state(true))));
        discord.expect_reopen_thread().returning(|_| Ok(true));
        discord
            .expect_send_text_with_images()
            .returning(|_, _, _| Err(anyhow!("discord is down")));
        discord.expect_close_thread().times(1).returning(|_| Ok(()));

        let result = use_case(diary_with_entry(), posts, MockNotionApi::new(), discord)
            .publish(post())
            .await;

        assert!(result.is_err());
    }

    /// 投稿に成功した後でクローズへ戻せなくても、投稿済みとして記録することを確認する。
    ///
    /// ここで失敗を返すと、次の tick で同じ内容をスレッドへもう一度投稿してしまうため。
    #[tokio::test]
    async fn publish_records_thread_post_even_when_reclose_fails() {
        let mut posts = MockDiaryPostRepository::new();
        posts
            .expect_get()
            .returning(|_| Ok(Some(record(true, false))));
        posts
            .expect_mark_thread_posted()
            .times(1)
            .returning(|_, _, _, _| Ok(()));
        let mut discord = MockDiscordGateway::new();
        discord
            .expect_thread_state()
            .returning(|_| Ok(Some(thread_state(true))));
        discord.expect_reopen_thread().returning(|_| Ok(true));
        discord
            .expect_send_text_with_images()
            .returning(|_, _, _| Ok(555));
        discord
            .expect_close_thread()
            .returning(|_| Err(anyhow!("missing permission")));

        let outcome = use_case(diary_with_entry(), posts, MockNotionApi::new(), discord)
            .publish(post())
            .await
            .unwrap();

        assert_eq!(outcome, PublishOutcome::Published);
    }

    /// 再開できなかったときは投稿せずに失敗を返すことを確認する。
    #[tokio::test]
    async fn publish_fails_without_posting_when_reopen_fails() {
        let mut posts = MockDiaryPostRepository::new();
        posts
            .expect_get()
            .returning(|_| Ok(Some(record(true, false))));
        let mut discord = MockDiscordGateway::new();
        discord
            .expect_thread_state()
            .returning(|_| Ok(Some(thread_state(true))));
        discord.expect_reopen_thread().returning(|_| Ok(false));
        discord.expect_send_text_with_images().times(0);
        discord.expect_close_thread().times(0);

        let result = use_case(diary_with_entry(), posts, MockNotionApi::new(), discord)
            .publish(post())
            .await;

        assert!(result.is_err());
    }

    /// is_done が記録の完了状態をそのまま返すことを確認する。
    #[tokio::test]
    async fn is_done_reflects_the_record() {
        let mut posts = MockDiaryPostRepository::new();
        posts
            .expect_get()
            .withf(|key| key == "done")
            .returning(|_| Ok(Some(record(true, true))));
        posts
            .expect_get()
            .withf(|key| key == "partial")
            .returning(|_| Ok(Some(record(true, false))));
        posts
            .expect_get()
            .withf(|key| key == "missing")
            .returning(|_| Ok(None));
        let use_case = use_case(
            MockDiaryRepository::new(),
            posts,
            MockNotionApi::new(),
            MockDiscordGateway::new(),
        );

        assert!(use_case.is_done("done").await.unwrap());
        assert!(!use_case.is_done("partial").await.unwrap());
        assert!(!use_case.is_done("missing").await.unwrap());
    }
}
