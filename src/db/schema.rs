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
// Facebook Posts table
// ============================================================
table! {
    gm_agent_facebook_posts (id) {
        id -> Int4,
        task_id -> Int4,
        campaign_id -> Nullable<Int4>,
        facebook_post_id -> Varchar,
        post_type -> Nullable<Varchar>,
        url -> Nullable<Text>,
        message -> Nullable<Text>,
        message_rich -> Nullable<Text>,
        timestamp -> Nullable<Int8>,
        posted_at -> Nullable<Timestamptz>,
        reactions_count -> Nullable<Int4>,
        comments_count -> Nullable<Int4>,
        reshare_count -> Nullable<Int4>,
        reactions_like -> Nullable<Int4>,
        reactions_love -> Nullable<Int4>,
        reactions_haha -> Nullable<Int4>,
        reactions_wow -> Nullable<Int4>,
        reactions_sad -> Nullable<Int4>,
        reactions_angry -> Nullable<Int4>,
        reactions_care -> Nullable<Int4>,
        author_id -> Nullable<Varchar>,
        author_name -> Nullable<Varchar>,
        author_url -> Nullable<Text>,
        author_profile_picture_url -> Nullable<Text>,
        author_title -> Nullable<Varchar>,
        has_image -> Nullable<Bool>,
        image_url -> Nullable<Text>,
        image_width -> Nullable<Int4>,
        image_height -> Nullable<Int4>,
        image_id -> Nullable<Varchar>,
        has_video -> Nullable<Bool>,
        video_thumbnail -> Nullable<Text>,
        external_url -> Nullable<Text>,
        attached_post_url -> Nullable<Text>,
        comments_id -> Nullable<Varchar>,
        shares_id -> Nullable<Varchar>,
        created_at -> Timestamptz,
        updated_at -> Nullable<Timestamptz>,
    }
}

// ============================================================
// Facebook Comments table
// ============================================================
table! {
    gm_agent_facebook_comments (id) {
        id -> Int4,
        post_db_id -> Int4,
        campaign_id -> Nullable<Int4>,
        facebook_comment_id -> Varchar,
        parent_comment_id -> Nullable<Varchar>,
        comment_url -> Nullable<Text>,
        comment_text -> Text,
        reason -> Nullable<Text>,
        suggested_reply -> Nullable<Text>,
        suggested_dm -> Nullable<Text>,
        suggested_reply_post -> Nullable<Text>,
        comment_user_id -> Nullable<Varchar>,
        comment_username -> Nullable<Varchar>,
        comment_user_url -> Nullable<Text>,
        comment_user_profile_picture -> Nullable<Text>,
        like_count -> Nullable<Int4>,
        reply_count -> Nullable<Int4>,
        threading_depth -> Nullable<Int4>,
        created_at_ts -> Nullable<Int8>,
        comment_created_at -> Nullable<Timestamptz>,
        facebook_post_id -> Nullable<Varchar>,
        post_url -> Nullable<Text>,
        created_at -> Timestamptz,
        updated_at -> Nullable<Timestamptz>,
        status -> Nullable<Int2>,
    }
}

// ============================================================
// Twitter Tweets table
// ============================================================
table! {
    gm_agent_twitter_tweets (id) {
        id -> Int4,
        task_id -> Int4,
        campaign_id -> Nullable<Int4>,
        twitter_tweet_id -> Varchar,
        conversation_id -> Nullable<Varchar>,
        full_text -> Text,
        lang -> Nullable<Varchar>,
        screen_name -> Nullable<Varchar>,
        user_name -> Nullable<Varchar>,
        user_id -> Nullable<Varchar>,
        user_description -> Nullable<Text>,
        user_followers_count -> Nullable<Int4>,
        user_avatar -> Nullable<Text>,
        user_verified -> Nullable<Bool>,
        media_urls -> Nullable<Array<Nullable<Text>>>,
        has_media -> Nullable<Bool>,
        favorite_count -> Nullable<Int4>,
        retweet_count -> Nullable<Int4>,
        reply_count -> Nullable<Int4>,
        quote_count -> Nullable<Int4>,
        bookmark_count -> Nullable<Int4>,
        view_count -> Nullable<Int4>,
        is_reply -> Nullable<Bool>,
        in_reply_to_status_id -> Nullable<Varchar>,
        in_reply_to_user_id -> Nullable<Varchar>,
        created_at_str -> Nullable<Varchar>,
        created_at_ts -> Nullable<Int8>,
        tweet_created_at -> Nullable<Timestamptz>,
        created_at -> Timestamptz,
        updated_at -> Nullable<Timestamptz>,
    }
}

// ============================================================
// Twitter Comments table
// ============================================================
table! {
    gm_agent_twitter_comments (id) {
        id -> Int4,
        tweet_db_id -> Int4,
        campaign_id -> Nullable<Int4>,
        twitter_comment_id -> Varchar,
        conversation_id -> Nullable<Varchar>,
        comment_screen_name -> Nullable<Varchar>,
        comment_user_name -> Nullable<Varchar>,
        comment_user_id -> Nullable<Varchar>,
        comment_user_followers -> Nullable<Int4>,
        comment_text -> Text,
        reason -> Nullable<Text>,
        suggested_reply -> Nullable<Text>,
        favorite_count -> Nullable<Int4>,
        retweet_count -> Nullable<Int4>,
        reply_count -> Nullable<Int4>,
        in_reply_to_status_id -> Nullable<Varchar>,
        is_reply -> Nullable<Bool>,
        media_urls -> Nullable<Array<Nullable<Text>>>,
        has_media -> Nullable<Bool>,
        created_at_str -> Nullable<Varchar>,
        created_at_ts -> Nullable<Int8>,
        comment_created_at -> Nullable<Timestamptz>,
        created_at -> Timestamptz,
        updated_at -> Nullable<Timestamptz>,
        suggested_dm -> Nullable<Text>,
        suggested_reply_post -> Nullable<Text>,
        status -> Nullable<Int2>,
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
diesel::joinable!(gm_agent_facebook_posts -> gm_crawler_tasks (task_id));
diesel::joinable!(gm_agent_facebook_posts -> gm_campaigns (campaign_id));
diesel::joinable!(gm_agent_facebook_comments -> gm_agent_facebook_posts (post_db_id));
diesel::joinable!(gm_agent_facebook_comments -> gm_campaigns (campaign_id));
diesel::joinable!(gm_agent_twitter_tweets -> gm_crawler_tasks (task_id));
diesel::joinable!(gm_agent_twitter_tweets -> gm_campaigns (campaign_id));
diesel::joinable!(gm_agent_twitter_comments -> gm_agent_twitter_tweets (tweet_db_id));
diesel::joinable!(gm_agent_twitter_comments -> gm_campaigns (campaign_id));

diesel::allow_tables_to_appear_in_same_query!(
    gm_platforms,
    gm_campaigns,
    gm_campaign_templates,
    gm_crawler_tasks,
    gm_agent_videos,
    gm_agent_comments,
    gm_agent_facebook_posts,
    gm_agent_facebook_comments,
    gm_agent_twitter_tweets,
    gm_agent_twitter_comments,
);
