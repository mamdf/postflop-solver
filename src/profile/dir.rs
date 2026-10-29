//! Directory format of a [`StrategyProfile`] (feature `profile-io`). One directory per profile:
//!
//! - `header.json`: `{"version": 2, "source", "starting_pot", "amount_convention": "raise-to",
//!   "hands": [["AsKd", ..] OOP, [..] IP]}`
//! - `nodes.jsonl`: one `{"line": ["X", "B:50", "7h", ..], "player", "actions": ["F", "C", ..],
//!   "offset"}` per action node (tokens of [`LineStep`] / [`ActionKey`])
//! - `strat.f32`: little-endian `f32`, node strategy at `offset`, layout `[action * #hands + hand]`,
//!   NaN = not covered
//!
//! Version 1 (no `version`; GTOpen exporter and the old `xeval` tool) is also read: hands are
//! `[c1, c2]` card ids and nodes have an index `"path"` instead of `"line"` (child tokens joined by
//! `/`: action index at action nodes, `c<card id>` at chance nodes).

use super::*;
use serde_json::{json, Value};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;

const CHUNK: usize = 1 << 16;

fn io_err(path: &Path, e: impl std::fmt::Display) -> String {
    format!("{}: {e}", path.display())
}

/// Writes `profile` in the version 2 format, creating `dir` if needed.
pub fn write_dir(profile: &StrategyProfile, dir: impl AsRef<Path>) -> Result<(), String> {
    let dir = dir.as_ref();
    std::fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;

    let hands = [
        holes_to_strings(profile.hands(0))?,
        holes_to_strings(profile.hands(1))?,
    ];
    let header = json!({
        "version": 2,
        "source": profile.source,
        "starting_pot": profile.starting_pot,
        "amount_convention": "raise-to",
        "hands": hands,
    });
    let path = dir.join("header.json");
    std::fs::write(&path, header.to_string()).map_err(|e| io_err(&path, e))?;

    let path = dir.join("nodes.jsonl");
    let mut out = BufWriter::new(File::create(&path).map_err(|e| io_err(&path, e))?);
    for node in profile.nodes() {
        let line: Vec<String> = node.line.iter().map(LineStep::to_token).collect();
        let actions: Vec<String> = node.actions.iter().map(ActionKey::to_token).collect();
        let rec =
            json!({"line": line, "player": node.player, "actions": actions, "offset": node.offset});
        writeln!(out, "{rec}").map_err(|e| io_err(&path, e))?;
    }
    out.flush().map_err(|e| io_err(&path, e))?;

    let path = dir.join("strat.f32");
    let mut out = BufWriter::new(File::create(&path).map_err(|e| io_err(&path, e))?);
    for v in &profile.data {
        out.write_all(&v.to_le_bytes())
            .map_err(|e| io_err(&path, e))?;
    }
    out.flush().map_err(|e| io_err(&path, e))
}

/// Reads a profile directory (version 1 or 2).
pub fn read_dir(dir: impl AsRef<Path>) -> Result<StrategyProfile, String> {
    let dir = dir.as_ref();
    let path = dir.join("header.json");
    let text = std::fs::read_to_string(&path).map_err(|e| io_err(&path, e))?;
    let header: Value = serde_json::from_str(&text).map_err(|e| io_err(&path, e))?;
    let v1 = header.get("version").is_none();
    if !v1 && header["version"] != 2 {
        return Err(io_err(&path, "unsupported version"));
    }
    if let Some(conv) = header.get("amount_convention") {
        if conv != "raise-to" {
            return Err(io_err(
                &path,
                format!("unsupported amount convention {conv}"),
            ));
        }
    }
    let hand = |h: &Value| -> Option<Hand> {
        match h {
            Value::String(s) => parse_hand(s).ok(),
            Value::Array(a) if a.len() == 2 => {
                let card = |v: &Value| v.as_u64().filter(|&c| c < 52).map(|c| c as Card);
                Some((card(&a[0])?, card(&a[1])?))
            }
            _ => None,
        }
    };
    let hands = [0, 1].map(|p| -> Result<Vec<Hand>, String> {
        let list = header["hands"][p]
            .as_array()
            .ok_or_else(|| io_err(&path, "bad hands"))?;
        list.iter()
            .map(|h| hand(h).ok_or_else(|| io_err(&path, format!("bad hand {h}"))))
            .collect()
    });
    let [hands0, hands1] = hands;
    let pot = header["starting_pot"]
        .as_f64()
        .ok_or_else(|| io_err(&path, "bad starting_pot"))?;
    let source = header["source"].as_str().unwrap_or("");
    let mut profile =
        StrategyProfile::new(source, pot, [hands0?, hands1?]).map_err(|e| io_err(&path, e))?;

    let path = dir.join("strat.f32");
    profile.data = read_f32s(&path)?;

    let path = dir.join("nodes.jsonl");
    let file = File::open(&path).map_err(|e| io_err(&path, e))?;
    // version 1: (path, player, actions, offset), converted once every parent is known
    let mut records = Vec::new();
    for (i, text) in BufReader::new(file).lines().enumerate() {
        let text = text.map_err(|e| io_err(&path, e))?;
        if text.trim().is_empty() {
            continue;
        }
        let at = |e: String| io_err(&path, format!("line {}: {e}", i + 1));
        let rec: Value = serde_json::from_str(&text).map_err(|e| at(e.to_string()))?;
        let actions = rec["actions"]
            .as_array()
            .ok_or_else(|| at("bad actions".into()))?
            .iter()
            .map(|a| ActionKey::from_token(a.as_str().unwrap_or("")))
            .collect::<Result<Vec<_>, _>>()
            .map_err(at)?;
        let player = rec["player"]
            .as_u64()
            .ok_or_else(|| at("bad player".into()))? as usize;
        let offset = rec["offset"]
            .as_u64()
            .ok_or_else(|| at("bad offset".into()))? as usize;
        if v1 {
            let p = rec["path"].as_str().ok_or_else(|| at("bad path".into()))?;
            records.push((p.to_string(), player, actions, offset));
            continue;
        }
        let tokens = rec["line"]
            .as_array()
            .ok_or_else(|| at("bad line".into()))?;
        let line = tokens
            .iter()
            .map(|t| LineStep::from_token(t.as_str().unwrap_or("")))
            .collect::<Result<Vec<_>, _>>()
            .map_err(at)?;
        let node = ProfileNode {
            line,
            player,
            actions,
            offset,
        };
        profile.insert_node(node).map_err(at)?;
    }

    // version 1: action indices refer to the actions of the node at the path prefix
    let by_path: HashMap<&str, &[ActionKey]> = records
        .iter()
        .map(|r| (r.0.as_str(), r.2.as_slice()))
        .collect();
    let lines = records
        .iter()
        .map(|r| path_to_line(&r.0, &by_path))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| io_err(&path, e))?;
    for ((_, player, actions, offset), line) in records.into_iter().zip(lines) {
        let node = ProfileNode {
            line,
            player,
            actions,
            offset,
        };
        profile.insert_node(node).map_err(|e| io_err(&path, e))?;
    }
    Ok(profile)
}

fn path_to_line(
    path: &str,
    by_path: &HashMap<&str, &[ActionKey]>,
) -> Result<Vec<LineStep>, String> {
    let mut line = Vec::new();
    if path.is_empty() {
        return Ok(line);
    }
    let mut start = 0;
    for token in path.split('/') {
        let prefix = &path[..start.max(1) - 1];
        start += token.len() + 1;
        let bad = || format!("bad path {path:?}");
        if let Some(card) = token.strip_prefix('c') {
            let card: Card = card.parse().ok().filter(|&c| c < 52).ok_or_else(bad)?;
            line.push(LineStep::Card(card));
        } else {
            let index: usize = token.parse().map_err(|_| bad())?;
            let parent = by_path
                .get(prefix)
                .ok_or_else(|| format!("missing parent of {path:?}"))?;
            line.push(LineStep::Action(*parent.get(index).ok_or_else(bad)?));
        }
    }
    Ok(line)
}

fn read_f32s(path: &Path) -> Result<Vec<f32>, String> {
    let mut file = File::open(path).map_err(|e| io_err(path, e))?;
    let len = file.metadata().map_err(|e| io_err(path, e))?.len() as usize;
    if !len.is_multiple_of(4) {
        return Err(io_err(path, "size is not a multiple of 4"));
    }
    let mut data = Vec::with_capacity(len / 4);
    let mut buf = vec![0u8; 4 * CHUNK];
    let mut left = len;
    while left > 0 {
        let n = left.min(buf.len());
        file.read_exact(&mut buf[..n])
            .map_err(|e| io_err(path, e))?;
        data.extend(
            buf[..n]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|&b| f32::from_le_bytes(b)),
        );
        left -= n;
    }
    Ok(data)
}
