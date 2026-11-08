//! Read-only house companion; callers have already enforced the house's access policy.
use crate::{error::ApiResult, state::WebState};
use pinchora_core::i18n::Lang;
use pinchora_game::Content;
use serde_json::{Value, json};
pub(super) async fn snapshot(state: &WebState, owner: u64, viewer: u64) -> ApiResult<Value> {
    let Some(s) = state.app.db.reef_state_if_any(owner).await? else {
        return Ok(Value::Null);
    };
    let fish = state.app.db.reef_aquarium(owner).await?;
    Ok(json!({
        "name":s.aquarium_name,
        "fish":fish.iter().map(|f|json!({"slot":f.slot,"id":f.fish_id,"size_cm":f.size_cm,
            "name":Content::get().name(&f.fish_id,Lang::En),"name_tr":Content::get().name(&f.fish_id,Lang::Tr)})).collect::<Vec<_>>(),
        "garden":if owner==viewer {json!({"seaweed":s.seaweed,"planted_at":s.planted_at,"watered_at":s.watered_at})}else{Value::Null},
    }))
}
