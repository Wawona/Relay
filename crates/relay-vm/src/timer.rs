//! Deterministic virtual timer and single-vCPU interrupt controller.
use relay_core::RelayError;
use std::collections::BTreeSet;
pub(crate) struct Timer {
    ticks: u64,
    deadline: Option<u64>,
    irq: u32,
}
impl Timer {
    pub(crate) fn new(irq: u32) -> Self {
        Self {
            ticks: 0,
            deadline: None,
            irq,
        }
    }
    pub(crate) fn set_deadline(&mut self, ticks: u64) -> Result<(), RelayError> {
        if ticks <= self.ticks {
            return Err(RelayError::Failed(
                "timer deadline must be in the future".into(),
            ));
        }
        self.deadline = Some(ticks);
        Ok(())
    }
    pub(crate) fn advance(&mut self, instructions: u64, gic: &mut Gic) {
        self.ticks = self.ticks.saturating_add(instructions);
        if self.deadline.is_some_and(|d| self.ticks >= d) {
            gic.raise(self.irq);
            self.deadline = None
        }
    }
}
pub(crate) struct Gic {
    pending: BTreeSet<u32>,
}
impl Gic {
    pub(crate) fn new() -> Self {
        Self {
            pending: BTreeSet::new(),
        }
    }
    pub(crate) fn raise(&mut self, irq: u32) {
        self.pending.insert(irq);
    }
    pub(crate) fn acknowledge(&mut self) -> Option<u32> {
        let irq = *self.pending.iter().next()?;
        self.pending.remove(&irq);
        Some(irq)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timer_raises_deterministic_irq() {
        let mut g = Gic::new();
        let mut t = Timer::new(27);
        t.set_deadline(10).unwrap();
        t.advance(9, &mut g);
        assert_eq!(g.acknowledge(), None);
        t.advance(1, &mut g);
        assert_eq!(g.acknowledge(), Some(27));
    }
}
