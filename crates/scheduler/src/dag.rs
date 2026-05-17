use std::collections::{HashMap, HashSet};
use uuid::Uuid;
use openforce_domain::task::TaskState;

#[derive(Debug, Clone)]
pub struct DagNode {
    pub task_id: Uuid,
    pub upstream: HashSet<Uuid>,
    pub downstream: HashSet<Uuid>,
}

pub struct Dag { nodes: HashMap<Uuid, DagNode> }

impl Dag {
    pub fn new() -> Self { Self { nodes: HashMap::new() } }

    pub fn add_node(&mut self, tid: Uuid, upstream: Vec<Uuid>) {
        let entry = self.nodes.entry(tid).or_insert_with(|| DagNode {
            task_id: tid, upstream: HashSet::new(), downstream: HashSet::new(),
        });
        let up_copy: Vec<Uuid> = upstream.iter().copied().collect();
        for u in &up_copy { entry.upstream.insert(*u); }
        for u in &up_copy {
            self.nodes.entry(*u).or_insert_with(|| DagNode {
                task_id: *u, upstream: HashSet::new(), downstream: HashSet::new(),
            }).downstream.insert(tid);
        }
    }

    /// Only Succeeded unblocks downstream. Failed/Cancelled keep downstream blocked.
    pub fn dependencies_satisfied(&self, tid: Uuid, states: &HashMap<Uuid, TaskState>) -> bool {
        self.nodes.get(&tid).map_or(true, |n| {
            n.upstream.iter().all(|u| states.get(u).map_or(false, |s| *s == TaskState::Succeeded))
        })
    }

    /// Pending tasks whose upstream deps are all Succeeded.
    pub fn ready_tasks(&self, states: &HashMap<Uuid, TaskState>) -> Vec<Uuid> {
        states.iter()
            .filter(|(_, s)| **s == TaskState::Pending)
            .filter(|(id, _)| self.dependencies_satisfied(**id, states))
            .map(|(id, _)| *id).collect()
    }

    /// Pending tasks blocked by Failed upstream — need human intervention.
    pub fn blocked_tasks(&self, states: &HashMap<Uuid, TaskState>) -> Vec<Uuid> {
        let failed: HashSet<Uuid> = states.iter()
            .filter(|(_, s)| **s == TaskState::Failed).map(|(id, _)| *id).collect();
        states.iter()
            .filter(|(_, s)| **s == TaskState::Pending)
            .filter(|(id, _)| self.nodes.get(id).map_or(false, |n| n.upstream.iter().any(|u| failed.contains(u))))
            .map(|(id, _)| *id).collect()
    }

    pub fn downstream_of(&self, tid: Uuid) -> Vec<Uuid> {
        self.nodes.get(&tid).map(|n| n.downstream.iter().copied().collect()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_worker_waits_for_backend_and_frontend() {
        let mut dag = Dag::new();
        let arch = Uuid::now_v7();
        let backend = Uuid::now_v7();
        let frontend = Uuid::now_v7();
        let testing = Uuid::now_v7();

        // 图书管理系统: 架构→(后端+前端)→测试
        dag.add_node(backend, vec![arch]);
        dag.add_node(frontend, vec![arch]);
        dag.add_node(testing, vec![backend, frontend]);

        // Phase 1: 只有架构 ready
        let s1 = HashMap::from([
            (arch, TaskState::Pending), (backend, TaskState::Pending),
            (frontend, TaskState::Pending), (testing, TaskState::Pending),
        ]);
        assert_eq!(dag.ready_tasks(&s1), vec![arch]);

        // Phase 2: 架构完成 → 后端+前端 ready，测试仍 pending
        let mut s2 = HashMap::new();
        s2.insert(arch, TaskState::Succeeded);
        s2.insert(backend, TaskState::Pending);
        s2.insert(frontend, TaskState::Pending);
        s2.insert(testing, TaskState::Pending);
        let r2 = dag.ready_tasks(&s2);
        assert_eq!(r2.len(), 2);
        assert!(r2.contains(&backend) && r2.contains(&frontend));
        assert!(!r2.contains(&testing), "测试不应 ready — 后端前端都还没完成");

        // Phase 3: 后端完成，前端还在跑 → 测试仍 pending
        let mut s3 = HashMap::new();
        s3.insert(arch, TaskState::Succeeded);
        s3.insert(backend, TaskState::Succeeded);
        s3.insert(frontend, TaskState::Pending);
        s3.insert(testing, TaskState::Pending);
        assert!(!dag.ready_tasks(&s3).contains(&testing));

        // Phase 4: 全部完成 → 测试 ready
        s3.insert(frontend, TaskState::Succeeded);
        assert!(dag.ready_tasks(&s3).contains(&testing));
    }

    #[test]
    fn test_upstream_failed_blocks_downstream() {
        let mut dag = Dag::new();
        let t1 = Uuid::now_v7();
        let t2 = Uuid::now_v7();
        dag.add_node(t2, vec![t1]);
        let states = HashMap::from([(t1, TaskState::Failed), (t2, TaskState::Pending)]);
        assert_eq!(dag.ready_tasks(&states).len(), 0);
        assert_eq!(dag.blocked_tasks(&states).len(), 1);
    }
}
