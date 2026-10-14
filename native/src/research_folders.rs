//! Loaded public research-folder keys, independent of open research GUI.
//! Layout/type/key traversal is reused from the independently recovered
//! current-build category_ui reader; no new offsets or input authority.
use crate::{
    category_ui,
    error::{Error, Result},
    game,
    memory::Reader,
    platform::Process,
    research_ui::Witnesses,
};
use serde_json::{json, Value};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Exact installed public keys in sorted order. Names are never aliases,
/// display labels, technology membership or an eligibility determination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderCatalog {
    pub ids: Vec<String>,
}
impl FolderCatalog {
    pub fn contains(&self, id: &str) -> bool {
        self.ids.iter().any(|key| key == id)
    }
}

/// Fresh catalog and own campaign context from the same witnessed bracket.
/// Owners must compare this context with their immutable campaign binding.
pub fn owned_snapshot(process: &dyn Process) -> Result<(FolderCatalog, game::ClockContext)> {
    let (catalog, context, _) = observe(process)?;
    Ok((catalog, context))
}

/// Read-only protocol projection; no process addresses or action authority.
pub fn snapshot(process: &dyn Process) -> Result<Value> {
    let (catalog, context, freshness) = observe(process)?;
    Ok(json!({
        "source":"owned_process_memory",
        "build_id":game::BUILD_ID,
        "scope":"loaded_public_definitions",
        "own_player":context.player,
        "ids":catalog.ids,
        "count":catalog.ids.len(),
        "requires_open_research_view":false,
        "technology_folder_mapping_available":false,
        "eligibility_available":false,
        "geometry_available":false,
        "input_authorized":false,
        "observation_paused_game":false,
        "freshness":freshness
    }))
}

fn observe(process: &dyn Process) -> Result<(FolderCatalog, game::ClockContext, Value)> {
    let start = Instant::now();
    process.begin_snapshot()?;
    let witnesses = Witnesses::new(process);
    let reader = Reader::new(&witnesses);
    let base = process.image_base();
    // Positive own-player/campaign checks precede public database traversal.
    // No GUI manager, research view, slot widget or callback is inspected.
    let context = game::context_from_reader(&reader, base)?;
    let folders = category_ui::canonical_folders(&reader, base)?;
    if folders.database != context.anchors[2] {
        return Err(Error::new(
            "torn_read",
            "Research folder database differs from campaign context",
        ));
    }
    let catalog = FolderCatalog {
        ids: folders.ids.into_iter().collect(),
    };
    let (end_calls, end_bytes) = witnesses.finish()?;
    process.check()?;
    let freshness = json!({
        "observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
        "elapsed_ms":start.elapsed().as_secs_f64()*1000.0,
        "read_calls":reader.calls.get()+end_calls as u64,
        "read_bytes":reader.bytes.get()+end_bytes
    });
    Ok((catalog, context, freshness))
}
