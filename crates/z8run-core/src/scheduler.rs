//! Flow engine scheduler.
//!
//! Determines execution order respecting dependencies
//! and maximizing parallelism between independent nodes.

use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

use crate::flow::Flow;
use crate::error::Z8Result;

/// An execution step that can contain multiple nodes in parallel.
#[derive(Debug, Clone)]
pub struct ExecutionStep {
    /// Nodes that can be executed in parallel in this step.
    pub node_ids: Vec<Uuid>,
    /// Step number (0-indexed).
    pub step: usize,
}

/// Compiled execution plan: sequence of steps with maximized parallelism.
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    /// Ordered execution steps.
    pub steps: Vec<ExecutionStep>,
    /// Total nodes to execute.
    pub total_nodes: usize,
}

impl ExecutionPlan {
    /// Compiles an execution plan from a flow.
    /// Groups nodes into steps where each step contains nodes
    /// that can execute in parallel (with no dependencies between them).
    pub fn compile(flow: &Flow) -> Z8Result<Self> {
        // Validate that it is a DAG
        flow.validate_acyclic()?;

        let enabled_nodes: HashSet<Uuid> = flow
            .nodes
            .iter()
            .filter(|n| n.enabled)
            .map(|n| n.id)
            .collect();

        // Calculate in-degree only for enabled nodes
        let mut in_degree: HashMap<Uuid, usize> =
            enabled_nodes.iter().map(|&id| (id, 0)).collect();

        for edge in &flow.edges {
            if enabled_nodes.contains(&edge.from_node) && enabled_nodes.contains(&edge.to_node) {
                *in_degree.entry(edge.to_node).or_insert(0) += 1;
            }
        }

        let mut steps = Vec::new();
        let mut queue: VecDeque<Uuid> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut step_num = 0;

        while !queue.is_empty() {
            // All nodes in the current queue can execute in parallel
            let current_batch: Vec<Uuid> = queue.drain(..).collect();

            steps.push(ExecutionStep {
                node_ids: current_batch.clone(),
                step: step_num,
            });

            // Reduce in-degree of successors
            for node_id in &current_batch {
                for edge in flow.outgoing_edges(*node_id) {
                    if let Some(deg) = in_degree.get_mut(&edge.to_node) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(edge.to_node);
                        }
                    }
                }
            }

            step_num += 1;
        }

        let total_nodes = steps.iter().map(|s| s.node_ids.len()).sum();

        Ok(Self { steps, total_nodes })
    }

    /// Returns the maximum degree of parallelism of the plan.
    pub fn max_parallelism(&self) -> usize {
        self.steps.iter().map(|s| s.node_ids.len()).max().unwrap_or(0)
    }

    /// Returns the depth (number of sequential steps).
    pub fn depth(&self) -> usize {
        self.steps.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{Node, PortType};
    use crate::flow::Flow;

    #[test]
    fn test_parallel_execution_plan() {
        let mut flow = Flow::new("Parallel Test");

        // Trigger -> [A, B] -> Merge
        let trigger = Node::new("Trigger", "trigger")
            .with_output("out", PortType::Any);
        let a = Node::new("A", "process")
            .with_input("in", PortType::Any)
            .with_output("out", PortType::Any);
        let b = Node::new("B", "process")
            .with_input("in", PortType::Any)
            .with_output("out", PortType::Any);
        let merge = Node::new("Merge", "merge")
            .with_input("in", PortType::Any);

        let t_id = trigger.id;
        let a_id = a.id;
        let b_id = b.id;
        let m_id = merge.id;

        flow.add_node(trigger);
        flow.add_node(a);
        flow.add_node(b);
        flow.add_node(merge);

        flow.connect(t_id, "out", a_id, "in").unwrap();
        flow.connect(t_id, "out", b_id, "in").unwrap();
        flow.connect(a_id, "out", m_id, "in").unwrap();
        flow.connect(b_id, "out", m_id, "in").unwrap();

        let plan = ExecutionPlan::compile(&flow).unwrap();

        assert_eq!(plan.depth(), 3); // Trigger -> [A,B] -> Merge
        assert_eq!(plan.max_parallelism(), 2); // A and B in parallel
        assert_eq!(plan.total_nodes, 4);
    }

    #[test]
    fn test_linear_plan() {
        let mut flow = Flow::new("Linear");
        let a = Node::new("A", "a").with_output("out", PortType::Any);
        let b = Node::new("B", "b")
            .with_input("in", PortType::Any)
            .with_output("out", PortType::Any);
        let c = Node::new("C", "c").with_input("in", PortType::Any);

        let a_id = a.id;
        let b_id = b.id;
        let c_id = c.id;

        flow.add_node(a);
        flow.add_node(b);
        flow.add_node(c);

        flow.connect(a_id, "out", b_id, "in").unwrap();
        flow.connect(b_id, "out", c_id, "in").unwrap();

        let plan = ExecutionPlan::compile(&flow).unwrap();
        assert_eq!(plan.depth(), 3);
        assert_eq!(plan.max_parallelism(), 1);
    }
}
