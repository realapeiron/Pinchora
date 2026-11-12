UPDATE guild_modules
SET enabled = TRUE
WHERE module = 'command_control' AND enabled = FALSE;
