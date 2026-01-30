//! Database schema definitions
//!
//! This schema matches the production database (glance_mind_rust).
//! Tables are defined to match the actual PostgreSQL schema.

use diesel::table;

// ============================================================
// Platforms table
// ============================================================
table! {
    gm_platforms (id) {
        id -> Int4,
        name -> Varchar,
        display_name -> Varchar,
        is_active -> Bool,
        created_at -> Timestamptz,
        updated_at -> Nullable<Timestamptz>,
        base_url -> Varchar,
        page_size -> Int4,
        content_table_name -> Nullable<Varchar>,
        comment_table_name -> Nullable<Varchar>,
    }
}

// ============================================================
// Campaigns table
// ============================================================
table! {
    gm_campaigns (id) {
        id -> Int4,
        user_id -> Int4,
        name -> Varchar,
        status -> Varchar,
        platform_id -> Int4,
        region_id -> Int4,
        ai_model_id -> Int4,
        target_audience -> Nullable<Text>,
        product_prompt -> Text,
        schedule_config -> Nullable<Jsonb>,
        enable_ai_refactor -> Nullable<Bool>,
        persona_id -> Nullable<Int4>,
        max_scan_count -> Nullable<Int4>,
        budget_cap -> Nullable<Numeric>,
        end_date -> Nullable<Timestamptz>,
        schedule_type -> Varchar,
        created_at -> Timestamptz,
        updated_at -> Nullable<Timestamptz>,
        keyword -> Nullable<Text>,
        social_group_id -> Nullable<Int4>,
        call_to_action -> Nullable<Text>,
        tone_of_voice -> Nullable<Text>,
        additional_info -> Nullable<Text>,
        total_scanned -> Int4,
        auto_like -> Bool,
        auto_follow -> Bool,
        auto_dm -> Bool,
        pending_consumption -> Numeric,
        actual_consumption -> Numeric,
        is_frozen -> Bool,
        search_options -> Nullable<Jsonb>,
        auto_reply_comments -> Bool,
        auto_reply_post -> Bool,
        completed_reason -> Nullable<Text>,
    }
}

// ============================================================
// Campaign Templates table
// ============================================================
table! {
    gm_campaign_templates (id) {
        id -> Int4,
        campaign_id -> Int4,
        weight -> Int4,
        reply_prompt -> Nullable<Text>,
        created_at -> Timestamptz,
        updated_at -> Nullable<Timestamptz>,
        dm_prompt -> Nullable<Text>,
        reply_post_prompt -> Nullable<Text>,
        name -> Nullable<Varchar>,
    }
}

// ============================================================
// Crawler Tasks table
// ============================================================
table! {
    gm_crawler_tasks (id) {
        id -> Int4,
        campaign_id -> Int4,
        keywords -> Nullable<Array<Text>>,
        max_count -> Int4,
        process_count -> Int4,
        status -> Varchar,
        created_at -> Timestamptz,
        updated_at -> Nullable<Timestamptz>,
        search_offset -> Int4,
        search_limit -> Int4,
        reserved_amount -> Nullable<Numeric>,
        actual_consumption -> Nullable<Numeric>,
        settled_at -> Nullable<Timestamptz>,
    }
}

// ============================================================
// Agent Videos table (TikTok)
// ============================================================
table! {
    gm_agent_videos (id) {
        id -> Int4,
        video_id -> Nullable<Varchar>,
        author -> Nullable<Varchar>,
        description -> Nullable<Text>,
        created_at -> Timestamptz,
        task_id -> Int4,
        campaign_id -> Nullable<Int4>,
        like_count -> Nullable<Int4>,
        comment_count -> Nullable<Int4>,
        share_count -> Nullable<Int4>,
        play_count -> Nullable<Int4>,
        publish_time -> Nullable<Int8>,
        author_unique_id -> Nullable<Varchar>,
        url -> Nullable<Text>,
        updated_at -> Nullable<Timestamptz>,
    }
}

// ============================================================
// Agent Comments table (TikTok)
// ============================================================
table! {
    gm_agent_comments (id) {
        id -> Int4,
        video_db_id -> Int4,
        comment_id -> Varchar,
        user_nickname -> Nullable<Varchar>,
        user_unique_id -> Nullable<Varchar>,
        content -> Nullable<Text>,
        reason -> Nullable<Text>,
        suggested_reply -> Nullable<Text>,
        create_time -> Nullable<Timestamp>,
        created_at -> Timestamptz,
        campaign_id -> Nullable<Int4>,
        status -> Int2,
        suggested_dm -> Nullable<Text>,
        suggested_reply_post -> Nullable<Text>,
        updated_at -> Nullable<Timestamptz>,
    }
}

// ============================================================
// Define relationships
// ============================================================
diesel::joinable!(gm_campaigns -> gm_platforms (platform_id));
diesel::joinable!(gm_campaign_templates -> gm_campaigns (campaign_id));
diesel::joinable!(gm_crawler_tasks -> gm_campaigns (campaign_id));
diesel::joinable!(gm_agent_videos -> gm_crawler_tasks (task_id));
diesel::joinable!(gm_agent_videos -> gm_campaigns (campaign_id));
diesel::joinable!(gm_agent_comments -> gm_agent_videos (video_db_id));
diesel::joinable!(gm_agent_comments -> gm_campaigns (campaign_id));

diesel::allow_tables_to_appear_in_same_query!(
    gm_platforms,
    gm_campaigns,
    gm_campaign_templates,
    gm_crawler_tasks,
    gm_agent_videos,
    gm_agent_comments,
);
