use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone)]
pub struct DagTask {
    pub id: String,
    pub role: String,
    pub title: String,
    pub description: String,
    pub depends_on: Vec<String>,
}

/// Topological sort into parallel waves.
/// Wave 0: no deps → all parallel. Wave N: deps satisfied by earlier waves.
pub fn compute_waves(tasks: &[DagTask]) -> Vec<Vec<usize>> {
    let n = tasks.len();
    let name_to_idx: HashMap<&str, usize> = tasks.iter().enumerate()
        .map(|(i, t)| (t.title.as_str(), i)).collect();
    let mut graph: Vec<Vec<usize>> = vec![vec![]; n];
    let mut in_degree = vec![0usize; n];

    for (i, task) in tasks.iter().enumerate() {
        for dep in &task.depends_on {
            if let Some(&di) = name_to_idx.get(dep.as_str()) {
                graph[di].push(i); in_degree[i] += 1;
            }
        }
    }
    let mut waves = vec![];
    let mut q: VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
    while !q.is_empty() {
        let wave: Vec<usize> = q.drain(..).collect();
        for &node in &wave {
            for &next in &graph[node] { in_degree[next] -= 1; if in_degree[next] == 0 { q.push_back(next); } }
        }
        waves.push(wave);
    }
    let rem: Vec<usize> = (0..n).filter(|i| !waves.iter().any(|w| w.contains(i))).collect();
    if !rem.is_empty() { tracing::warn!("circular deps: {} tasks in final wave", rem.len()); waves.push(rem); }
    waves
}

/// Build DAG tasks from RoundTable output, using explicit dependencies when available.
pub fn infer_dependencies(tasks: &[(String, String, String, Vec<String>)]) -> Vec<DagTask> {
    tasks.iter().enumerate().map(|(i, (role, title, desc, deps))| {
        DagTask {
            id: format!("task-{}", i+1),
            role: role.clone(),
            title: title.clone(),
            description: desc.clone(),
            depends_on: if deps.is_empty() {
                // Fallback heuristic: test/verify/integration tasks depend on earlier tasks
                let dl = desc.to_lowercase();
                let kw = ["验证", "测试", "test", "verify", "合并", "综合", "覆盖", "最终"];
                if i > 0 && kw.iter().any(|k| dl.contains(k)) {
                    tasks[..i].iter().map(|(_, t, _, _)| t.clone()).collect()
                } else { vec![] }
            } else { deps.clone() },
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_single_wave() {
        let t = vec![DagTask{id:"1".into(),role:"a".into(),title:"A".into(),description:"".into(),depends_on:vec![]},DagTask{id:"2".into(),role:"b".into(),title:"B".into(),description:"".into(),depends_on:vec![]}];
        let w = compute_waves(&t); assert_eq!(w.len(),1); assert_eq!(w[0].len(),2);
    }
    #[test] fn test_two_waves() {
        let t = vec![DagTask{id:"1".into(),role:"a".into(),title:"A".into(),description:"".into(),depends_on:vec![]},DagTask{id:"2".into(),role:"b".into(),title:"B".into(),description:"".into(),depends_on:vec!["A".into()]}];
        let w = compute_waves(&t); assert_eq!(w.len(),2);
    }
}
