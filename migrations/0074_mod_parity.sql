-- D11 configuration is portable; notes, evidence and active sanctions are not.
CREATE TABLE mod_parity_settings (guild_id BIGINT PRIMARY KEY, config JSONB NOT NULL DEFAULT '{}');
CREATE TABLE mod_notes (
 id BIGSERIAL PRIMARY KEY, guild_id BIGINT NOT NULL, user_id BIGINT NOT NULL, author_id BIGINT NOT NULL,
 body TEXT NOT NULL CHECK(char_length(body) BETWEEN 1 AND 1000), created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX mod_notes_member ON mod_notes(guild_id,user_id,id);
CREATE TABLE mod_reports (
 id BIGSERIAL PRIMARY KEY, guild_id BIGINT NOT NULL, user_id BIGINT NOT NULL, reporter_id BIGINT NOT NULL,
 channel_id BIGINT, message_id BIGINT, body TEXT NOT NULL CHECK(char_length(body) BETWEEN 1 AND 1000),
 status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','accepted','rejected')),
 reviewer_id BIGINT, created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX mod_reports_queue ON mod_reports(guild_id,status,id);
CREATE TABLE mod_report_blocks (guild_id BIGINT NOT NULL,user_id BIGINT NOT NULL,PRIMARY KEY(guild_id,user_id));
CREATE TABLE mod_appeals (
 id BIGSERIAL PRIMARY KEY,guild_id BIGINT NOT NULL,case_number INTEGER NOT NULL,user_id BIGINT NOT NULL,
 body TEXT NOT NULL CHECK(char_length(body) BETWEEN 10 AND 2000),
 status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','accepted','rejected')),
 reviewer_id BIGINT, decision TEXT CHECK(char_length(decision)<=512),created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 UNIQUE(guild_id,case_number,user_id)
);
CREATE TABLE mod_restrictions (
 guild_id BIGINT NOT NULL,user_id BIGINT NOT NULL,role_id BIGINT NOT NULL,role_ids BIGINT[] NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('mute','jail')),created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 PRIMARY KEY(guild_id,user_id)
);
CREATE TABLE mod_voice_bans (guild_id BIGINT NOT NULL,user_id BIGINT NOT NULL,PRIMARY KEY(guild_id,user_id));
ALTER TABLE mod_cases DROP CONSTRAINT mod_cases_kind_check;
ALTER TABLE mod_cases ADD CONSTRAINT mod_cases_kind_check CHECK (kind IN (
 'ban','unban','kick','timeout','untimeout','warn','unwarn','purge','slowmode','lock','unlock','nick','role_add','role_remove',
 'note','report','appeal','mute','unmute','jail','unjail','softban','voice','role_edit'));

-- Preserve policy constraints and cooldown identities when the remaining moderation roots move.
CREATE FUNCTION pinchora_command_path_v3(path TEXT) RETURNS TEXT LANGUAGE sql IMMUTABLE AS $$
 SELECT COALESCE((SELECT new FROM (VALUES
 ('ban', 'mod ban'), ('kick', 'mod kick'), ('timeout', 'mod timeout'), ('warn', 'mod warn'), ('purge', 'mod purge'),
 ('modtools notes', 'mod notes'), ('modtools report', 'mod report'), ('modtools restrict', 'mod restrict'),
 ('modtools mass', 'mod mass'), ('modtools voice', 'mod voice'), ('modtools roles', 'mod roles'),
 ('modtools names', 'mod names'), ('modtools policy', 'safety moderation policy')
 ) AS paths(old,new) WHERE path=old),path)
$$;
CREATE FUNCTION pinchora_normalize_command_control_v3(config JSONB) RETURNS JSONB LANGUAGE plpgsql IMMUTABLE AS $$
DECLARE normalized JSONB := pinchora_normalize_command_control_v2(config);
 rules JSONB := COALESCE(normalized->'rules','{}'::jsonb); aliases JSONB := '{}'::jsonb; item RECORD; target TEXT;
BEGIN
 FOR item IN SELECT key,value FROM jsonb_each(rules) LOOP
  target := pinchora_command_path_v3(item.key);
  IF target <> item.key THEN
   rules := rules-item.key;
   rules := jsonb_set(rules,ARRAY[target],COALESCE(rules->target,'{}'::jsonb) || jsonb_build_object('constraints',
    COALESCE(rules->target->'constraints','{}'::jsonb) || COALESCE(item.value->'constraints','{}'::jsonb) || jsonb_build_object(item.key,item.value-'constraints')));
  END IF;
 END LOOP;
 FOR item IN SELECT key,value FROM jsonb_each_text(COALESCE(normalized->'aliases','{}'::jsonb)) LOOP
  aliases := aliases || jsonb_build_object(item.key,pinchora_command_path_v3(item.value));
 END LOOP;
 RETURN normalized || jsonb_build_object('rules',rules,'aliases',aliases);
END
$$;
CREATE OR REPLACE FUNCTION pinchora_command_paths_on_write() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN NEW.config := pinchora_normalize_command_control_v3(NEW.config); RETURN NEW; END
$$;
UPDATE command_control SET config=pinchora_normalize_command_control_v3(config),revision=revision+1
 WHERE config IS DISTINCT FROM pinchora_normalize_command_control_v3(config);

CREATE TABLE mod_booster_roles (guild_id BIGINT NOT NULL,user_id BIGINT NOT NULL,role_id BIGINT NOT NULL,PRIMARY KEY(guild_id,user_id),UNIQUE(guild_id,role_id));
