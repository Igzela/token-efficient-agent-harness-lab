use std::collections::HashSet;

/// Common graph operations interface extracted from `DAGManager` and `DependencyResolver`.
///
/// Both modules provide validation, topological ordering, and ready-node queries
/// over different graph representations (`DAGState` vs `WorkflowGraph`). This trait
/// unifies that interface so downstream consumers can be generic over the graph type.
pub trait GraphOperations {
    /// Validate the graph structure (dangling edges, cycles, etc.).
    /// Returns `(is_valid, errors)` where `errors` is empty on success.
    fn validate(&self) -> (bool, Vec<String>);

    /// Return a topologically sorted list of node IDs (Kahn's algorithm).
    /// Returns empty if the graph is invalid.
    fn topological_order(&self) -> Vec<String>;

    /// Return node IDs whose all predecessors are in `completed`.
    /// Results are sorted for deterministic output.
    fn ready_nodes(&self, completed: &HashSet<String>) -> Vec<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockGraph {
        valid: bool,
        topo: Vec<String>,
        ready: Vec<String>,
    }

    impl GraphOperations for MockGraph {
        fn validate(&self) -> (bool, Vec<String>) {
            if self.valid {
                (true, vec![])
            } else {
                (false, vec!["mock_error".to_string()])
            }
        }

        fn topological_order(&self) -> Vec<String> {
            self.topo.clone()
        }

        fn ready_nodes(&self, _completed: &HashSet<String>) -> Vec<String> {
            self.ready.clone()
        }
    }

    #[test]
    fn mock_graph_valid() {
        let g = MockGraph {
            valid: true,
            topo: vec!["a".into(), "b".into()],
            ready: vec!["a".into()],
        };
        let (ok, errs) = g.validate();
        assert!(ok);
        assert!(errs.is_empty());
        assert_eq!(g.topological_order(), vec!["a", "b"]);
    }

    #[test]
    fn mock_graph_invalid() {
        let g = MockGraph {
            valid: false,
            topo: vec![],
            ready: vec![],
        };
        let (ok, errs) = g.validate();
        assert!(!ok);
        assert_eq!(errs.len(), 1);
        assert!(g.topological_order().is_empty());
    }

    #[test]
    fn trait_works_as_dyn_dispatch() {
        let g: Box<dyn GraphOperations> = Box::new(MockGraph {
            valid: true,
            topo: vec!["x".into()],
            ready: vec![],
        });
        let (ok, _) = g.validate();
        assert!(ok);
    }
}
