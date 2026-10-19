//! State lookup by name: localised vanilla state names from the install and
//! each state's owner and controller as the political map shows them.
use super::ui::read;
use crate::{
    error::{Error, Result},
    game,
    memory::Reader,
    platform::Process,
};
use serde_json::{json, Value};
use std::path::Path;

/// Lower-case ASCII folding for the accented letters of European state names.
fn fold(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        let folded = match c {
            'ą' | 'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ă' => "a",
            'ć' | 'č' | 'ç' => "c",
            'ę' | 'é' | 'è' | 'ê' | 'ë' | 'ě' => "e",
            'í' | 'ì' | 'î' | 'ï' => "i",
            'ł' => "l",
            'ń' | 'ñ' | 'ň' => "n",
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' | 'ő' => "o",
            'ś' | 'š' | 'ș' | 'ş' => "s",
            'ź' | 'ż' | 'ž' => "z",
            'ú' | 'ù' | 'û' | 'ü' | 'ů' | 'ű' => "u",
            'ř' => "r",
            'ť' | 'ț' => "t",
            'ý' => "y",
            'ß' => "ss",
            c if c.is_ascii() => {
                out.push(c);
                continue;
            }
            _ => continue,
        };
        out.push_str(folded);
    }
    out
}

/// `STATE_<id>: "Name"` lines of the base (not country-specific) names.
fn parse_names(text: &str) -> Vec<(i32, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("STATE_") else {
            continue;
        };
        let Some((id, value)) = rest.split_once(':') else {
            continue;
        };
        let Ok(id) = id.parse::<i32>() else {
            continue;
        };
        let value = value
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .trim();
        if let Some(name) = value.strip_prefix('"').and_then(|v| v.rsplit_once('"')) {
            out.push((id, name.0.to_string()));
        }
    }
    out
}

fn names(dir: &Path) -> Result<Vec<(i32, String)>> {
    let text = std::fs::read_to_string(dir.join("localisation/english/state_names_l_english.yml"))
        .map_err(|_| Error::new("map_data_unavailable", "State names unreadable"))?;
    Ok(parse_names(&text))
}

fn tag_name(r: &Reader<'_>, image: u64, tag: i32) -> Result<String> {
    let g = game::layout(r);
    let db = r.u64(image + g.tag_db_global)?;
    let (_, data, count) = r.array_header(db)?;
    if tag <= 0 || tag as usize >= count {
        return Err(Error::new("invalid_tag", "Tag index outside the tag table"));
    }
    let name = r.string(data + tag as u64 * g.tag_stride)?;
    if name.len() != 3 || !name.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(Error::new("invalid_tag", "Unexpected tag name"));
    }
    Ok(name)
}

/// (owner, controller) tags of state `id`.
fn holders(r: &Reader<'_>, image: u64, id: i32) -> Result<(String, String)> {
    let g = game::layout(r);
    let l = crate::construction::layout(r);
    let gs = r.u64(image + g.gs_global)?;
    let state = u64::from_le_bytes(
        r.array_entry(gs + l.gs_states, id as usize, 8)?
            .try_into()
            .unwrap(),
    );
    if state == 0 || r.u64(state)? != image + l.state_vtable || r.i32(state + l.state_id)? != id {
        return Err(Error::new("invalid_target", "State not in the catalog"));
    }
    Ok((
        tag_name(r, image, r.i32(state + l.state_owner)?)?,
        tag_name(r, image, r.i32(state + l.state_controller)?)?,
    ))
}

/// States whose English name contains `query` (accent-insensitive), at most 25.
pub(super) fn lookup(process: &dyn Process, dir: &Path, query: &str) -> Result<Value> {
    let wanted = fold(query.trim());
    if wanted.is_empty() {
        return Err(Error::new("invalid_arguments", "Empty state query"));
    }
    let query = query.trim();
    // A country tag (e.g. GER, POL) lists the states that country owns.
    if query.len() == 3 && query.bytes().all(|b| b.is_ascii_uppercase()) {
        let all = names(dir)?;
        let centers = super::mapdata::state_centers(dir).ok();
        let mut rows = Vec::new();
        for (id, name) in all {
            if let Ok((owner, controller)) = read(process, |r, i| holders(r, i, id)) {
                if owner == query || controller == query {
                    let mut row =
                        json!({"state_id":id,"name":name,"owner":owner,"controller":controller});
                    if let Some((x, z)) = centers.as_ref().and_then(|c| c.get(&id)) {
                        row["x"] = json!(x.round());
                        row["z"] = json!(z.round());
                    }
                    rows.push(row);
                }
            }
        }
        return Ok(json!({"states":rows}));
    }
    let found: Vec<(i32, String)> = names(dir)?
        .into_iter()
        .filter(|(id, name)| fold(name).contains(&wanted) || id.to_string() == wanted)
        .take(25)
        .collect();
    let centers = super::mapdata::state_centers(dir).ok();
    let rows = found
        .into_iter()
        .map(|(id, name)| {
            let holders = read(process, |r, i| holders(r, i, id));
            let mut row = match holders {
                Ok((owner, controller)) => {
                    json!({"state_id":id,"name":name,"owner":owner,"controller":controller})
                }
                Err(_) => json!({"state_id":id,"name":name}),
            };
            if let Some((x, z)) = centers.as_ref().and_then(|c| c.get(&id)) {
                row["x"] = json!(x.round());
                row["z"] = json!(z.round());
            }
            Ok(row)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({"states":rows}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folds_accents_and_parses_base_names() {
        assert_eq!(fold("Stanisławów"), "stanislawow");
        assert_eq!(fold("Łódź"), "lodz");
        let text = "l_english:\n STATE_89: \"Stanisławów\"\n GER_STATE_89: \"Stanislau\"\n STATE_90:0 \"Kielce\"\n";
        assert_eq!(
            parse_names(text),
            vec![(89, "Stanisławów".to_string()), (90, "Kielce".to_string())]
        );
    }
}
