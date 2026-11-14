use super::*;

impl Db {
    /// Preview is a revision-bound confirmation: concurrent members or point changes invalidate it.
    pub async fn economy_points_preview(&self, guild: u64) -> anyhow::Result<Value> {
        let (count,total,version):(i64,String,String)=sqlx::query_as("SELECT count(*),COALESCE(sum(points),0)::text,md5($1::bigint::text||':'||COALESCE(string_agg(user_id::text||':'||points::text,',' ORDER BY user_id),'')) FROM members WHERE guild_id=$1").bind(guild as i64).fetch_one(self.pool()).await?;
        Ok(json!({"count":count,"total":total,"version":version}))
    }
    pub async fn economy_scale_points(&self, guild: u64, percent: i32, version: &str) -> anyhow::Result<u64> {
        self.economy_scale_points_audited(guild, percent, version, None).await
    }
    pub async fn economy_scale_points_audited(
        &self,
        guild: u64,
        percent: i32,
        version: &str,
        actor: Option<&EconomyAudit<'_>>,
    ) -> anyhow::Result<u64> {
        require((0..=100).contains(&percent), "invalid")?;
        retry("economy_scale_points",||async {
   let mut tx=self.pool().begin().await?;
   let users:Vec<i64>=sqlx::query_scalar("SELECT user_id FROM members WHERE guild_id=$1 ORDER BY user_id").bind(guild as i64).fetch_all(&mut *tx).await?;
   require(users.len()<=10000,"cap")?;
   let mut locked = users.clone();
   if let Some(actor) = actor { locked.push(actor.actor as i64); }
   lock_users(&mut tx,&locked).await?;
   // A late member must not be waited on out of order.
   let late:Vec<i64>=sqlx::query_scalar("SELECT user_id FROM members WHERE guild_id=$1 AND NOT(user_id=ANY($2))").bind(guild as i64).bind(&users).fetch_all(&mut *tx).await?;
   require(late.is_empty(),"changed")?;
   let current:String=sqlx::query_scalar("SELECT md5($1::bigint::text||':'||COALESCE(string_agg(user_id::text||':'||points::text,',' ORDER BY user_id),'')) FROM members WHERE guild_id=$1").bind(guild as i64).fetch_one(&mut *tx).await?;
   require(current==version,"changed")?;
   let before: String=sqlx::query_scalar("SELECT COALESCE(sum(points),0)::text FROM members WHERE guild_id=$1 AND user_id=ANY($2)").bind(guild as i64).bind(&users).fetch_one(&mut *tx).await?;
   let n=sqlx::query("UPDATE members SET points=floor(points::numeric*$2/100)::bigint WHERE guild_id=$1 AND user_id=ANY($3)").bind(guild as i64).bind(percent).bind(&users).execute(&mut *tx).await?.rows_affected();
   let after: String=sqlx::query_scalar("SELECT COALESCE(sum(points),0)::text FROM members WHERE guild_id=$1 AND user_id=ANY($2)").bind(guild as i64).bind(&users).fetch_one(&mut *tx).await?;
   if before != after {
       admin_audit(&mut tx,guild,actor,"economy.points_scale",json!({"count":users.len(),"total":before,"version":version}),json!({"count":n,"total":after,"percent":percent})).await?;
   }
   tx.commit().await?;Ok(n)
  }).await
    }
}
