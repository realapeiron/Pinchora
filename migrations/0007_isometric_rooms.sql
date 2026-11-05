-- Rooms became isometric: sizes and coordinates changed (no wall rows on the floor grid,
-- wall items now sit on two walls), so every placed item goes back to its owner's
-- inventory once and gets placed again in the new editor.
INSERT INTO inventory (user_id, item_id, quantity)
SELECT user_id, item_id, COUNT(*)::INTEGER
FROM placements
GROUP BY user_id, item_id
ON CONFLICT (user_id, item_id) DO UPDATE SET quantity = inventory.quantity + EXCLUDED.quantity;

DELETE FROM placements;
