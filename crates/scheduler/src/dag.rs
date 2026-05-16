use std::collections::{HashMap, HashSet};
use uuid::Uuid;
use openforce_domain::task::TaskState;

#[derive(Debug, Clone)]
pub struct DagNode { pub task_id: Uuid, pub upstream: HashSet<Uuid>, pub downstream: HashSet<Uuid> }

pub struct Dag { nodes: HashMap<Uuid, DagNode> }

impl Dag {
    pub fn new() -> Self { Self { nodes: HashMap::new() } }
    pub fn add_node(&mut self, tid: Uuid, upstream: Vec<Uuid>) {
        let entry = self.nodes.entry(tid).or_insert_with(|| DagNode { task_id: tid, upstream: HashSet::new(), downstream: HashSet::new() });
        let up_copy: Vec<Uuid> = upstream.iter().copied().collect();
        for u in &up_copy { entry.upstream.insert(*u); }
        for u in &up_copy {
            self.nodes.entry(*u).or_insert_with(|| DagNode { task_id: *u, upstream: HashSet::new(), downstream: HashSet::new() }).downstream.insert(tid);
        }
    }
    pub fn dependencies_satisfied(&self, tid: Uuid, completed: &HashSet<Uuid>) -> bool {
        self.nodes.get(&tid).map_or(true, |n| n.upstream.iter().all(|u| completed.contains(u)))
    }
    pub fn ready_tasks(&self, states: &HashMap<Uuid, TaskState>) -> Vec<Uuid> {
        let completed: HashSet<Uuid> = states.iter().filter(|(_, s)| s.is_terminal()).map(|(id, _)| *id).collect();
        states.iter().filter(|(_, s)| **s == TaskState::Pending).filter(|(id, _)| self.dependencies_satisfied(**id, &completed)).map(|(id, _)| *id).collect()
    }
}
