//! Incremental face clustering by cosine similarity.
//!
//! A face embedding is an L2-normalized vector. Two faces of the same person
//! give a high cosine similarity. Clustering groups faces whose similarity is
//! above a threshold. This module does the pure math. The database holds the
//! cluster ids.

/// The default cosine-similarity threshold for the SFace model. Two faces match
/// as the same person when the cosine similarity is at or above this value.
/// SFace's own recommended threshold is 0.363. That value put too many
/// different people into one group, so the default is stricter.
pub const DEFAULT_COSINE_THRESHOLD: f32 = 0.45;

/// The default member agreement, 0.0..1.0. A face joins a group only when at
/// least this part of the group's sample faces also match it.
pub const DEFAULT_AGREEMENT: f32 = 0.5;

/// The maximum number of sample faces per group for the member check.
const SAMPLE_MAX: usize = 10;

/// The grouping parameters.
#[derive(Debug, Clone, Copy)]
pub struct ClusterParams {
    /// The cosine-similarity threshold, 0.0..1.0. Higher is stricter.
    pub threshold: f32,
    /// The member agreement, 0.0..1.0. Zero turns the member check off.
    pub agreement: f32,
}

impl Default for ClusterParams {
    fn default() -> Self {
        ClusterParams {
            threshold: DEFAULT_COSINE_THRESHOLD,
            agreement: DEFAULT_AGREEMENT,
        }
    }
}

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
/// 2. Each remaining face joins the most similar existing cluster. The face
///    must pass two checks. The similarity to the cluster centroid must be at
///    or above `threshold`. At least `agreement` of the cluster's sample faces
///    must also be at or above `threshold`. The second check stops one face
///    from pulling a different person into a group.
/// 3. A face that matches no cluster starts a new cluster.
/// 4. An unnamed cluster drops each member that is now below `threshold` to
///    the final centroid. A dropped face starts its own cluster.
///
/// Cluster ids for unnamed clusters start at `next_cluster_id` and count up.
/// A person-anchored cluster uses `PERSON_CLUSTER_BASE + person_id` so it never
/// collides with an unnamed cluster id.
pub const PERSON_CLUSTER_BASE: i64 = 1_000_000_000;

pub fn cluster(
    items: &[ClusterItem],
    params: ClusterParams,
    mut next_cluster_id: i64,
) -> Vec<ClusterAssignment> {
    let threshold = params.threshold;
    let agreement = params.agreement.clamp(0.0, 1.0);
    // A centroid holds the sum of its members' embeddings. Cosine similarity
    // ignores scale, so the sum gives the same result as the mean.
    struct Centroid {
        cluster_id: i64,
        sum: Vec<f32>,
        /// Up to SAMPLE_MAX member indexes into `items`, for the member check.
        samples: Vec<usize>,
        /// All member indexes into `items`.
        members: Vec<usize>,
    }
    let mut centroids: Vec<Centroid> = Vec::new();
    // Per item: the index of its centroid.
    let mut owner: Vec<usize> = vec![usize::MAX; items.len()];

    // Seed centroids from person-anchored faces first, so named people pull
    // matching faces in.
    for (i, it) in items.iter().enumerate() {
        if it.person_id != 0 {
            let cid = PERSON_CLUSTER_BASE + it.person_id;
            let idx = match centroids.iter().position(|c| c.cluster_id == cid) {
                Some(idx) => {
                    accumulate(&mut centroids[idx].sum, &it.embedding);
                    idx
                }
                None => {
                    centroids.push(Centroid {
                        cluster_id: cid,
                        sum: it.embedding.clone(),
                        samples: Vec::new(),
                        members: Vec::new(),
                    });
                    centroids.len() - 1
                }
            };
            let c = &mut centroids[idx];
            c.members.push(i);
            if c.samples.len() < SAMPLE_MAX {
                c.samples.push(i);
            }
            owner[i] = idx;
        }
    }

    // Assign the rest.
    for (i, it) in items.iter().enumerate() {
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
            let sim = cosine_similarity(&it.embedding, &c.sum);
            if sim < best_sim {
                continue;
            }
            if !members_agree(it, c.samples.iter().map(|&s| &items[s]), threshold, agreement) {
                continue;
            }
            best_sim = sim;
            best_idx = Some(idx);
        }
        let idx = match best_idx {
            Some(idx) => {
                accumulate(&mut centroids[idx].sum, &it.embedding);
                idx
            }
            None => {
                let cid = next_cluster_id;
                next_cluster_id += 1;
                centroids.push(Centroid {
                    cluster_id: cid,
                    sum: it.embedding.clone(),
                    samples: Vec::new(),
                    members: Vec::new(),
                });
                centroids.len() - 1
            }
        };
        let c = &mut centroids[idx];
        c.members.push(i);
        if c.samples.len() < SAMPLE_MAX {
            c.samples.push(i);
        }
        owner[i] = idx;
    }

    // Final pass: an unnamed cluster drops members that are far from its final
    // centroid. Named clusters stay as they are.
    let mut cluster_of: Vec<i64> = owner
        .iter()
        .map(|&o| if o == usize::MAX { 0 } else { centroids[o].cluster_id })
        .collect();
    for c in &centroids {
        if c.cluster_id >= PERSON_CLUSTER_BASE || c.members.len() < 2 {
            continue;
        }
        for &m in &c.members {
            if cosine_similarity(&items[m].embedding, &c.sum) < threshold {
                cluster_of[m] = next_cluster_id;
                next_cluster_id += 1;
            }
        }
    }

    items
        .iter()
        .zip(cluster_of)
        .map(|(it, cid)| ClusterAssignment {
            face_id: it.face_id,
            cluster_id: cid,
        })
        .collect()
}

/// Report whether enough sample faces of a cluster match `it`. A cluster with
/// no samples, or an agreement of zero, always agrees.
fn members_agree<'a>(
    it: &ClusterItem,
    samples: impl Iterator<Item = &'a ClusterItem>,
    threshold: f32,
    agreement: f32,
) -> bool {
    if agreement <= 0.0 {
        return true;
    }
    let mut n = 0usize;
    let mut ok = 0usize;
    for s in samples {
        n += 1;
        if cosine_similarity(&it.embedding, &s.embedding) >= threshold {
            ok += 1;
        }
    }
    if n == 0 {
        return true;
    }
    (ok as f32) >= agreement * n as f32 - 1e-6
}

fn accumulate(sum: &mut [f32], v: &[f32]) {
    for i in 0..sum.len().min(v.len()) {
        sum[i] += v[i];
    }
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
        let asg = cluster(&items, ClusterParams { threshold: 0.5, agreement: 0.5 }, 1);
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
        let asg = cluster(&items, ClusterParams { threshold: 0.5, agreement: 0.5 }, 1);
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
        let asg = cluster(&items, ClusterParams { threshold: 0.5, agreement: 0.5 }, 1);
        let cid = |fid: i64| asg.iter().find(|a| a.face_id == fid).unwrap().cluster_id;
        assert_eq!(cid(1), PERSON_CLUSTER_BASE + 7);
        assert_ne!(cid(2), PERSON_CLUSTER_BASE + 7);
    }

    fn unnamed(face_id: i64, embedding: Vec<f32>) -> ClusterItem {
        ClusterItem { face_id, embedding, cluster_id: 0, person_id: 0, rejected: vec![] }
    }

    fn at_deg(d: f32) -> Vec<f32> {
        let r = d.to_radians();
        vec![r.cos(), r.sin()]
    }

    #[test]
    fn member_check_blocks_a_centroid_only_match() {
        // Person 7 has two faces far apart. The new face is near their
        // centroid but far from each member.
        let mk = || {
            vec![
                ClusterItem { face_id: 1, embedding: vec![1.0, 1.0, 0.0], cluster_id: 0, person_id: 7, rejected: vec![] },
                ClusterItem { face_id: 2, embedding: vec![1.0, -1.0, 0.0], cluster_id: 0, person_id: 7, rejected: vec![] },
                unnamed(3, vec![1.0, 0.0, 0.6]),
            ]
        };
        let items = mk();
        let off = cluster(&items, ClusterParams { threshold: 0.7, agreement: 0.0 }, 1);
        let on = cluster(&items, ClusterParams { threshold: 0.7, agreement: 0.5 }, 1);
        let cid = |a: &[ClusterAssignment], fid: i64| a.iter().find(|x| x.face_id == fid).unwrap().cluster_id;
        assert_eq!(cid(&off, 3), PERSON_CLUSTER_BASE + 7);
        assert_ne!(cid(&on, 3), PERSON_CLUSTER_BASE + 7);
    }

    #[test]
    fn drifted_member_leaves_unnamed_cluster() {
        // The group drifts from 0 deg toward 65 deg. The first face ends far
        // from the final centroid and must leave the group.
        let items: Vec<ClusterItem> = [0.0, 35.0, 50.0, 60.0, 65.0]
            .iter()
            .enumerate()
            .map(|(i, d)| unnamed(i as i64 + 1, at_deg(*d)))
            .collect();
        let asg = cluster(&items, ClusterParams { threshold: 0.8, agreement: 0.5 }, 1);
        let cid = |fid: i64| asg.iter().find(|a| a.face_id == fid).unwrap().cluster_id;
        assert_eq!(cid(2), cid(5));
        assert_eq!(cid(3), cid(5));
        assert_ne!(cid(1), cid(5));
    }
}
