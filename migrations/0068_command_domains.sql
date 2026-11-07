-- Split the oversized 0066 /setup tree. Never edit 0066: it is already applied.
-- Preserve old cooldown identities and independent constraints, including v1 constraints.
CREATE FUNCTION pinchora_command_path_v2(path TEXT) RETURNS TEXT
LANGUAGE sql IMMUTABLE AS $$
SELECT COALESCE((SELECT new || substr(path, length(old) + 1) FROM (VALUES
    ('setup moderation', 'safety moderation'),
    ('setup automod', 'safety automod'),
    ('setup verification', 'safety verification'),
    ('setup antinuke', 'safety antinuke'),
    ('setup tickets', 'community tickets'),
    ('setup starboard', 'community starboard'),
    ('setup events', 'community events'),
    ('setup birthdays', 'community birthdays'),
    ('setup utility', 'community utility'),
    ('setup community', 'community server'),
    ('setup feeds', 'integrations feeds'),
    ('setup stats', 'integrations stats'),
    ('setup invites', 'integrations invites'),
    ('setup leveling', 'progression leveling'),
    ('setup economy', 'progression economy'),
    ('setup achievements', 'progression achievements')
) AS moves(old, new) WHERE path = old OR starts_with(path, old || ' ')), path)
$$;

CREATE FUNCTION pinchora_command_targets_v2(path TEXT) RETURNS SETOF TEXT
LANGUAGE sql IMMUTABLE AS $$
SELECT canonical FROM (VALUES
    ('setup achievements add', 'progression achievements add'),
    ('setup achievements edit', 'progression achievements edit'),
    ('setup achievements grant', 'progression achievements grant'),
    ('setup achievements list', 'progression achievements list'),
    ('setup achievements remove', 'progression achievements remove'),
    ('setup antinuke disable', 'safety antinuke disable'),
    ('setup antinuke enable', 'safety antinuke enable'),
    ('setup antinuke events', 'safety antinuke events'),
    ('setup antinuke limits', 'safety antinuke limits'),
    ('setup antinuke status', 'safety antinuke status'),
    ('setup antinuke trust-add', 'safety antinuke trust-add'),
    ('setup antinuke trust-remove', 'safety antinuke trust-remove'),
    ('setup antinuke vip-add', 'safety antinuke vip-add'),
    ('setup antinuke vip-remove', 'safety antinuke vip-remove'),
    ('setup automod exempt-channel', 'safety automod exempt-channel'),
    ('setup automod exempt-role', 'safety automod exempt-role'),
    ('setup automod profile', 'safety automod profile'),
    ('setup automod show', 'safety automod show'),
    ('setup automod test', 'safety automod test'),
    ('setup automod toggle', 'safety automod toggle'),
    ('setup roles auto-add', 'setup roles auto-add'),
    ('setup roles auto-list', 'setup roles auto-list'),
    ('setup roles auto-remove', 'setup roles auto-remove'),
    ('setup roles auto-sticky', 'setup roles auto-sticky'),
    ('setup birthdays preview', 'community birthdays preview'),
    ('setup birthdays settings', 'community birthdays settings'),
    ('setup roles color-setup', 'setup roles color-setup'),
    ('setup economy critters', 'progression economy critters'),
    ('setup economy gambling', 'progression economy gambling'),
    ('setup welcome goodbye', 'setup welcome goodbye'),
    ('setup roles join-role', 'setup roles join-role'),
    ('setup leveling roles-add', 'progression leveling roles-add'),
    ('setup leveling roles-list', 'progression leveling roles-list'),
    ('setup leveling roles-mode', 'progression leveling roles-mode'),
    ('setup leveling roles-remove', 'progression leveling roles-remove'),
    ('setup leveling roles-sync', 'progression leveling roles-sync'),
    ('setup leveling settings', 'progression leveling settings'),
    ('setup moderation log', 'safety moderation log'),
    ('setup moderation log-events', 'safety moderation log-events'),
    ('setup moderation log-ignore', 'safety moderation log-ignore'),
    ('setup moderation settings', 'safety moderation settings'),
    ('setup economy partygames', 'progression economy partygames'),
    ('setup economy points-settings', 'progression economy points-settings'),
    ('setup economy rep-roles-add', 'progression economy rep-roles-add'),
    ('setup economy rep-roles-list', 'progression economy rep-roles-list'),
    ('setup economy rep-roles-remove', 'progression economy rep-roles-remove'),
    ('setup economy store-add', 'progression economy store-add'),
    ('setup economy store-list', 'progression economy store-list'),
    ('setup economy store-remove', 'progression economy store-remove'),
    ('setup community suggestions', 'community server suggestions'),
    ('setup economy surge', 'progression economy surge'),
    ('setup moderation warn-ladder', 'safety moderation warn-ladder'),
    ('setup welcome message', 'setup welcome message'),
    ('setup welcome card', 'setup welcome card'),
    ('setup leveling xp-clear', 'progression leveling xp-clear'),
    ('setup leveling xp-multiplier', 'progression leveling xp-multiplier'),
    ('setup leveling xp-show', 'progression leveling xp-show'),
    ('setup leveling xp-voice', 'progression leveling xp-voice'),
    ('setup stats counter-add', 'integrations stats counter-add'),
    ('setup stats counter-list', 'integrations stats counter-list'),
    ('setup stats counter-remove', 'integrations stats counter-remove'),
    ('setup commands custom-create', 'setup commands custom-create'),
    ('setup commands custom-delete', 'setup commands custom-delete'),
    ('setup commands custom-edit', 'setup commands custom-edit'),
    ('setup commands custom-info', 'setup commands custom-info'),
    ('setup commands custom-list', 'setup commands custom-list'),
    ('setup events template', 'community events template'),
    ('setup feeds add', 'integrations feeds add'),
    ('setup feeds edit', 'integrations feeds edit'),
    ('setup feeds list', 'integrations feeds list'),
    ('setup feeds remove', 'integrations feeds remove'),
    ('setup feeds test', 'integrations feeds test'),
    ('setup invites bonus-add', 'integrations invites bonus-add'),
    ('setup invites bonus-remove', 'integrations invites bonus-remove'),
    ('setup invites post', 'integrations invites post'),
    ('setup invites reward-add', 'integrations invites reward-add'),
    ('setup invites reward-remove', 'integrations invites reward-remove'),
    ('setup invites settings', 'integrations invites settings'),
    ('setup community plaza-edit', 'community server plaza-edit'),
    ('setup economy points-give', 'progression economy points-give'),
    ('setup economy points-take', 'progression economy points-take'),
    ('setup roles panel-add', 'setup roles panel-add'),
    ('setup roles panel-create', 'setup roles panel-create'),
    ('setup roles panel-post', 'setup roles panel-post'),
    ('setup roles panel-remove', 'setup roles panel-remove'),
    ('setup roles panel-temp', 'setup roles panel-temp'),
    ('setup utility schedule-add', 'community utility schedule-add'),
    ('setup utility schedule-list', 'community utility schedule-list'),
    ('setup utility schedule-pause', 'community utility schedule-pause'),
    ('setup utility schedule-preview', 'community utility schedule-preview'),
    ('setup utility schedule-remove', 'community utility schedule-remove'),
    ('setup utility schedule-resume', 'community utility schedule-resume'),
    ('setup utility schedule-run-now', 'community utility schedule-run-now'),
    ('setup starboard edit', 'community starboard edit'),
    ('setup starboard list', 'community starboard list'),
    ('setup starboard remove', 'community starboard remove'),
    ('setup starboard setup', 'community starboard setup'),
    ('setup starboard top', 'community starboard top'),
    ('setup starboard votes-add', 'community starboard votes-add'),
    ('setup starboard votes-list', 'community starboard votes-list'),
    ('setup starboard votes-remove', 'community starboard votes-remove'),
    ('setup utility sticky-remove', 'community utility sticky-remove'),
    ('setup utility sticky-set', 'community utility sticky-set'),
    ('setup utility sticky-show', 'community utility sticky-show'),
    ('setup tickets panel-create', 'community tickets panel-create'),
    ('setup tickets panel-delete', 'community tickets panel-delete'),
    ('setup tickets panel-edit', 'community tickets panel-edit'),
    ('setup tickets panel-list', 'community tickets panel-list'),
    ('setup tickets panel-post', 'community tickets panel-post'),
    ('setup tickets settings', 'community tickets settings'),
    ('setup verification message', 'safety verification message'),
    ('setup verification mode', 'safety verification mode'),
    ('setup verification post', 'safety verification post'),
    ('setup verification setup', 'safety verification setup'),
    ('setup utility voice-add', 'community utility voice-add'),
    ('setup utility voice-list', 'community utility voice-list'),
    ('setup utility voice-remove', 'community utility voice-remove'),
    ('setup leveling xp-add', 'progression leveling xp-add'),
    ('setup leveling xp-import-mee6', 'progression leveling xp-import-mee6'),
    ('setup leveling xp-remove', 'progression leveling xp-remove'),
    ('setup leveling xp-reset', 'progression leveling xp-reset'),
    ('setup leveling xp-reset-all', 'progression leveling xp-reset-all'),
    ('setup leveling xp-set', 'progression leveling xp-set')
) AS routes(previous, canonical) WHERE previous = path OR starts_with(previous, path || ' ')
$$;

CREATE FUNCTION pinchora_normalize_command_control_v2(config JSONB) RETURNS JSONB
LANGUAGE plpgsql IMMUTABLE AS $$
DECLARE
    normalized JSONB := pinchora_normalize_command_control_v1(config);
    original JSONB := COALESCE(normalized->'rules', '{}'::jsonb);
    rules JSONB := original;
    aliases JSONB := '{}'::jsonb;
    item RECORD;
    target TEXT;
BEGIN
    FOR item IN SELECT key, value FROM jsonb_each(original) LOOP
        IF item.key IN ('setup', 'setup achievements', 'setup achievements add', 'setup achievements edit', 'setup achievements grant', 'setup achievements list', 'setup achievements remove', 'setup antinuke', 'setup antinuke disable', 'setup antinuke enable', 'setup antinuke events', 'setup antinuke limits', 'setup antinuke status', 'setup antinuke trust-add', 'setup antinuke trust-remove', 'setup antinuke vip-add', 'setup antinuke vip-remove', 'setup automod', 'setup automod exempt-channel', 'setup automod exempt-role', 'setup automod profile', 'setup automod show', 'setup automod test', 'setup automod toggle', 'setup birthdays', 'setup birthdays preview', 'setup birthdays settings', 'setup community', 'setup community plaza-edit', 'setup community suggestions', 'setup economy', 'setup economy critters', 'setup economy gambling', 'setup economy partygames', 'setup economy points-give', 'setup economy points-settings', 'setup economy points-take', 'setup economy rep-roles-add', 'setup economy rep-roles-list', 'setup economy rep-roles-remove', 'setup economy store-add', 'setup economy store-list', 'setup economy store-remove', 'setup economy surge', 'setup events', 'setup events template', 'setup feeds', 'setup feeds add', 'setup feeds edit', 'setup feeds list', 'setup feeds remove', 'setup feeds test', 'setup invites', 'setup invites bonus-add', 'setup invites bonus-remove', 'setup invites post', 'setup invites reward-add', 'setup invites reward-remove', 'setup invites settings', 'setup leveling', 'setup leveling roles-add', 'setup leveling roles-list', 'setup leveling roles-mode', 'setup leveling roles-remove', 'setup leveling roles-sync', 'setup leveling settings', 'setup leveling xp-add', 'setup leveling xp-clear', 'setup leveling xp-import-mee6', 'setup leveling xp-multiplier', 'setup leveling xp-remove', 'setup leveling xp-reset', 'setup leveling xp-reset-all', 'setup leveling xp-set', 'setup leveling xp-show', 'setup leveling xp-voice', 'setup moderation', 'setup moderation log', 'setup moderation log-events', 'setup moderation log-ignore', 'setup moderation settings', 'setup moderation warn-ladder', 'setup starboard', 'setup starboard edit', 'setup starboard list', 'setup starboard remove', 'setup starboard setup', 'setup starboard top', 'setup starboard votes-add', 'setup starboard votes-list', 'setup starboard votes-remove', 'setup stats', 'setup stats counter-add', 'setup stats counter-list', 'setup stats counter-remove', 'setup tickets', 'setup tickets panel-create', 'setup tickets panel-delete', 'setup tickets panel-edit', 'setup tickets panel-list', 'setup tickets panel-post', 'setup tickets settings', 'setup utility', 'setup utility schedule-add', 'setup utility schedule-list', 'setup utility schedule-pause', 'setup utility schedule-preview', 'setup utility schedule-remove', 'setup utility schedule-resume', 'setup utility schedule-run-now', 'setup utility sticky-remove', 'setup utility sticky-set', 'setup utility sticky-show', 'setup utility voice-add', 'setup utility voice-list', 'setup utility voice-remove', 'setup verification', 'setup verification message', 'setup verification mode', 'setup verification post', 'setup verification setup') THEN
            rules := rules - item.key;
            FOR target IN SELECT * FROM pinchora_command_targets_v2(item.key) LOOP
                rules := jsonb_set(rules, ARRAY[target],
                    COALESCE(rules->target, '{}'::jsonb) || jsonb_build_object('constraints',
                        COALESCE(rules->target->'constraints', '{}'::jsonb)
                        || COALESCE(item.value->'constraints', '{}'::jsonb)
                        || jsonb_build_object(item.key, item.value - 'constraints')));
            END LOOP;
        END IF;
    END LOOP;
    FOR item IN SELECT key, value FROM jsonb_each_text(COALESCE(normalized->'aliases', '{}'::jsonb)) LOOP
        aliases := aliases || jsonb_build_object(item.key, pinchora_command_path_v2(item.value));
    END LOOP;
    RETURN normalized || jsonb_build_object('rules', rules, 'aliases', aliases);
END
$$;

CREATE OR REPLACE FUNCTION pinchora_command_paths_on_write() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    NEW.config := pinchora_normalize_command_control_v2(NEW.config);
    RETURN NEW;
END
$$;
UPDATE command_control SET config = pinchora_normalize_command_control_v2(config), revision = revision + 1
WHERE config IS DISTINCT FROM pinchora_normalize_command_control_v2(config);
