//! Incremental grouping of new stylised faces.
//!
//! A full HDBSCAN pass over a large library takes too long. For example,
//! 100,000 faces can take hours. So a scan does not run HDBSCAN. A scan puts
//! each new face into the nearest existing group instead:
//!
//! 1. `CentroidIndex::load` reads every grouped face one time and keeps one
//!    sum vector per character and per unnamed group. The memory is about
//!    3 KB per group.
//! 2. `assign` compares each new face with each centroid. The face joins the
//!    nearest group within `max_dist`. Otherwise the face becomes noise (-1).
//!
//! New groups come only from HDBSCAN over the noise faces. See
//! `group_noise`.

use std::collections::HashMap;

use super::cluster::{self, ClusterItem, ClusterParams, CHARACTER_CLUSTER_BASE, NOISE_CLUSTER_ID};
use crate::db::Library;

/// The maximum number of noise faces for one HDBSCAN pass. HDBSCAN time grows
/// with the square of the count. 5,000 faces take about 30 s.
pub const NOISE_GROUP_MAX: i64 = 5000;

/// A group key: a character or an unnamed cluster.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Key {
    Character(i64),
    Cluster(i64),
}

struct Centroid {
    key: Key,
    sum: Vec<f32>,
    norm: f32,
}

/// One sum vector per group.
pub struct CentroidIndex {
    centroids: Vec<Centroid>,
    by_key: HashMap<Key, usize>,
}

impl CentroidIndex {
    /// Read every grouped face and build the sums. Returns `None` when `stop`
    /// ends the read.
    pub fn load(lib: &Library, stop: &dyn Fn() -> bool) -> Result<Option<Self>, String> {
        let mut idx = CentroidIndex { centroids: Vec::new(), by_key: HashMap::new() };
        let complete = lib
            .for_each_grouped_style_face(stop, &mut |character, cluster, emb| {
                let key = if character != 0 {
                    Key::Character(character)
                } else {
                    Key::Cluster(cluster)
                };
                idx.add(key, emb, false);
            })
            .map_err(|e| format!("read grouped faces: {e}"))?;
        if !complete {
            return Ok(None);
        }
        for c in &mut idx.centroids {
            c.norm = length(&c.sum);
        }
        Ok(Some(idx))
    }

    /// The number of groups.
    pub fn len(&self) -> usize {
        self.centroids.len()
    }

    fn add(&mut self, key: Key, emb: &[f32], update_norm: bool) {
        let i = match self.by_key.get(&key) {
            Some(&i) => {
                let s = &mut self.centroids[i].sum;
                for (a, b) in s.iter_mut().zip(emb) {
                    *a += *b;
                }
                i
            }
            None => {
                self.centroids.push(Centroid { key, sum: emb.to_vec(), norm: 0.0 });
                self.by_key.insert(key, self.centroids.len() - 1);
                self.centroids.len() - 1
            }
        };
        if update_norm {
            let c = &mut self.centroids[i];
            c.norm = length(&c.sum);
        }
    }

    /// Give each face a cluster id: the nearest group within `max_dist`, or
    /// noise. A face never joins a character that rejected it. Each joined
    /// face updates its group sum. Returns `(face_id, cluster_id)` pairs.
    pub fn assign(
        &mut self,
        faces: &[(i64, Vec<f32>)],
        rejections: &HashMap<i64, Vec<i64>>,
        max_dist: f32,
    ) -> Vec<(i64, i64)> {
        let mut out = Vec::with_capacity(faces.len());
        for (face_id, emb) in faces {
            let en = length(emb);
            let rejected = rejections.get(face_id);
            let mut best: Option<(usize, f32)> = None;
            if en > 0.0 {
                for (i, c) in self.centroids.iter().enumerate() {
                    if c.norm == 0.0 || c.sum.len() != emb.len() {
                        continue;
                    }
                    let d = 1.0 - dot(&c.sum, emb) / (c.norm * en);
                    if d > max_dist || best.is_some_and(|(_, bd)| d >= bd) {
                        continue;
                    }
                    if let (Key::Character(ch), Some(r)) = (c.key, rejected) {
                        if r.contains(&ch) {
                            continue;
                        }
                    }
                    best = Some((i, d));
                }
            }
            let cid = match best {
                Some((i, _)) => {
                    let key = self.centroids[i].key;
                    self.add(key, emb, true);
                    match key {
                        Key::Character(ch) => CHARACTER_CLUSTER_BASE + ch,
                        Key::Cluster(cl) => cl,
                    }
                }
                None => NOISE_CLUSTER_ID,
            };
            out.push((*face_id, cid));
        }
        out
    }
}

/// Group the faces that a scan added and that have no group yet. Writes the
/// result. Returns the number of faces it grouped.
pub fn assign_new(lib: &Library, idx: &mut CentroidIndex, params: ClusterParams) -> Result<usize, String> {
    let faces = lib.ungrouped_style_faces().map_err(|e| format!("read new faces: {e}"))?;
    if faces.is_empty() {
        return Ok(0);
    }
    let rejections = lib.style_face_rejection_map().unwrap_or_default();
    let pairs = idx.assign(&faces, &rejections, params.max_dist);
    lib.set_style_face_clusters(&pairs).map_err(|e| format!("write clusters: {e}"))?;
    Ok(pairs.len())
}

/// Run HDBSCAN over the newest `NOISE_GROUP_MAX` noise faces. New groups get
/// ids above the largest existing unnamed id. Existing groups do not change.
/// Faces that stay noise keep -1. Returns the number of faces it read.
pub fn group_noise(lib: &Library, params: ClusterParams) -> Result<usize, String> {
    let faces = lib.noise_style_faces(NOISE_GROUP_MAX).map_err(|e| format!("read noise: {e}"))?;
    if faces.is_empty() {
        return Ok(0);
    }
    let next = lib
        .max_unnamed_style_cluster_id(CHARACTER_CLUSTER_BASE)
        .map_err(|e| format!("read max cluster: {e}"))?
        + 1;
    let items: Vec<ClusterItem> = faces
        .into_iter()
        .map(|(face_id, embedding)| ClusterItem { face_id, embedding, character_id: 0, rejected: Vec::new() })
        .collect();
    let n = items.len();
    let pairs: Vec<(i64, i64)> = cluster::cluster(&items, params, next)
        .into_iter()
        .map(|a| (a.face_id, a.cluster_id))
        .collect();
    lib.set_style_face_clusters(&pairs).map_err(|e| format!("write clusters: {e}"))?;
    Ok(n)
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn length(v: &[f32]) -> f32 {
    dot(v, v).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(groups: &[(Key, Vec<f32>)]) -> CentroidIndex {
        let mut idx = CentroidIndex { centroids: Vec::new(), by_key: HashMap::new() };
        for (k, v) in groups {
            idx.add(*k, v, true);
        }
        idx
    }

    #[test]
    fn joins_nearest_group_or_becomes_noise() {
        let mut idx = index(&[
            (Key::Character(7), vec![1.0, 0.0]),
            (Key::Cluster(3), vec![0.0, 1.0]),
        ]);
        let faces = vec![(1, vec![0.99, 0.05]), (2, vec![0.05, 0.99]), (3, vec![-1.0, 0.0])];
        let out = idx.assign(&faces, &HashMap::new(), 0.15);
        assert_eq!(out, vec![(1, CHARACTER_CLUSTER_BASE + 7), (2, 3), (3, NOISE_CLUSTER_ID)]);
    }

    #[test]
    fn rejected_character_is_skipped() {
        let mut idx = index(&[(Key::Character(7), vec![1.0, 0.0])]);
        let rej = HashMap::from([(1, vec![7])]);
        let out = idx.assign(&[(1, vec![1.0, 0.0])], &rej, 0.15);
        assert_eq!(out, vec![(1, NOISE_CLUSTER_ID)]);
    }
}

