//! Reading a house as it is, for screens that must not move anyone in.

use crate::{Db, HouseRow, RoomRow, sql_id};

impl Db {
    /// The player's house and its rooms, or `None` when they never moved in. Unlike [`Db::house`]
    /// this creates nothing, so a page that only looks cannot hand out the starter kit.
    pub async fn house_if_any(&self, user_id: u64) -> anyhow::Result<Option<(HouseRow, Vec<RoomRow>)>> {
        let id = sql_id(user_id);
        let Some(house) =
            sqlx::query_as::<_, HouseRow>("SELECT user_id, tier, name, locked, likes FROM houses WHERE user_id = $1")
                .bind(id)
                .fetch_optional(self.pool())
                .await?
        else {
            return Ok(None);
        };
        let rooms = sqlx::query_as::<_, RoomRow>(
            "SELECT idx, room_type, wallpaper, floor FROM rooms WHERE user_id = $1 ORDER BY idx",
        )
        .bind(id)
        .fetch_all(self.pool())
        .await?;
        Ok(Some((house, rooms)))
    }

    /// How many things stand or hang in each of the player's rooms: `(room index, count)`.
    pub async fn placement_counts(&self, user_id: u64) -> anyhow::Result<Vec<(i16, i64)>> {
        Ok(sqlx::query_as::<_, (i16, i64)>(
            "SELECT room_idx, COUNT(*) FROM placements WHERE user_id = $1 GROUP BY room_idx ORDER BY room_idx",
        )
        .bind(sql_id(user_id))
        .fetch_all(self.pool())
        .await?)
    }
}
