//! Content Gateway Port - Interface for fetching content from platforms

use async_trait::async_trait;

use crate::domain::errors::{GatewayError, GatewayResult};
use crate::domain::{Content, KeywordType, SearchOptions};

/// 取数欠交付原因(per R-007;cursor 不过 trait,A005 drop)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchShortfall {
    /// 上游枯竭:has_more=false / cursor 缺失 / 空页上限 / 重复 cursor(F-003/F-004/F-005)
    Exhausted,
    /// 翻页中途失败但已有部分进展(F-002/F-006);message 仅用于 terminal_reason,入库前必经脱敏
    PartialFailure { message: String },
}

/// fetch 路径返回载体
#[derive(Debug, Clone)]
pub struct FetchOutcome {
    pub contents: Vec<Content>,
    /// None = 足量交付 或 适配器未提供欠交付信息(未迁移平台的滚动兼容语义)
    pub shortfall: Option<FetchShortfall>,
}

impl FetchOutcome {
    /// 构造:足量交付(`shortfall = None`)。
    pub fn complete(contents: Vec<Content>) -> Self {
        Self {
            contents,
            shortfall: None,
        }
    }

    /// 构造:上游枯竭欠交付(`shortfall = Some(FetchShortfall::Exhausted)`)。
    pub fn exhausted(contents: Vec<Content>) -> Self {
        Self {
            contents,
            shortfall: Some(FetchShortfall::Exhausted),
        }
    }

    /// 构造:部分失败(`shortfall = Some(FetchShortfall::PartialFailure { message })`)。
    ///
    /// **构造不变量(DR-01,冻结)**:`PartialFailure` ⇒ `contents` 非空;
    /// 零可交付进展(过滤后为空)的失败一律走 `Err`(F-001 语义)。
    /// 本构造器对空 `contents` 必须返回 `Err`(归一化为错误),
    /// 不得产出「空 contents + Some(PartialFailure)」的合法 `FetchOutcome`。
    pub fn partial(contents: Vec<Content>, message: impl Into<String>) -> GatewayResult<Self> {
        if contents.is_empty() {
            // DR-01:零可交付进展的失败归一化为 Err(F-001 语义)
            return Err(GatewayError::InvalidParams(
                "PartialFailure requires non-empty contents (DR-01): zero-progress failures must be Err".into(),
            ));
        }
        Ok(Self {
            contents,
            shortfall: Some(FetchShortfall::PartialFailure {
                message: message.into(),
            }),
        })
    }
}

/// Port for fetching content (videos, posts) from external platforms
#[async_trait]
pub trait ContentGateway: Send + Sync {
    /// Search for content based on options
    ///
    /// # Arguments
    /// * `options` - Search parameters (query, region, count, etc.)
    ///
    /// # Returns
    /// * `GatewayResult<Vec<Content>>` - List of matching content
    async fn search(&self, options: &SearchOptions) -> GatewayResult<Vec<Content>>;

    /// Fetch content by a specific keyword type
    ///
    /// This handles platform-specific keyword parsing:
    /// - `KeywordType::Search` -> regular search
    /// - `KeywordType::UserId` -> fetch user's content
    /// - `KeywordType::ContentId` -> fetch specific content
    ///
    /// # Arguments
    /// * `keyword` - Parsed keyword type
    /// * `options` - Additional search options
    ///
    /// # Returns
    /// * `GatewayResult<Vec<Content>>` - List of matching content
    async fn fetch_by_keyword(
        &self,
        keyword: &KeywordType,
        options: &SearchOptions,
    ) -> GatewayResult<Vec<Content>>;

    /// 取数并暴露欠交付原因(D1 契约,m1-pagination-core.md §2;R-007,A005:不传 cursor)。
    ///
    /// 默认方法 = 契约原文:包装既有 `fetch_by_keyword`,`shortfall = None`
    /// (未迁移平台的滚动兼容语义;`shortfall=None` 时 orchestrator 行为与现状完全一致)。
    /// M2~M5 各平台适配器 override 此方法。
    async fn fetch_by_keyword_with_outcome(
        &self,
        keyword: &KeywordType,
        options: &SearchOptions,
    ) -> GatewayResult<FetchOutcome> {
        Ok(FetchOutcome {
            contents: self.fetch_by_keyword(keyword, options).await?,
            shortfall: None,
        })
    }

    /// Fetch content from a specific user
    ///
    /// # Arguments
    /// * `user_id` - User identifier (username or platform-specific ID)
    /// * `count` - Maximum number of items to fetch
    ///
    /// # Returns
    /// * `GatewayResult<Vec<Content>>` - List of user's content
    async fn fetch_user_content(&self, user_id: &str, count: u32) -> GatewayResult<Vec<Content>>;

    /// Fetch a specific content item by ID
    ///
    /// # Arguments
    /// * `content_id` - Platform-specific content ID
    ///
    /// # Returns
    /// * `GatewayResult<Option<Content>>` - The content if found
    async fn fetch_by_id(&self, content_id: &str) -> GatewayResult<Option<Content>>;

    /// Get the platform name this gateway handles
    fn platform(&self) -> &str;
}

/// Options for fetching user content
#[derive(Debug, Clone, Default)]
pub struct UserContentOptions {
    /// User's unique ID (e.g., @username)
    pub user_id: Option<String>,

    /// User's secure/internal ID (platform-specific)
    pub sec_user_id: Option<String>,

    /// Maximum number of items to fetch
    pub count: u32,

    /// Pagination cursor
    pub cursor: Option<i64>,

    /// Sort type (platform-specific)
    pub sort_type: Option<u8>,
}

impl UserContentOptions {
    /// Create options for fetching by username
    pub fn by_username(username: impl Into<String>) -> Self {
        Self {
            user_id: Some(username.into()),
            count: 20,
            ..Default::default()
        }
    }

    /// Create options for fetching by secure user ID
    pub fn by_sec_user_id(sec_uid: impl Into<String>) -> Self {
        Self {
            sec_user_id: Some(sec_uid.into()),
            count: 20,
            ..Default::default()
        }
    }

    /// Set the count
    pub fn with_count(mut self, count: u32) -> Self {
        self.count = count;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_content_options() {
        let opts = UserContentOptions::by_username("testuser").with_count(10);
        assert_eq!(opts.user_id, Some("testuser".to_string()));
        assert_eq!(opts.count, 10);
    }

    // ===== M1-T3 测试载荷(m1-pagination-core.md §3 M1-T3;断言 = 计划原文契约) =====

    /// 仅实现既有 4 方法的最小测试 gateway(不 override `fetch_by_keyword_with_outcome`)。
    struct LegacyOnlyGateway;

    #[async_trait]
    impl ContentGateway for LegacyOnlyGateway {
        async fn search(&self, _options: &SearchOptions) -> GatewayResult<Vec<Content>> {
            Ok(vec![Content::new("legacy", "s1")])
        }

        async fn fetch_by_keyword(
            &self,
            _keyword: &KeywordType,
            _options: &SearchOptions,
        ) -> GatewayResult<Vec<Content>> {
            Ok(vec![
                Content::new("legacy", "k1"),
                Content::new("legacy", "k2"),
            ])
        }

        async fn fetch_user_content(
            &self,
            _user_id: &str,
            _count: u32,
        ) -> GatewayResult<Vec<Content>> {
            Ok(vec![])
        }

        async fn fetch_by_id(&self, _content_id: &str) -> GatewayResult<Option<Content>> {
            Ok(None)
        }

        fn platform(&self) -> &str {
            "legacy"
        }
    }

    /// M1-T3 测试 1(允许先绿,契约文档型;AG-006 有效性由 AG-012 变异预检兜底:
    /// 默认方法被变异时本测试须变红)。
    #[tokio::test]
    async fn default_method_preserves_legacy_semantics() {
        let gateway = LegacyOnlyGateway;
        let keyword = KeywordType::Search("kw".to_string());
        let options = SearchOptions::new("kw");

        let legacy = gateway.fetch_by_keyword(&keyword, &options).await.unwrap();
        let outcome = gateway
            .fetch_by_keyword_with_outcome(&keyword, &options)
            .await
            .unwrap();

        assert!(outcome.shortfall.is_none());
        let legacy_ids: Vec<&str> = legacy.iter().map(|c| c.content_id.as_str()).collect();
        let outcome_ids: Vec<&str> = outcome
            .contents
            .iter()
            .map(|c| c.content_id.as_str())
            .collect();
        assert_eq!(outcome_ids, legacy_ids);
        assert_eq!(outcome_ids, vec!["k1", "k2"]);
    }

    /// M1-T3 测试 5:三构造器字段断言(partial 携带 message)。
    #[test]
    fn fetch_outcome_constructors() {
        let complete = FetchOutcome::complete(vec![
            Content::new("mock", "a"),
            Content::new("mock", "b"),
        ]);
        assert_eq!(complete.contents.len(), 2);
        assert!(complete.shortfall.is_none());

        let exhausted = FetchOutcome::exhausted(vec![Content::new("mock", "a")]);
        assert_eq!(exhausted.contents.len(), 1);
        assert_eq!(exhausted.shortfall, Some(FetchShortfall::Exhausted));

        let partial = FetchOutcome::partial(vec![Content::new("mock", "a")], "rate limited upstream")
            .expect("partial with non-empty contents must construct");
        assert_eq!(partial.contents.len(), 1);
        match partial.shortfall {
            Some(FetchShortfall::PartialFailure { ref message }) => {
                assert_eq!(message, "rate limited upstream");
            }
            ref other => panic!("expected PartialFailure shortfall, got {other:?}"),
        }
    }

    /// M1-T3 测试 7(DR-01 构造不变量):空 contents + partial → 构造器拒绝(Err)。
    /// 违例形状「空 contents + Some(PartialFailure)」不得作为合法 `FetchOutcome` 产出。
    #[test]
    fn partial_constructor_rejects_empty_contents() {
        let result = FetchOutcome::partial(vec![], "rate limited upstream");
        assert!(
            result.is_err(),
            "constructor rejects/normalizes empty contents; got {result:?}"
        );
    }
}
