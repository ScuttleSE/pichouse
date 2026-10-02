//! Stylised face clustering with HDBSCAN.
//!
//! A stylised face embedding is an L2-normalised 768-value CCIP vector. HDBSCAN
//! groups the embeddings. HDBSCAN makes many small groups and marks unclear
//! faces as noise. The design rule is: make many small groups. Two clicks merge
//! two groups. Two characters in one group is worse.
//!
//! Named characters anchor stable clusters, like the human face system. A face
//! already assigned to a character keeps that character's cluster id. An unnamed
//! face that is very near a character joins it. HDBSCAN groups the rest.

#![allow(dead_code)]

use hdbscan::{Center, DistanceMetric, Hdbscan, HdbscanHyperParams};

/// The default HDBSCAN cluster-selection epsilon. Zero uses pure HDBSCAN
/// selection, which makes the smallest groups.
pub const DEFAULT_EPSILON: f32 = 0.0;

/// The minimum cluster size. Two is the smallest useful group.
const MIN_CLUSTER_SIZE: usize = 2;

/// The default cosine-distance limit, 0..2. A smaller value is stricter. The
/// limit applies in two places. An unnamed face joins a named character only
/// when it is within the limit of the character centroid. A face stays in an
/// HDBSCAN group only when it is within the limit of the group centroid. The
/// old fixed value was 0.20. This value may need adjustment after real tests.
pub const DEFAULT_MAX_DIST: f32 = 0.15;

/// The default HDBSCAN `min_samples`. A value of 1 makes HDBSCAN act as
/// single linkage. Single linkage chains similar faces, and a chain can link
/// two characters into one group. A larger value stops the chain.
pub const DEFAULT_MIN_SAMPLES: usize = 3;

/// The grouping parameters.
#[derive(Debug, Clone, Copy)]
pub struct ClusterParams {
    /// The HDBSCAN cluster-selection epsilon. Larger makes fewer, larger groups.
    pub epsilon: f32,
    /// The cosine-distance limit. See `DEFAULT_MAX_DIST`.
    pub max_dist: f32,
    /// The HDBSCAN `min_samples`, 1 or more.
    pub min_samples: usize,
}

impl Default for ClusterParams {
    fn default() -> Self {
        ClusterParams {
            epsilon: DEFAULT_EPSILON,
            max_dist: DEFAULT_MAX_DIST,
            min_samples: DEFAULT_MIN_SAMPLES,
        }
    }
}

/// The base offset for a character-anchored cluster id. A named character owns a
/// stable cluster id equal to this base plus the character id.
pub const CHARACTER_CLUSTER_BASE: i64 = 1_000_000_000;

/// The cluster id for noise (unclear or unmatched faces). HDBSCAN uses -1.
pub const NOISE_CLUSTER_ID: i64 = -1;

/// Euclidean distance of two equal-length vectors. For L2-normalised vectors
/// this relates to cosine distance by d^2 = 2 - 2*cos.
fn euclidean(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let mut s = 0f32;
    for i in 0..n {
        let d = a[i] - b[i];
        s += d * d;
    }
    s.sqrt()
}

/// Cosine distance of two vectors, 0..2. The vectors need not have unit
/// length, so a centroid (a mean or a sum) gives a correct value.
fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let mut dot = 0f32;
    let mut na = 0f32;
    let mut nb = 0f32;
    for i in 0..n {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 1.0;
    }
    1.0 - dot / (na.sqrt() * nb.sqrt())
}

/// One face for clustering: its id and its embedding.
pub struct ClusterItem {
    pub face_id: i64,
    pub embedding: Vec<f32>,
    /// The assigned character, or 0. A character anchors its cluster.
    pub character_id: i64,
    /// Character ids this face was rejected from. Clustering never attaches the
    /// face to a rejected character's cluster.
    pub rejected: Vec<i64>,
}

/// The result of a clustering pass: face id -> new cluster id.
pub struct ClusterAssignment {
    pub face_id: i64,
    pub cluster_id: i64,
}

/// Assign every face to a cluster.
///
/// Steps:
/// 1. A face assigned to a character keeps that character's stable cluster id.
/// 2. An unnamed face very near a character centroid joins it, unless rejected.
/// 3. HDBSCAN groups the remaining unnamed faces. Its labels start at
///    `next_cluster_id`. HDBSCAN noise stays as `NOISE_CLUSTER_ID` (-1).
/// 4. Each HDBSCAN group drops the faces that are farther than `max_dist` from
///    the group centroid. A dropped face becomes noise. A group with fewer
///    than `MIN_CLUSTER_SIZE` faces left becomes noise.
pub fn cluster(
    items: &[ClusterItem],
    params: ClusterParams,
    next_cluster_id: i64,
) -> Vec<ClusterAssignment> {
    let epsilon = params.epsilon;
    let max_dist = params.max_dist;
    let mut out: Vec<ClusterAssignment> = Vec::with_capacity(items.len());

    // Step 1: character centroids from anchored faces.
    struct Centroid {
        character_id: i64,
        sum: Vec<f32>,
        count: usize,
    }
    let mut centroids: Vec<Centroid> = Vec::new();
    for it in items {
        if it.character_id != 0 {
            let cid = CHARACTER_CLUSTER_BASE + it.character_id;
            if let Some(c) = centroids
                .iter_mut()
                .find(|c| c.character_id == it.character_id)
            {
                for i in 0..c.sum.len().min(it.embedding.len()) {
                    c.sum[i] += it.embedding[i];
                }
                c.count += 1;
            } else {
                centroids.push(Centroid {
                    character_id: it.character_id,
                    sum: it.embedding.clone(),
                    count: 1,
                });
            }
            out.push(ClusterAssignment {
                face_id: it.face_id,
                cluster_id: cid,
            });
        }
    }

    // Step 2: unnamed faces that are very near a character join it. Collect the
    // rest for HDBSCAN.
    let mean = |c: &Centroid| -> Vec<f32> {
        if c.count == 0 {
            c.sum.clone()
        } else {
            c.sum.iter().map(|x| x / c.count as f32).collect()
        }
    };
    let mut rest: Vec<&ClusterItem> = Vec::new();
    for it in items {
        if it.character_id != 0 {
            continue;
        }
        let mut best: Option<i64> = None;
        let mut best_dist = max_dist;
        for c in &centroids {
            if it.rejected.contains(&c.character_id) {
                continue;
            }
            let d = cosine_distance(&it.embedding, &mean(c));
            if d <= best_dist {
                best_dist = d;
                best = Some(c.character_id);
            }
        }
        match best {
            Some(chid) => out.push(ClusterAssignment {
                face_id: it.face_id,
                cluster_id: CHARACTER_CLUSTER_BASE + chid,
            }),
            None => rest.push(it),
        }
    }

    // Step 3: HDBSCAN over the rest. It needs at least MIN_CLUSTER_SIZE points.
    if rest.len() >= MIN_CLUSTER_SIZE {
        let data: Vec<Vec<f32>> = rest.iter().map(|it| it.embedding.clone()).collect();
        // HDBSCAN panics when min_samples is larger than the point count.
        let min_samples = params.min_samples.max(1).min(rest.len());
        let hp = HdbscanHyperParams::builder()
            .min_cluster_size(MIN_CLUSTER_SIZE)
            .min_samples(min_samples)
            .epsilon(epsilon as f64)
            .dist_metric(DistanceMetric::Euclidean)
            .build();
        let model = Hdbscan::new(&data, hp);
        match model.cluster() {
            Ok(labels) => {
                let labels = drop_outliers(&data, &labels, max_dist);
                for (i, it) in rest.iter().enumerate() {
                    let lbl = labels[i];
                    let cid = if lbl < 0 {
                        NOISE_CLUSTER_ID
                    } else {
                        next_cluster_id + lbl as i64
                    };
                    out.push(ClusterAssignment {
                        face_id: it.face_id,
                        cluster_id: cid,
                    });
                }
            }
            Err(e) => {
                log::warn!("hdbscan: {e}; marking rest as noise");
                for it in &rest {
                    out.push(ClusterAssignment {
                        face_id: it.face_id,
                        cluster_id: NOISE_CLUSTER_ID,
                    });
                }
            }
        }
    } else {
        // Too few to cluster. Mark as noise.
        for it in &rest {
            out.push(ClusterAssignment {
                face_id: it.face_id,
                cluster_id: NOISE_CLUSTER_ID,
            });
        }
    }

    out
}

/// Set the label of each far-off face to noise (-1). A face is far off when its
/// cosine distance to its group centroid is above `max_dist`. A group with
/// fewer than `MIN_CLUSTER_SIZE` faces left becomes noise.
fn drop_outliers(data: &[Vec<f32>], labels: &[i32], max_dist: f32) -> Vec<i32> {
    use std::collections::HashMap;
    let mut out = labels.to_vec();
    let mut groups: HashMap<i32, Vec<usize>> = HashMap::new();
    for (i, &l) in labels.iter().enumerate() {
        if l >= 0 {
            groups.entry(l).or_default().push(i);
        }
    }
    for members in groups.values() {
        let dim = data[members[0]].len();
        let mut sum = vec![0f32; dim];
        for &m in members {
            for k in 0..dim.min(data[m].len()) {
                sum[k] += data[m][k];
            }
        }
        let mut kept = Vec::new();
        for &m in members {
            if cosine_distance(&data[m], &sum) > max_dist {
                out[m] = -1;
            } else {
                kept.push(m);
            }
        }
        if kept.len() < MIN_CLUSTER_SIZE {
            for m in kept {
                out[m] = -1;
            }
        }
    }
    out
}

/// Compute the centroid of a set of embeddings. Used to pick a group's cover
/// face (the face nearest the centroid).
pub fn centroid(embeddings: &[Vec<f32>]) -> Vec<f32> {
    if embeddings.is_empty() {
        return Vec::new();
    }
    let dim = embeddings[0].len();
    let mut sum = vec![0f32; dim];
    for e in embeddings {
        for i in 0..dim.min(e.len()) {
            sum[i] += e[i];
        }
    }
    for v in sum.iter_mut() {
        *v /= embeddings.len() as f32;
    }
    sum
}

/// Distance from an embedding to a centroid, for cover-face ranking.
pub fn dist_to_centroid(embedding: &[f32], centroid: &[f32]) -> f32 {
    euclidean(embedding, centroid)
}

/// Silence unused-import lints when Center is not referenced elsewhere.
#[allow(dead_code)]
fn _center_ref() -> Center {
    Center::Centroid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_tight_groups_form_two_clusters() {
        let mut items = Vec::new();
        // Group A near a 384-d vector e0, group B near e1. Use small dims here.
        let a = vec![1.0, 0.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0, 0.0];
        for (i, base) in [a.clone(), a.clone(), a.clone(), b.clone(), b.clone(), b.clone()]
            .into_iter()
            .enumerate()
        {
            items.push(ClusterItem {
                face_id: i as i64 + 1,
                embedding: base,
                character_id: 0,
                rejected: vec![],
            });
        }
        let asg = cluster(&items, ClusterParams::default(), 1);
        let cid = |fid: i64| asg.iter().find(|a| a.face_id == fid).unwrap().cluster_id;
        // The two groups must differ (or be noise, but not merged wrongly).
        assert_ne!(cid(1), cid(4));
    }

    #[test]
    fn character_anchors_stable_cluster() {
        let items = vec![
            ClusterItem {
                face_id: 1,
                embedding: vec![1.0, 0.0, 0.0],
                character_id: 7,
                rejected: vec![],
            },
            ClusterItem {
                face_id: 2,
                embedding: vec![0.99, 0.01, 0.0],
                character_id: 0,
                rejected: vec![],
            },
        ];
        let asg = cluster(&items, ClusterParams::default(), 1);
        let cid = |fid: i64| asg.iter().find(|a| a.face_id == fid).unwrap().cluster_id;
        assert_eq!(cid(1), CHARACTER_CLUSTER_BASE + 7);
        assert_eq!(cid(2), CHARACTER_CLUSTER_BASE + 7);
    }

    #[test]
    fn rejected_face_does_not_join_character() {
        let items = vec![
            ClusterItem {
                face_id: 1,
                embedding: vec![1.0, 0.0, 0.0],
                character_id: 7,
                rejected: vec![],
            },
            ClusterItem {
                face_id: 2,
                embedding: vec![0.99, 0.01, 0.0],
                character_id: 0,
                rejected: vec![7],
            },
        ];
        let asg = cluster(&items, ClusterParams::default(), 1);
        let cid = |fid: i64| asg.iter().find(|a| a.face_id == fid).unwrap().cluster_id;
        assert_ne!(cid(2), CHARACTER_CLUSTER_BASE + 7);
    }

    fn unnamed(face_id: i64, embedding: Vec<f32>) -> ClusterItem {
        ClusterItem { face_id, embedding, character_id: 0, rejected: vec![] }
    }

    fn unit(v: Vec<f32>) -> Vec<f32> {
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.into_iter().map(|x| x / n).collect()
    }

    #[test]
    fn chain_does_not_link_two_groups() {
        // Two tight groups and one bridge face between them. With
        // min_samples = 1 the bridge can chain the groups. With the default
        // the two groups must stay apart.
        let mut items = Vec::new();
        let mut id = 1;
        for k in 0..4 {
            items.push(unnamed(id, unit(vec![1.0, 0.01 * k as f32, 0.0])));
            id += 1;
        }
        items.push(unnamed(id, unit(vec![1.0, 1.0, 0.0])));
        id += 1;
        for k in 0..4 {
            items.push(unnamed(id, unit(vec![0.01 * k as f32, 1.0, 0.0])));
            id += 1;
        }
        let asg = cluster(&items, ClusterParams::default(), 1);
        let cid = |fid: i64| asg.iter().find(|a| a.face_id == fid).unwrap().cluster_id;
        assert_ne!(cid(1), NOISE_CLUSTER_ID);
        assert_ne!(cid(6), NOISE_CLUSTER_ID);
        assert_ne!(cid(1), cid(6));
        // The bridge face is far from both centroids, so it is noise or alone.
        assert_ne!(cid(5), cid(1));
        assert_ne!(cid(5), cid(6));
    }

    #[test]
    fn far_member_becomes_noise() {
        let data = vec![
            unit(vec![1.0, 0.0]),
            unit(vec![1.0, 0.05]),
            unit(vec![1.0, -0.05]),
            unit(vec![0.3, 1.0]),
        ];
        let labels = vec![0, 0, 0, 0];
        let out = drop_outliers(&data, &labels, 0.15);
        assert_eq!(out, vec![0, 0, 0, -1]);
    }

    #[test]
    fn small_remainder_becomes_noise() {
        let data = vec![unit(vec![1.0, 0.0]), unit(vec![0.0, 1.0])];
        let out = drop_outliers(&data, &[0, 0], 0.15);
        assert_eq!(out, vec![-1, -1]);
    }

    #[test]
    fn min_samples_above_count_does_not_panic() {
        let items = vec![unnamed(1, vec![1.0, 0.0]), unnamed(2, vec![1.0, 0.01])];
        let p = ClusterParams { min_samples: 10, ..ClusterParams::default() };
        let _ = cluster(&items, p, 1);
    }
}
