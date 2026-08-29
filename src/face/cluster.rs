//! Incremental face clustering by cosine similarity.
//!
//! A face embedding is an L2-normalized vector. Two faces of the same person
//! give a high cosine similarity. Clustering groups faces whose similarity is
//! above a threshold. This module does the pure math. The database holds the
//! cluster ids.

/// The default cosine-similarity threshold for the SFace model. Two faces match
/// as the same person when the cosine similarity is at or above this value.
/// SFace's own recommended threshold is 0.363.
pub const DEFAULT_COSINE_THRESHOLD: f32 = 0.363;

/// Cosine similarity of two equal-length vectors. Returns 0.0 for a length
/// mismatch or an empty vector.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// One face for clustering: its id and its embedding.
pub struct ClusterItem {
    pub face_id: i64,
    pub embedding: Vec<f32>,
    /// The current cluster id, or 0 when not yet clustered.
    pub cluster_id: i64,
    /// The assigned person, or 0. A person anchors its cluster.
    pub person_id: i64,
    /// Person ids this face was rejected from. Clustering never attaches the
    /// face to a rejected person's cluster.
    pub rejected: Vec<i64>,
}

/// The result of a clustering pass: face id -> new cluster id.
pub struct ClusterAssignment {
    pub face_id: i64,
    pub cluster_id: i64,
}

/// Assign every face to a cluster with a greedy nearest-centroid method.
///
/// The method is incremental and stable:
/// 1. A face already assigned to a person keeps that person's cluster. The
///    cluster id equals the person id offset, so a person owns a stable cluster.
/// 2. Each remaining face joins the existing cluster whose centroid is nearest,
///    if the similarity is at or above `threshold`.
/// 3. A face that matches no cluster starts a new cluster.
///
/// Cluster ids for unnamed clusters start at `next_cluster_id` and count up.
/// A person-anchored cluster uses `PERSON_CLUSTER_BASE + person_id` so it never
/// collides with an unnamed cluster id.
pub const PERSON_CLUSTER_BASE: i64 = 1_000_000_000;

pub fn cluster(
    items: &[ClusterItem],
    threshold: f32,
    mut next_cluster_id: i64,
) -> Vec<ClusterAssignment> {
    // A centroid is the running mean of its members' embeddings.
    struct Centroid {
        cluster_id: i64,
        sum: Vec<f32>,
        count: usize,
    }
    let mut centroids: Vec<Centroid> = Vec::new();
    let mut out: Vec<ClusterAssignment> = Vec::with_capacity(items.len());

    // Seed centroids from person-anchored faces first, so named people pull
    // matching faces in.
    for it in items {
        if it.person_id != 0 {
            let cid = PERSON_CLUSTER_BASE + it.person_id;
            if let Some(c) = centroids.iter_mut().find(|c| c.cluster_id == cid) {
                accumulate(&mut c.sum, &it.embedding);
                c.count += 1;
            } else {
                centroids.push(Centroid {
                    cluster_id: cid,
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

    // Assign the rest.
    for it in items {
        if it.person_id != 0 {
            continue;
        }
        let mut best_idx: Option<usize> = None;
        let mut best_sim = threshold;
        for (idx, c) in centroids.iter().enumerate() {
            // Skip a person's cluster this face was rejected from.
            if c.cluster_id >= PERSON_CLUSTER_BASE {
                let pid = c.cluster_id - PERSON_CLUSTER_BASE;
                if it.rejected.contains(&pid) {
                    continue;
                }
            }
            let centroid = mean(&c.sum, c.count);
            let sim = cosine_similarity(&it.embedding, &centroid);
            if sim >= best_sim {
                best_sim = sim;
                best_idx = Some(idx);
            }
        }
        let cid = match best_idx {
            Some(idx) => {
                accumulate(&mut centroids[idx].sum, &it.embedding);
                centroids[idx].count += 1;
                centroids[idx].cluster_id
            }
            None => {
                let cid = next_cluster_id;
                next_cluster_id += 1;
                centroids.push(Centroid {
                    cluster_id: cid,
                    sum: it.embedding.clone(),
                    count: 1,
                });
                cid
            }
        };
        out.push(ClusterAssignment {
            face_id: it.face_id,
            cluster_id: cid,
        });
    }
    out
}

fn accumulate(sum: &mut [f32], v: &[f32]) {
    for i in 0..sum.len().min(v.len()) {
        sum[i] += v[i];
    }
}

fn mean(sum: &[f32], count: usize) -> Vec<f32> {
    if count == 0 {
        return sum.to_vec();
    }
    sum.iter().map(|x| x / count as f32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_of_identical_is_one() {
        let a = vec![0.6, 0.8];
        assert!((cosine_similarity(&a, &a) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_of_orthogonal_is_zero() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn length_mismatch_is_zero() {
        assert_eq!(cosine_similarity(&[1.0], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn two_tight_groups_form_two_clusters() {
        // Group one near (1,0), group two near (0,1).
        let items = vec![
            ClusterItem { face_id: 1, embedding: vec![1.0, 0.02], cluster_id: 0, person_id: 0, rejected: vec![] },
            ClusterItem { face_id: 2, embedding: vec![0.98, 0.0], cluster_id: 0, person_id: 0, rejected: vec![] },
            ClusterItem { face_id: 3, embedding: vec![0.0, 1.0], cluster_id: 0, person_id: 0, rejected: vec![] },
            ClusterItem { face_id: 4, embedding: vec![0.03, 0.99], cluster_id: 0, person_id: 0, rejected: vec![] },
        ];
        let asg = cluster(&items, 0.5, 1);
        let cid = |fid: i64| asg.iter().find(|a| a.face_id == fid).unwrap().cluster_id;
        assert_eq!(cid(1), cid(2));
        assert_eq!(cid(3), cid(4));
        assert_ne!(cid(1), cid(3));
    }

    #[test]
    fn named_person_anchors_a_stable_cluster() {
        // Face 1 belongs to person 7. Face 2 is similar and unnamed. It must
        // join person 7's cluster.
        let items = vec![
            ClusterItem { face_id: 1, embedding: vec![1.0, 0.0], cluster_id: 0, person_id: 7, rejected: vec![] },
            ClusterItem { face_id: 2, embedding: vec![0.99, 0.01], cluster_id: 0, person_id: 0, rejected: vec![] },
        ];
        let asg = cluster(&items, 0.5, 1);
        let cid = |fid: i64| asg.iter().find(|a| a.face_id == fid).unwrap().cluster_id;
        assert_eq!(cid(1), PERSON_CLUSTER_BASE + 7);
        assert_eq!(cid(2), PERSON_CLUSTER_BASE + 7);
    }

    #[test]
    fn rejected_face_does_not_rejoin_person() {
        // Face 2 is similar to person 7 but was rejected from person 7. It must
        // NOT join person 7's cluster; it starts its own instead.
        let items = vec![
            ClusterItem { face_id: 1, embedding: vec![1.0, 0.0], cluster_id: 0, person_id: 7, rejected: vec![] },
            ClusterItem { face_id: 2, embedding: vec![0.99, 0.01], cluster_id: 0, person_id: 0, rejected: vec![7] },
        ];
        let asg = cluster(&items, 0.5, 1);
        let cid = |fid: i64| asg.iter().find(|a| a.face_id == fid).unwrap().cluster_id;
        assert_eq!(cid(1), PERSON_CLUSTER_BASE + 7);
        assert_ne!(cid(2), PERSON_CLUSTER_BASE + 7);
    }
}
