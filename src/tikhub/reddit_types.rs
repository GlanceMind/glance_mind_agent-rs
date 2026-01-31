//! Reddit TikHub API Response Types
//!
//! These types are derived from TikHub Reddit API responses.
//! Based on endpoints:
//! - `/api/v1/reddit/app/fetch_dynamic_search` - Search posts dynamically
//! - `/api/v1/reddit/app/fetch_post_comments` - Fetch post comments
//! - `/api/v1/reddit/app/fetch_user_posts` - Fetch user's posts

use serde::{Deserialize, Serialize};

// ============================================================
// Common Response Types
// ============================================================

/// Reddit API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditResponse<T> {
    pub code: i32,
    #[serde(default)]
    pub message: String,
    pub data: Option<T>,
}

// ============================================================
// Dynamic Search API Types
// Endpoint: /api/v1/reddit/app/fetch_dynamic_search
// ============================================================

/// Dynamic search response
pub type RedditSearchResponse = RedditResponse<RedditSearchData>;

/// Search data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditSearchData {
    pub search: Option<RedditSearchWrapper>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditSearchWrapper {
    pub dynamic: Option<RedditDynamicData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditDynamicData {
    pub components: Option<RedditComponents>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditComponents {
    #[serde(rename = "__typename")]
    pub typename: Option<String>,
    pub main: Option<RedditMainComponent>,
    // Allow unknown fields
    #[serde(flatten)]
    pub extra: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditMainComponent {
    pub edges: Option<Vec<RedditEdge>>,
    #[serde(rename = "pageInfo")]
    pub page_info: Option<RedditPageInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditEdge {
    pub node: Option<RedditNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditNode {
    #[serde(rename = "__typename")]
    pub typename: Option<String>,
    pub children: Option<Vec<RedditSearchChild>>,
    pub edges: Option<Vec<RedditPostEdge>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditSearchChild {
    #[serde(rename = "__typename")]
    pub typename: Option<String>,
    pub post: Option<RedditPost>,
    // Direct post fields for SubredditPost typename
    pub id: Option<String>,
    #[serde(rename = "postTitle")]
    pub post_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditPostEdge {
    pub node: Option<RedditPost>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditPageInfo {
    #[serde(rename = "hasNextPage")]
    pub has_next_page: Option<bool>,
    #[serde(rename = "endCursor")]
    pub end_cursor: Option<String>,
}

// ============================================================
// Post Comments API Types
// Endpoint: /api/v1/reddit/app/fetch_post_comments
// ============================================================

/// Post comments response
pub type RedditCommentsResponse = RedditResponse<RedditCommentsData>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditCommentsData {
    #[serde(rename = "postInfoById")]
    pub post_info_by_id: Option<RedditPostInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditPostInfo {
    #[serde(rename = "commentForest")]
    pub comment_forest: Option<RedditCommentForest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditCommentForest {
    pub trees: Option<Vec<RedditCommentTree>>,
    #[serde(rename = "pageInfo")]
    pub page_info: Option<RedditPageInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditCommentTree {
    pub node: Option<RedditCommentNode>,
    pub children: Option<Vec<RedditCommentTree>>,
    #[serde(rename = "parentId")]
    pub parent_id: Option<String>,
    pub depth: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditCommentNode {
    #[serde(rename = "__typename")]
    pub typename: Option<String>,
    pub id: Option<String>,
    pub content: Option<RedditContent>,
    pub author: Option<RedditAuthor>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
    #[serde(rename = "voteCount")]
    pub vote_count: Option<i64>,
    pub permalink: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditContent {
    pub markdown: Option<String>,
    pub richtext: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RedditAuthor {
    Object { name: Option<String> },
    String(String),
}

impl RedditAuthor {
    pub fn name(&self) -> Option<&str> {
        match self {
            RedditAuthor::Object { name } => name.as_deref(),
            RedditAuthor::String(s) => Some(s.as_str()),
        }
    }
}

// ============================================================
// User Posts API Types
// Endpoint: /api/v1/reddit/app/fetch_user_posts
// ============================================================

/// User posts response
pub type RedditUserPostsResponse = RedditResponse<RedditUserPostsData>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditUserPostsData {
    #[serde(rename = "postFeed")]
    pub post_feed: Option<RedditPostFeed>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditPostFeed {
    pub elements: Option<RedditPostElements>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditPostElements {
    pub edges: Option<Vec<RedditPostEdge>>,
    #[serde(rename = "pageInfo")]
    pub page_info: Option<RedditPageInfo>,
}

// ============================================================
// Batch Post Details API Types
// Endpoint: /api/v1/reddit/app/fetch_post_details_batch_large
// ============================================================

/// Batch posts response
pub type RedditBatchPostsResponse = RedditResponse<RedditBatchPostsData>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditBatchPostsData {
    /// List of post details
    pub posts: Option<Vec<RedditBatchPostDetail>>,
    /// Allow unknown fields
    #[serde(flatten)]
    pub extra: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditBatchPostDetail {
    /// Post ID
    pub id: Option<String>,
    /// Post title
    pub title: Option<String>,
    /// Post content/selftext
    pub selftext: Option<String>,
    /// Author
    pub author: Option<String>,
    /// Subreddit name
    pub subreddit: Option<String>,
    /// Score
    pub score: Option<i64>,
    /// Number of comments
    pub num_comments: Option<i64>,
    /// Created timestamp
    pub created_utc: Option<f64>,
    /// Permalink
    pub permalink: Option<String>,
    /// URL
    pub url: Option<String>,
    /// Is video
    pub is_video: Option<bool>,
    /// Allow unknown fields
    #[serde(flatten)]
    pub extra: Option<serde_json::Value>,
}

// ============================================================
// Reddit Post Data Structure
// ============================================================

/// Reddit post information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditPost {
    /// Post ID (may include "t3_" prefix)
    pub id: Option<String>,

    /// Post title
    #[serde(alias = "postTitle")]
    pub title: Option<String>,

    /// Post content
    pub content: Option<RedditContent>,

    /// Self text (legacy format)
    pub selftext: Option<String>,

    /// Post author
    pub author: Option<RedditAuthor>,

    /// Subreddit
    pub subreddit: Option<RedditSubreddit>,

    /// Created at timestamp (ISO or Unix)
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,

    /// Vote count / score
    #[serde(alias = "voteCount")]
    pub score: Option<i64>,

    /// Upvote ratio
    #[serde(rename = "upvoteRatio")]
    pub upvote_ratio: Option<f64>,

    /// Comment count
    #[serde(alias = "commentCount", alias = "numComments")]
    pub num_comments: Option<i64>,

    /// Post URL
    pub url: Option<String>,

    /// Permalink
    pub permalink: Option<String>,

    /// Is video
    #[serde(rename = "isVideo")]
    pub is_video: Option<bool>,

    /// Domain
    pub domain: Option<String>,

    /// Thumbnail (can be string URL or complex object)
    #[serde(default)]
    pub thumbnail: Option<serde_json::Value>,

    /// Typename (for GraphQL responses)
    #[serde(rename = "__typename")]
    pub typename: Option<String>,

    /// Media object (complex structure)
    #[serde(default)]
    pub media: Option<serde_json::Value>,

    /// Allow unknown fields
    #[serde(flatten)]
    pub extra: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RedditSubreddit {
    Object { name: Option<String> },
    String(String),
}

impl RedditSubreddit {
    pub fn name(&self) -> Option<&str> {
        match self {
            RedditSubreddit::Object { name } => name.as_deref(),
            RedditSubreddit::String(s) => Some(s.as_str()),
        }
    }
}

/// Reddit comment (flattened from tree structure)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditComment {
    pub id: String,
    pub name: Option<String>, // Full ID like "t1_xxxxx"
    pub author: String,
    pub body: String,
    pub body_html: Option<String>,
    pub created_utc: i64,
    pub score: i64,
    pub parent_id: Option<String>,
    pub is_reply: bool,
    pub depth: i32,
    pub subreddit: Option<String>,
    pub permalink: Option<String>,
}

// ============================================================
// Request Parameters
// ============================================================

/// Dynamic search parameters
#[derive(Debug, Clone, Default)]
pub struct RedditSearchParams {
    pub query: String,
    pub safe_search: String, // "unset" or "strict"
    pub allow_nsfw: String,  // "0" or "1"
    pub after: Option<String>,
}

impl RedditSearchParams {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            safe_search: "unset".to_string(),
            allow_nsfw: "0".to_string(),
            after: None,
        }
    }

    pub fn with_safe_search(mut self, safe_search: impl Into<String>) -> Self {
        self.safe_search = safe_search.into();
        self
    }

    pub fn with_allow_nsfw(mut self, allow: bool) -> Self {
        self.allow_nsfw = if allow {
            "1".to_string()
        } else {
            "0".to_string()
        };
        self
    }

    pub fn with_after(mut self, after: impl Into<String>) -> Self {
        self.after = Some(after.into());
        self
    }
}

/// Comment fetch parameters
#[derive(Debug, Clone, Default)]
pub struct RedditCommentParams {
    pub post_id: String,
    pub sort: String, // "best", "top", "new", "controversial", "old", "qa"
    pub limit: u32,
    pub after: Option<String>,
}

impl RedditCommentParams {
    pub fn new(post_id: impl Into<String>) -> Self {
        Self {
            post_id: post_id.into(),
            sort: "best".to_string(),
            limit: 20,
            after: None,
        }
    }

    pub fn with_sort(mut self, sort: impl Into<String>) -> Self {
        self.sort = sort.into();
        self
    }

    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }

    pub fn with_after(mut self, after: impl Into<String>) -> Self {
        self.after = Some(after.into());
        self
    }
}

/// User posts fetch parameters
#[derive(Debug, Clone, Default)]
pub struct RedditUserPostsParams {
    pub username: String,
    pub sort: String, // "NEW", "TOP", "HOT", "CONTROVERSIAL"
    pub after: Option<String>,
}

impl RedditUserPostsParams {
    pub fn new(username: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            sort: "NEW".to_string(),
            after: None,
        }
    }

    pub fn with_sort(mut self, sort: impl Into<String>) -> Self {
        self.sort = sort.into();
        self
    }

    pub fn with_after(mut self, after: impl Into<String>) -> Self {
        self.after = Some(after.into());
        self
    }
}

// ============================================================
// Conversion Utilities
// ============================================================

impl RedditPost {
    /// Get the post ID without prefix
    pub fn post_id(&self) -> Option<String> {
        self.id.as_ref().map(|id| {
            if let Some(stripped) = id.strip_prefix("t3_") {
                stripped.to_string()
            } else {
                id.clone()
            }
        })
    }

    /// Get the full post ID with prefix
    pub fn full_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// Get post title
    pub fn title_str(&self) -> &str {
        self.title.as_deref().unwrap_or("")
    }

    /// Get post content/selftext
    pub fn content_str(&self) -> &str {
        if let Some(ref content) = self.content {
            if let Some(ref md) = content.markdown {
                if !md.is_empty() {
                    return md;
                }
            }
        }
        self.selftext.as_deref().unwrap_or("")
    }

    /// Get author name
    pub fn author_name(&self) -> Option<&str> {
        self.author.as_ref()?.name()
    }

    /// Get subreddit name
    pub fn subreddit_name(&self) -> Option<&str> {
        self.subreddit.as_ref()?.name()
    }

    /// Get score/vote count
    pub fn votes(&self) -> i64 {
        self.score.unwrap_or(0)
    }

    /// Get comment count
    pub fn comments(&self) -> i64 {
        self.num_comments.unwrap_or(0)
    }

    /// Get created timestamp
    pub fn created_at_timestamp(&self) -> Option<i64> {
        let created_at = self.created_at.as_ref()?;

        // Try to parse ISO format
        if created_at.contains('+') || created_at.contains('Z') {
            if let Ok(dt) =
                chrono::DateTime::parse_from_rfc3339(&created_at.replace("+0000", "+00:00"))
            {
                return Some(dt.timestamp());
            }
        }

        // Try to parse as Unix timestamp
        created_at.parse().ok()
    }
}

impl RedditComment {
    /// Get comment ID without prefix
    pub fn comment_id(&self) -> &str {
        if self.id.starts_with("t1_") {
            &self.id[3..]
        } else {
            &self.id
        }
    }
}

/// Extract posts from search response
pub fn extract_posts_from_search(data: &RedditSearchData) -> Vec<&RedditPost> {
    let mut posts = Vec::new();

    if let Some(ref search) = data.search {
        if let Some(ref dynamic) = search.dynamic {
            if let Some(ref components) = dynamic.components {
                if let Some(ref main) = components.main {
                    if let Some(ref edges) = main.edges {
                        for edge in edges {
                            if let Some(ref node) = edge.node {
                                // Extract from children
                                if let Some(ref children) = node.children {
                                    for child in children {
                                        if let Some(ref post) = child.post {
                                            posts.push(post);
                                        }
                                    }
                                }
                                // Extract from edges
                                if let Some(ref post_edges) = node.edges {
                                    for post_edge in post_edges {
                                        if let Some(ref post) = post_edge.node {
                                            posts.push(post);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    posts
}

/// Extract comments from tree structure
pub fn extract_comments_from_trees(
    trees: &[RedditCommentTree],
    parent_id: Option<&str>,
) -> Vec<RedditComment> {
    let mut comments = Vec::new();

    for tree in trees {
        if let Some(ref node) = tree.node {
            if node.typename.as_deref() == Some("Comment") {
                let author = node
                    .author
                    .as_ref()
                    .map(|a| a.name().unwrap_or("").to_string())
                    .unwrap_or_default();

                let body = node
                    .content
                    .as_ref()
                    .and_then(|c| c.markdown.as_deref())
                    .unwrap_or("")
                    .to_string();

                let created_utc = parse_reddit_timestamp(node.created_at.as_deref().unwrap_or(""));

                let comment = RedditComment {
                    id: node.id.clone().unwrap_or_default(),
                    name: node.id.clone(),
                    author,
                    body,
                    body_html: node.content.as_ref().and_then(|c| c.richtext.clone()),
                    created_utc,
                    score: node.vote_count.unwrap_or(0),
                    parent_id: tree
                        .parent_id
                        .clone()
                        .or_else(|| parent_id.map(String::from)),
                    is_reply: tree.parent_id.is_some() || parent_id.is_some(),
                    depth: tree.depth.unwrap_or(0),
                    subreddit: None,
                    permalink: node.permalink.clone(),
                };
                comments.push(comment);
            }
        }

        // Process children recursively
        if let Some(ref children) = tree.children {
            let child_comments = extract_comments_from_trees(
                children,
                tree.node.as_ref().and_then(|n| n.id.as_deref()),
            );
            comments.extend(child_comments);
        }
    }

    comments
}

/// Parse Reddit timestamp string to Unix timestamp
fn parse_reddit_timestamp(timestamp_str: &str) -> i64 {
    if timestamp_str.is_empty() {
        return 0;
    }

    // Try to parse ISO format
    if timestamp_str.contains('+') || timestamp_str.contains('Z') {
        if let Ok(dt) =
            chrono::DateTime::parse_from_rfc3339(&timestamp_str.replace("+0000", "+00:00"))
        {
            return dt.timestamp();
        }
    }

    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reddit_search_params() {
        let params = RedditSearchParams::new("rust programming")
            .with_safe_search("strict")
            .with_allow_nsfw(false);

        assert_eq!(params.query, "rust programming");
        assert_eq!(params.safe_search, "strict");
        assert_eq!(params.allow_nsfw, "0");
    }

    #[test]
    fn test_reddit_post_helpers() {
        let post = RedditPost {
            id: Some("t3_abc123".to_string()),
            title: Some("Test Post".to_string()),
            content: Some(RedditContent {
                markdown: Some("Post content".to_string()),
                richtext: None,
            }),
            selftext: None,
            author: Some(RedditAuthor::Object {
                name: Some("testuser".to_string()),
            }),
            subreddit: Some(RedditSubreddit::Object {
                name: Some("rust".to_string()),
            }),
            created_at: None,
            score: Some(100),
            upvote_ratio: Some(0.95),
            num_comments: Some(25),
            url: None,
            permalink: None,
            is_video: Some(false),
            domain: None,
            thumbnail: None,
            typename: None,
            media: None,
            extra: None,
        };

        assert_eq!(post.post_id(), Some("abc123".to_string()));
        assert_eq!(post.title_str(), "Test Post");
        assert_eq!(post.content_str(), "Post content");
        assert_eq!(post.author_name(), Some("testuser"));
        assert_eq!(post.subreddit_name(), Some("rust"));
        assert_eq!(post.votes(), 100);
        assert_eq!(post.comments(), 25);
    }

    #[test]
    fn test_reddit_comment_helpers() {
        let comment = RedditComment {
            id: "t1_xyz789".to_string(),
            name: Some("t1_xyz789".to_string()),
            author: "commenter".to_string(),
            body: "Great post!".to_string(),
            body_html: None,
            created_utc: 1234567890,
            score: 50,
            parent_id: None,
            is_reply: false,
            depth: 0,
            subreddit: Some("rust".to_string()),
            permalink: None,
        };

        assert_eq!(comment.comment_id(), "xyz789");
    }
}
