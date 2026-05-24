use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone)]
pub struct DagTask {
    pub id: String,
    pub role: String,
    pub title: String,
    pub description: String,
    pub depends_on: Vec<String>,
    pub priority: String,
    pub files: Vec<String>,
}

/// Execution plan: waves of tasks that can run in parallel.
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub waves: Vec<Vec<usize>>,
    pub max_concurrent: usize,
}

/// Topological sort into parallel waves with priority ordering and concurrency limits.
pub fn compute_waves(tasks: &[DagTask]) -> ExecutionPlan {
    let n = tasks.len();
    let idx: HashMap<&str, usize> = tasks.iter().enumerate()
        .map(|(i, t)| (t.title.as_str(), i)).collect();
    let mut graph: Vec<Vec<usize>> = vec![vec![]; n];
    let mut in_degree = vec![0usize; n];

    // Build dependency graph: dep name → index
    for (i, task) in tasks.iter().enumerate() {
        for dep in &task.depends_on {
            // Match by title (exact) or by start of title (prefix, for robustness)
            let matched = idx.get(dep.as_str()).copied()
                .or_else(|| tasks.iter().position(|t| t.title.contains(dep.as_str())));
            if let Some(di) = matched {
                if di != i {
                    graph[di].push(i);
                    in_degree[i] += 1;
                }
            }
        }
    }

    let mut waves = vec![];
    let mut q: VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
    sort_deque_by_priority(&mut q, tasks);

    while !q.is_empty() {
        let mut wave: Vec<usize> = q.drain(..).collect();
        // Within wave: high priority first
        wave.sort_by_key(|&i| priority_val(&tasks[i].priority));
        for &node in &wave {
            for &next in &graph[node] {
                in_degree[next] -= 1;
                if in_degree[next] == 0 { q.push_back(next); }
            }
        }
        waves.push(wave);
    }

    // Circular deps → final wave
    let rem: Vec<usize> = (0..n).filter(|i| !waves.iter().any(|w| w.contains(i))).collect();
    if !rem.is_empty() {
        tracing::warn!("circular deps: {} tasks in final wave", rem.len());
        waves.push(rem);
    }

    // Max concurrent = size of largest wave, capped at 8
    let max_concurrent = waves.iter().map(|w| w.len()).max().unwrap_or(1).min(8);

    ExecutionPlan { waves, max_concurrent }
}

fn priority_val(p: &str) -> usize {
    match p.to_lowercase().as_str() {
        "high" => 0, "medium" => 1, "low" => 2, _ => 1,
    }
}

fn sort_deque_by_priority(q: &mut VecDeque<usize>, tasks: &[DagTask]) {
    let mut v: Vec<usize> = q.drain(..).collect();
    v.sort_by_key(|&i| priority_val(&tasks[i].priority));
    q.extend(v);
}

// ── DAG Builder ──

/// Build DAG from planner output.
/// Uses explicit dependencies from planner + file-based inference as fallback.
pub fn build_dag(tasks: &[(String, String, String, Vec<String>, Vec<String>)]) -> Vec<DagTask> {
    // temp: (role, title, desc, deps, files)
    let mut file_owners: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, (_, _, _, _, files)) in tasks.iter().enumerate() {
        for f in files { file_owners.entry(f.as_str()).or_default().push(i); }
    }

    tasks.iter().enumerate().map(|(i, (role, title, desc, deps, files))| {
        let mut all_deps = deps.clone();

        // File-based: if this task references files owned by an earlier task
        if all_deps.is_empty() {
            for f in files {
                if let Some(owners) = file_owners.get(f.as_str()) {
                    for &owner in owners {
                        if owner < i && !all_deps.contains(&tasks[owner].1) {
                            all_deps.push(tasks[owner].1.clone());
                        }
                    }
                }
            }
        }

        // Priority from description keyword analysis
        let priority = priority_from_desc(desc);

        DagTask {
            id: format!("task-{}", i + 1),
            role: role.clone(),
            title: title.clone(),
            description: desc.clone(),
            depends_on: all_deps,
            priority,
            files: files.clone(),
        }
    }).collect()
}

fn priority_from_desc(desc: &str) -> String {
    let dl = desc.to_lowercase();
    if dl.contains("critical") || dl.contains("urgent") || dl.contains("must have") {
        "high".into()
    } else if dl.contains("nice to have") || dl.contains("optional") || dl.contains("later") {
        "low".into()
    } else {
        "medium".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_waves() {
        let t = vec![
            DagTask { id: "1".into(), role: "Architect".into(), title: "Design".into(), description: "".into(), depends_on: vec![], priority: "high".into(), files: vec![] },
            DagTask { id: "2".into(), role: "Developer".into(), title: "Implement".into(), description: "".into(), depends_on: vec!["Design".into()], priority: "high".into(), files: vec![] },
        ];
        let plan = compute_waves(&t);
        assert_eq!(plan.waves.len(), 2);
    }

    #[test]
    fn test_max_concurrent() {
        let t = vec![
            DagTask { id: "1".into(), role: "dev".into(), title: "A".into(), description: "".into(), depends_on: vec![], priority: "high".into(), files: vec![] },
            DagTask { id: "2".into(), role: "dev".into(), title: "B".into(), description: "".into(), depends_on: vec![], priority: "medium".into(), files: vec![] },
            DagTask { id: "3".into(), role: "dev".into(), title: "C".into(), description: "".into(), depends_on: vec![], priority: "high".into(), files: vec![] },
        ];
        let plan = compute_waves(&t);
        assert_eq!(plan.max_concurrent, 3);
    }
}
