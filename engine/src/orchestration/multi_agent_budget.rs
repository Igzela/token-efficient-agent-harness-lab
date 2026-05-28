use std::collections::HashMap;

#[derive(Debug, Default)]
struct WorkflowBudget {
    total_limit: f64,
    consumed: f64,
    agent_limits: HashMap<String, f64>,
    agent_consumed: HashMap<String, f64>,
    node_reservations: HashMap<String, f64>,
}

pub struct MultiAgentBudgetManager {
    budgets: HashMap<String, WorkflowBudget>,
    pub overrun_strategy: String,
}

impl MultiAgentBudgetManager {
    pub fn new(overrun_strategy: &str) -> Self {
        Self {
            budgets: HashMap::new(),
            overrun_strategy: overrun_strategy.to_string(),
        }
    }

    pub fn create_workflow_budget(&mut self, workflow_id: &str, total_limit: f64) -> String {
        self.budgets.insert(
            workflow_id.to_string(),
            WorkflowBudget {
                total_limit,
                ..Default::default()
            },
        );
        workflow_id.to_string()
    }

    pub fn reserve_node_budget(
        &mut self,
        workflow_id: &str,
        node_id: &str,
        agent_id: &str,
        estimated_cost: f64,
    ) -> bool {
        let budget = match self.budgets.get_mut(workflow_id) {
            Some(b) => b,
            None => return false,
        };

        if budget.consumed + estimated_cost > budget.total_limit {
            return false;
        }

        let agent_limit = budget
            .agent_limits
            .get(agent_id)
            .copied()
            .unwrap_or(f64::INFINITY);
        let agent_consumed = budget.agent_consumed.get(agent_id).copied().unwrap_or(0.0);
        if agent_consumed + estimated_cost > agent_limit {
            return false;
        }

        budget
            .node_reservations
            .insert(node_id.to_string(), estimated_cost);
        true
    }

    pub fn record_cost(&mut self, workflow_id: &str, _node_id: &str, agent_id: &str, cost: f64) {
        let budget = match self.budgets.get_mut(workflow_id) {
            Some(b) => b,
            None => return,
        };
        budget.consumed += cost;
        *budget
            .agent_consumed
            .entry(agent_id.to_string())
            .or_insert(0.0) += cost;
    }

    pub fn check_workflow_budget(&self, workflow_id: &str) -> (bool, Option<String>) {
        let budget = match self.budgets.get(workflow_id) {
            Some(b) => b,
            None => return (true, None),
        };
        if budget.consumed > budget.total_limit {
            (
                false,
                Some(format!(
                    "workflow_budget_exceeded:{:.4}/{:.4}",
                    budget.consumed, budget.total_limit
                )),
            )
        } else {
            (true, None)
        }
    }

    pub fn check_agent_budget(&self, workflow_id: &str, agent_id: &str) -> (bool, Option<String>) {
        let budget = match self.budgets.get(workflow_id) {
            Some(b) => b,
            None => return (true, None),
        };
        let limit = budget
            .agent_limits
            .get(agent_id)
            .copied()
            .unwrap_or(f64::INFINITY);
        let consumed = budget.agent_consumed.get(agent_id).copied().unwrap_or(0.0);
        if consumed > limit {
            (
                false,
                Some(format!(
                    "agent_budget_exceeded:{agent_id}:{consumed:.4}/{limit:.4}"
                )),
            )
        } else {
            (true, None)
        }
    }

    pub fn set_agent_limit(&mut self, workflow_id: &str, agent_id: &str, limit: f64) {
        if let Some(budget) = self.budgets.get_mut(workflow_id) {
            budget.agent_limits.insert(agent_id.to_string(), limit);
        }
    }

    pub fn get_workflow_cost(&self, workflow_id: &str) -> f64 {
        self.budgets
            .get(workflow_id)
            .map(|b| b.consumed)
            .unwrap_or(0.0)
    }

    pub fn get_agent_cost(&self, workflow_id: &str, agent_id: &str) -> f64 {
        self.budgets
            .get(workflow_id)
            .and_then(|b| b.agent_consumed.get(agent_id).copied())
            .unwrap_or(0.0)
    }
}
