//! Assegna id stabili ai blocchi: diff LCS sugli hash tra vecchi e nuovi blocchi.

use std::hash::{DefaultHasher, Hash, Hasher};

use similar::{Algorithm, DiffOp, capture_diff_slices};

use super::document::{Block, Change};

pub fn hash_str(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Sorgente + hash di ogni blocco; i blocchi uguali ripetuti si distinguono con l'occorrenza.
pub fn hash_sources(sources: Vec<String>) -> Vec<(String, u64)> {
    let mut seen: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
    sources
        .into_iter()
        .map(|s| {
            let base = hash_str(&s);
            let n = seen.entry(base).or_insert(0);
            let h = base ^ *n;
            *n += 1;
            (s, h)
        })
        .collect()
}

pub fn assign(old: &[Block], new: Vec<(String, u64)>, next_id: &mut u64) -> (Vec<Block>, Change) {
    let old_h: Vec<u64> = old.iter().map(|b| b.hash).collect();
    let new_h: Vec<u64> = new.iter().map(|b| b.1).collect();
    let ops = capture_diff_slices(Algorithm::Myers, &old_h, &new_h);

    let mut ids: Vec<Option<u64>> = vec![None; new.len()];
    let mut change = Change::default();
    let mut kept = vec![false; old.len()];

    // Unisce Delete+Insert adiacenti in un Replace.
    let mut merged: Vec<DiffOp> = Vec::new();
    for op in ops {
        if let (Some(&DiffOp::Delete { old_index, old_len, new_index }), DiffOp::Insert { new_len, .. }) =
            (merged.last(), &op)
        {
            merged.pop();
            merged.push(DiffOp::Replace { old_index, old_len, new_index, new_len: *new_len });
        } else {
            merged.push(op);
        }
    }

    for op in merged {
        match op {
            DiffOp::Equal { old_index, new_index, len } => {
                for i in 0..len {
                    ids[new_index + i] = Some(old[old_index + i].id);
                    kept[old_index + i] = true;
                }
            }
            DiffOp::Replace { old_index, old_len, new_index, new_len } if old_len == new_len => {
                for i in 0..old_len {
                    let id = old[old_index + i].id;
                    ids[new_index + i] = Some(id);
                    kept[old_index + i] = true;
                    change.changed.push(id);
                }
            }
            _ => {}
        }
    }

    let blocks = new
        .into_iter()
        .zip(ids)
        .map(|((source, hash), id)| {
            let id = id.unwrap_or_else(|| {
                let id = *next_id;
                *next_id += 1;
                change.added.push(id);
                id
            });
            Block { id, source: source.into(), hash }
        })
        .collect();
    change.removed = old.iter().zip(&kept).filter(|(_, k)| !**k).map(|(b, _)| b.id).collect();
    (blocks, change)
}


#[cfg(test)]
mod tests {
    use super::super::document::Document;
    use std::path::{Path, PathBuf};

    fn doc(s: &str) -> Document {
        Document::from_source(Path::new("t.md"), s)
    }

    fn ids(d: &Document) -> Vec<u64> {
        d.blocks.iter().map(|b| b.id).collect()
    }

    #[test]
    fn same_blocks_keep_ids() {
        let src = "# A\n\nuno\n\ndue\n";
        let mut d = doc(src);
        let before = ids(&d);
        let ch = d.update(src);
        assert_eq!(ids(&d), before);
        assert!(ch.added.is_empty() && ch.changed.is_empty() && ch.removed.is_empty());
        assert_eq!(d.revision, 1);
    }

    #[test]
    fn insert_above_keeps_ids_below() {
        let mut d = doc("# A\n\nuno\n\ndue\n");
        let before = ids(&d);
        let ch = d.update("nuovo\n\n# A\n\nuno\n\ndue\n");
        assert_eq!(&ids(&d)[1..], &before[..]);
        assert_eq!(ch.added.len(), 1);
        assert!(ch.changed.is_empty() && ch.removed.is_empty());
    }

    #[test]
    fn edit_in_place_keeps_id() {
        let mut d = doc("# A\n\nuno\n\ndue tre\n\nquattro\n");
        let before = ids(&d);
        let ch = d.update("# A\n\nuno\n\ndue cinque\n\nquattro\n");
        assert_eq!(ids(&d), before);
        assert_eq!(ch.changed, [before[2]]);
        assert!(ch.added.is_empty() && ch.removed.is_empty());
    }

    #[test]
    fn repeated_rules_get_distinct_ids() {
        let src = "---\n\n***\n\n___\n";
        let mut d = doc(src);
        let before = ids(&d);
        assert_eq!(before.len(), 3);
        assert!(before[0] != before[1] && before[1] != before[2] && before[0] != before[2]);
        d.update(src);
        assert_eq!(ids(&d), before);
    }

    #[test]
    fn removal_and_empty_transitions() {
        let mut d = doc("");
        assert!(d.blocks.is_empty());
        let ch = d.update("a\n\nb\n");
        assert_eq!(ch.added.len(), 2);
        let ch = d.update("");
        assert_eq!(ch.removed.len(), 2);
        assert!(d.blocks.is_empty());
    }

    #[test]
    fn non_utf8_is_lossy() {
        let p: PathBuf = std::env::temp_dir().join(format!("runemd-nonutf8-{}.md", std::process::id()));
        std::fs::write(&p, [0xff, b'#', b' ', b'a']).unwrap();
        let d = Document::load(&p).unwrap();
        std::fs::remove_file(&p).ok();
        assert_eq!(d.blocks.len(), 1);
        assert!(d.blocks[0].source.contains('\u{fffd}'));
    }
}
