use anyhow::Result;
/// Auto-clustering for memories based on vector similarity.
/// Uses k-means-like approach for grouping similar memories.

/// Cluster memories using a simple k-means-like approach.
pub fn cluster_memories(
    embeddings: &[(String, Vec<f32>)],
    num_clusters: usize,
) -> Result<Vec<(String, usize)>> {
    if embeddings.is_empty() || num_clusters == 0 {
        return Ok(Vec::new());
    }
    let dim = embeddings[0].1.len();
    let n = embeddings.len();
    let mut centroids: Vec<Vec<f32>> = Vec::new();
    for i in 0..num_clusters.min(n) {
        centroids.push(embeddings[i].1.clone());
    }
    if n < num_clusters {
        return Ok(embeddings.iter().enumerate().map(|(i, (id, _))| (id.clone(), i)).collect());
    }
    let mut assignments: Vec<usize> = vec![0; n];
    for _ in 0..10 {
        for (i, (_, emb)) in embeddings.iter().enumerate() {
            let mut min_dist = f32::MAX;
            let mut min_idx = 0;
            for (j, centroid) in centroids.iter().enumerate() {
                let dist = cosine_distance(emb, centroid);
                if dist < min_dist {
                    min_dist = dist;
                    min_idx = j;
                }
            }
            assignments[i] = min_idx;
        }
        for j in 0..centroids.len() {
            let mut sum = vec![0.0f32; dim];
            let mut count = 0;
            for (i, (_, emb)) in embeddings.iter().enumerate() {
                if assignments[i] == j {
                    for k in 0..dim {
                        sum[k] += emb[k];
                    }
                    count += 1;
                }
            }
            if count > 0 {
                for k in 0..dim {
                    centroids[j][k] = sum[k] / count as f32;
                }
            }
        }
    }
    Ok(embeddings.iter().enumerate().map(|(i, (id, _))| (id.clone(), assignments[i])).collect())
}

fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return f32::MAX;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 1.0;
    }
    1.0 - (dot / (norm_a * norm_b))
}