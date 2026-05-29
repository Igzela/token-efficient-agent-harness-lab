use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CostGateConfig {
    pub per_dispatch_cap_usd: Option<f64>,
    pub daily_cap_usd: Option<f64>,
}

impl CostGateConfig {
    pub fn from_env() -> Self {
        Self {
            per_dispatch_cap_usd: read_optional_f64("ACP_COST_PER_DISPATCH_USD"),
            daily_cap_usd: read_optional_f64("ACP_COST_DAILY_USD"),
        }
    }

    pub fn new(per_dispatch_cap_usd: Option<f64>, daily_cap_usd: Option<f64>) -> Self {
        Self {
            per_dispatch_cap_usd,
            daily_cap_usd,
        }
    }

    pub fn is_active(&self) -> bool {
        self.per_dispatch_cap_usd.is_some() || self.daily_cap_usd.is_some()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CostGateBlock {
    PerDispatchExceeded { cap: f64, reserved: f64 },
    DailyExceeded { cap: f64, today_total: f64 },
}

impl std::fmt::Display for CostGateBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CostGateBlock::PerDispatchExceeded { cap, reserved } => {
                write!(
                    f,
                    "per-dispatch cost cap ${:.4} exceeded by reservation ${:.4}",
                    cap, reserved
                )
            }
            CostGateBlock::DailyExceeded { cap, today_total } => {
                write!(
                    f,
                    "daily cost cap ${:.2} exceeded by today's total ${:.4}",
                    cap, today_total
                )
            }
        }
    }
}

pub fn check_cost_gates(
    config: &CostGateConfig,
    reserved_cost: f64,
    daily_cost_usd: f64,
) -> Result<(), CostGateBlock> {
    if let Some(cap) = config.per_dispatch_cap_usd {
        if reserved_cost > cap {
            return Err(CostGateBlock::PerDispatchExceeded {
                cap,
                reserved: reserved_cost,
            });
        }
    }
    if let Some(cap) = config.daily_cap_usd {
        if daily_cost_usd + reserved_cost > cap {
            return Err(CostGateBlock::DailyExceeded {
                cap,
                today_total: daily_cost_usd,
            });
        }
    }
    Ok(())
}

fn read_optional_f64(key: &str) -> Option<f64> {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| *v > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_caps_always_pass() {
        let config = CostGateConfig::new(None, None);
        assert_eq!(check_cost_gates(&config, 100.0, 500.0), Ok(()));
    }

    #[test]
    fn per_dispatch_within_cap() {
        let config = CostGateConfig::new(Some(1.0), None);
        assert_eq!(check_cost_gates(&config, 0.5, 0.0), Ok(()));
    }

    #[test]
    fn per_dispatch_exceeds_cap() {
        let config = CostGateConfig::new(Some(1.0), None);
        let result = check_cost_gates(&config, 1.5, 0.0);
        assert_eq!(
            result,
            Err(CostGateBlock::PerDispatchExceeded {
                cap: 1.0,
                reserved: 1.5,
            })
        );
    }

    #[test]
    fn daily_within_cap() {
        let config = CostGateConfig::new(None, Some(10.0));
        assert_eq!(check_cost_gates(&config, 3.0, 5.0), Ok(()));
    }

    #[test]
    fn daily_exceeds_cap() {
        let config = CostGateConfig::new(None, Some(10.0));
        let result = check_cost_gates(&config, 3.0, 8.0);
        assert_eq!(
            result,
            Err(CostGateBlock::DailyExceeded {
                cap: 10.0,
                today_total: 8.0,
            })
        );
    }

    #[test]
    fn daily_exact_at_cap_passes() {
        let config = CostGateConfig::new(None, Some(10.0));
        assert_eq!(check_cost_gates(&config, 2.0, 8.0), Ok(()));
    }

    #[test]
    fn both_caps_per_dispatch_fails_first() {
        let config = CostGateConfig::new(Some(1.0), Some(10.0));
        let result = check_cost_gates(&config, 1.5, 0.0);
        assert!(matches!(
            result,
            Err(CostGateBlock::PerDispatchExceeded { .. })
        ));
    }

    #[test]
    fn both_caps_daily_fails() {
        let config = CostGateConfig::new(Some(5.0), Some(10.0));
        let result = check_cost_gates(&config, 3.0, 8.0);
        assert!(matches!(result, Err(CostGateBlock::DailyExceeded { .. })));
    }

    #[test]
    fn is_active_true_when_either_set() {
        assert!(CostGateConfig::new(Some(1.0), None).is_active());
        assert!(CostGateConfig::new(None, Some(1.0)).is_active());
        assert!(CostGateConfig::new(Some(1.0), Some(1.0)).is_active());
        assert!(!CostGateConfig::new(None, None).is_active());
    }

    #[test]
    fn block_display_messages() {
        let pd = CostGateBlock::PerDispatchExceeded {
            cap: 1.0,
            reserved: 1.5,
        };
        assert!(pd.to_string().contains("per-dispatch"));
        let daily = CostGateBlock::DailyExceeded {
            cap: 10.0,
            today_total: 8.0,
        };
        assert!(daily.to_string().contains("daily"));
    }
}
