//! Card style and the profile showcase: what the player chose, and what they may choose from.

use crate::{
    Db,
    house::{HouseRow, PlacementRow, RoomRow},
    sql_id,
};

/// A fish in the player's fish book.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FishBookRow {
    pub fish_id: String,
    pub caught: i32,
    pub best_cm: f32,
}

/// What a thumbnail of the player's house is drawn from: the first room, as it stands.
#[derive(Debug, Clone)]
pub struct HousePreview {
    pub house: HouseRow,
    pub room: RoomRow,
    pub placements: Vec<PlacementRow>,
}

/// Sets one column of the player's `users` row, making the row if there is none.
macro_rules! set_user_column {
    ($db:expr, $user:expr, $column:literal, $value:expr) => {{
        sqlx::query(concat!(
            "INSERT INTO users (id, ",
            $column,
            ") VALUES ($1, $2) ON CONFLICT (id) DO UPDATE SET ",
            $column,
            " = EXCLUDED.",
            $column
        ))
        .bind(sql_id($user))
        .bind($value)
        .execute($db.pool())
        .await?;
        Ok(())
    }};
}

impl Db {
    /// Saves the accent colour as `#rrggbb`, or `None` to follow the background again.
    pub async fn set_accent(&self, user: u64, accent: Option<&str>) -> anyhow::Result<()> {
        if let Some(accent) = accent {
            anyhow::ensure!(
                accent.len() == 7
                    && accent.starts_with('#')
                    && accent[1..].chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')),
                "an accent is `#rrggbb` in lower case"
            );
        }
        set_user_column!(self, user, "accent", accent)
    }

    /// Saves how see-through the panels are (0 light, 1 normal, 2 solid).
    pub async fn set_panel_opacity(&self, user: u64, opacity: i16) -> anyhow::Result<()> {
        anyhow::ensure!((0..=2).contains(&opacity), "opacity is 0, 1 or 2");
        set_user_column!(self, user, "panel_opacity", opacity)
    }

    /// Saves the frame id. The caller has checked that it exists and that the player may wear it.
    pub async fn set_frame(&self, user: u64, frame: &str) -> anyhow::Result<()> {
        set_user_column!(self, user, "frame", frame)
    }

    /// Back to the card look every player starts with.
    pub async fn reset_card_style(&self, user: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE users SET accent = NULL, panel_opacity = 1, frame = 'plain' WHERE id = $1")
            .bind(sql_id(user))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Saves the showcase fish. The caller has checked it is in the fish book.
    pub async fn set_showcase_fish(&self, user: u64, fish: Option<&str>) -> anyhow::Result<()> {
        set_user_column!(self, user, "showcase_fish", fish)
    }

    /// Saves the showcase pet by its id. The caller has checked it is theirs.
    pub async fn set_showcase_pet(&self, user: u64, pet: Option<i64>) -> anyhow::Result<()> {
        set_user_column!(self, user, "showcase_pet", pet)
    }

    /// Saves the showcase furniture item. The caller has checked they own or have placed it.
    pub async fn set_showcase_item(&self, user: u64, item: Option<&str>) -> anyhow::Result<()> {
        set_user_column!(self, user, "showcase_item", item)
    }

    /// Shows or hides the house thumbnail.
    pub async fn set_showcase_house(&self, user: u64, shown: bool) -> anyhow::Result<()> {
        set_user_column!(self, user, "showcase_house", shown)
    }

    /// Empties the showcase: no fish, pet, furniture or house.
    pub async fn clear_showcase(&self, user: u64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE users SET showcase_fish = NULL, showcase_pet = NULL, showcase_item = NULL,
                              showcase_house = FALSE WHERE id = $1",
        )
        .bind(sql_id(user))
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The fish in the player's book, the one caught first at the front.
    pub async fn fish_book(&self, user: u64) -> anyhow::Result<Vec<FishBookRow>> {
        Ok(sqlx::query_as::<_, FishBookRow>(
            "SELECT fish_id, caught, best_cm FROM fish_book WHERE user_id = $1 ORDER BY first_at, fish_id",
        )
        .bind(sql_id(user))
        .fetch_all(self.pool())
        .await?)
    }

    /// Furniture ids the player has: in the inventory or standing in a room, each once, by id.
    pub async fn owned_item_ids(&self, user: u64) -> anyhow::Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT item_id FROM inventory WHERE user_id = $1 AND quantity > 0
             UNION SELECT item_id FROM placements WHERE user_id = $1
             ORDER BY item_id",
        )
        .bind(sql_id(user))
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// The first room of the player's house, or `None` if they never opened their house.
    pub async fn house_preview(&self, user: u64) -> anyhow::Result<Option<HousePreview>> {
        let id = sql_id(user);
        let Some(house) =
            sqlx::query_as::<_, HouseRow>("SELECT user_id, tier, name, locked, likes FROM houses WHERE user_id = $1")
                .bind(id)
                .fetch_optional(self.pool())
                .await?
        else {
            return Ok(None);
        };
        let Some(room) = sqlx::query_as::<_, RoomRow>(
            "SELECT idx, room_type, wallpaper, floor FROM rooms WHERE user_id = $1 ORDER BY idx LIMIT 1",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?
        else {
            return Ok(None);
        };
        let placements = self.placements(user, room.idx).await?;
        Ok(Some(HousePreview {
            house,
            room,
            placements,
        }))
    }
}
