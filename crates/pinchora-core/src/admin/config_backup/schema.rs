/// A configuration key; records without a unique editor key retain their local ids.
pub struct Section {
    pub table: &'static str,
    pub columns: &'static str,
    pub key: &'static str,
    pub serial: bool,
}
macro_rules! section {
    ($table:literal, $cols:literal, $key:literal, $serial:literal) => {
        Section {
            table: $table,
            columns: $cols,
            key: $key,
            serial: $serial,
        }
    };
}
/// Reef records are player data, never server configuration (including moderation evidence).
pub const EXCLUDED_REEF_TABLES: &[&str] = &[
    "reef_consents",
    "reef_sanctions",
    "reef_chat_evidence",
    "reef_chat_recipients",
    "reef_reports",
    "reef_moderator_audit",
];
pub const SECTIONS: &[Section] = &[
    section!(
        "guilds",
        "leveling_enabled,levelup_channel_id,levelup_message,welcome_enabled,welcome_channel_id,welcome_message,welcome_theme,welcome_title,welcome_accent,level_roles_stack,points_enabled,points_name,points_emoji,points_min,points_max,join_role_id,goodbye_channel_id,goodbye_message,log_channel_id,log_members,log_names,log_bans,log_voice,levelup_mode,voice_xp_enabled,voice_xp_amount,gambling_enabled,prefix,language,sticky_roles,stats_public",
        "",
        false
    ),
    section!("guild_modules", "module,enabled,channel_mode,channels", "module", false),
    section!("command_control", "config", "", false),
    section!(
        "tracker_settings",
        "patch_channels,mc_watch,mc_news_channel_id",
        "",
        false
    ),
    // AFK policy is stored in guild_modules/command_control; member AFK records stay private.
    section!("utility_settings", "timezone", "", false),
    section!(
        "sticky_messages",
        "channel_id,content,embed,min_gap_secs,min_messages,enabled",
        "channel_id",
        false
    ),
    section!(
        "scheduled_messages",
        "channel_id,content,embed,schedule,tz,next_run_at,enabled",
        "id",
        true
    ),
    section!("event_settings", "timezone", "", false),
    section!("card_uploads", "id,png,width,height", "id", false),
    section!(
        "guild_branding",
        "nickname,avatar_upload_id,banner_upload_id,bio,embed_color",
        "",
        false
    ),
    section!("card_designs", "kind,design,delivery", "kind", false),
    section!("mod_settings", "log_channel_id,dm_members", "", false),
    section!("mod_parity_settings", "config", "", false),
    section!("mod_warn_thresholds", "warns,action,duration_secs", "warns", false),
    section!(
        "log_settings",
        "disabled_events,ignored_channels,ignored_roles,ignore_bots",
        "",
        false
    ),
    section!("log_channels", "category,channel_id", "category", false),
    section!("level_roles", "level,role_id", "role_id", false),
    section!("rep_roles", "reputation,role_id", "role_id", false),
    section!("xp_rules", "kind,target_id,multiplier", "kind,target_id", false),
    section!("store_items", "role_id,price,stock", "role_id", false),
    section!("suggestion_settings", "channel_id", "", false),
    section!(
        "party_settings",
        "surge_enabled,surge_channel_id,surge_every_hours",
        "",
        false
    ),
    section!("collection_settings", "enabled,channel_id", "", false),
    section!(
        "auto_roles",
        "role_id,kind,delay_secs,remove_after_secs,enabled",
        "role_id,kind",
        true
    ),
    section!("color_settings", "enabled,allowed_roles,mode,anchor_role_id", "", false),
    section!("color_roles", "role_id,hex,name", "role_id", false),
    section!(
        "role_panels",
        "title,description,channel_id,style,exclusive,required_role,min_picks,max_picks",
        "title",
        true
    ),
    section!(
        "role_panel_roles",
        "panel_id,role_id,label,emoji,position,description,emoji_id,temp_secs",
        "panel_id,role_id",
        false
    ),
    section!("panel_reactions", "panel_id,emoji,role_id", "panel_id,emoji", false),
    section!(
        "ticket_settings",
        "open_category_id,closed_category_id,log_channel_id,auto_close_hours,dm_transcript,rating_enabled,naming,mode,max_open_per_user",
        "",
        false
    ),
    section!("ticket_forms", "name,questions", "name", true),
    section!(
        "ticket_panels",
        "name,channel_id,title,description,color,style,buttons,form_id,support_roles,welcome_text,open_category_id,log_channel_id,enabled",
        "name",
        true
    ),
    section!(
        "automod_settings",
        "profile,escalation,dm_on_action,alert_channel_id",
        "",
        false
    ),
    section!(
        "automod_rules",
        "name,kind,enabled,log_only,native,params,actions,exempt_roles,exempt_channels,only_channels",
        "name",
        true
    ),
    section!(
        "feeds",
        "kind,source,display_name,channel_id,message,embed,ping_role_id,filter_include,filter_exclude,enabled",
        "kind,source,channel_id",
        true
    ),
    section!(
        "starboards",
        "name,channel_id,emoji,threshold,self_star,allow_bots,only_channels,except_channels,require_image,color,role_weights,remove_below,enabled",
        "channel_id",
        true
    ),
    section!(
        "vote_channels",
        "channel_id,up_emoji,down_emoji,only_media,create_thread,enabled",
        "channel_id",
        false
    ),
    section!(
        "counter_channels",
        "channel_id,kind,role_id,template",
        "channel_id",
        true
    ),
    section!(
        "invite_settings",
        "enabled,log_channel_id,fake_days,temp_enabled,temp_channel_id,temp_max_age_secs,temp_max_uses,temp_per_user_daily",
        "",
        false
    ),
    section!("invite_rewards", "invites,role_id", "invites", false),
    section!(
        "roblox_settings",
        "enabled,verified_role_id,unverified_role_id,nickname_template,verify_channel_id,require_verified",
        "",
        false
    ),
    section!(
        "roblox_binds",
        "kind,group_id,rank_min,rank_max,role_id,remove_if_unmet",
        "id",
        true
    ),
    section!(
        "verification",
        "enabled,mode,verified_role_id,unverified_role_id,channel_id,kick_after_mins,min_account_age_hours,block_default_avatar,block_name_patterns,dm_on_block,log_channel_id,message,rule_action,exempt_roles",
        "",
        false
    ),
    section!(
        "raid_settings",
        "enabled,join_threshold,window_secs,action,lockdown_minutes,alert_channel_id,similar_threshold,similar_names,similar_avatars,new_account_hours,channels,slowmode_secs,lock_channels,pause_invites",
        "",
        false
    ),
    section!(
        "voice_creators",
        "channel_id,category_id,name_template,default_limit,default_locked,max_per_user,panel,panel_channel_id,enabled",
        "channel_id",
        false
    ),
    section!(
        "custom_commands",
        "name,description,responses,embed,reply_mode,actions,allow_roles,deny_roles,allow_channels,deny_channels,cooldown_secs,enabled,allow_mentions",
        "name",
        true
    ),
    section!(
        "custom_responders",
        "name,trigger_kind,trigger_value,match_kind,responses,embed,actions,destination,channel_id,chance,allow_channels,deny_channels,cooldown_secs,enabled,allow_mentions",
        "id",
        true
    ),
    section!(
        "qotd_settings",
        "channel_id,ping_role_id,hour,timezone,thread_per_question",
        "",
        false
    ),
    section!("qotd_questions", "text,text_tr,approved", "text", true),
    section!(
        "counting_channels",
        "channel_id,reset_on_fail,allow_math,fail_role_id",
        "channel_id",
        false
    ),
    section!(
        "forms",
        "name,questions,submit_channel_id,review_roles,accept_role_id,dm_result,open",
        "name",
        true
    ),
    section!(
        "modmail_settings",
        "enabled,forum_channel_id,staff_roles,greeting,close_after_hours,anonymous_staff,log_channel_id",
        "",
        false
    ),
    section!("modmail_snippets", "name,text", "name", false),
];
