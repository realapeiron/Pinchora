-- Seashell Confessions uses the existing party poll rows and vote table.
ALTER TABLE party_games DROP CONSTRAINT party_games_kind_check;
ALTER TABLE party_games ADD CONSTRAINT party_games_kind_check
    CHECK (kind IN ('hotpearl', 'acronym', 'postcard', 'wordrace',
                   'truthordare', 'wouldyourrather', 'nhie'));
