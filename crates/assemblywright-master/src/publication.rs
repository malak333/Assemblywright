use crate::{
    current_time_ms, MasterError, MasterKernel, PublicationActionEvidence, PublicationActionKind,
    PublicationAuthorization, PublicationExecutionPlan,
};
use assemblywright_protocol::{
    FeatureConveyorPublicationReceipt, FeatureConveyorPublicationRequest,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const PUBLICATION_ACTION_DEADLINE: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct PublicationExecutionControl {
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
    authority_current: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl PublicationExecutionControl {
    pub fn new(
        cancelled: Arc<AtomicBool>,
        deadline: Instant,
        authority_current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        Self {
            cancelled,
            deadline,
            authority_current,
        }
    }

    pub fn poll(&self) -> Result<(), PublicationAdapterError> {
        if self.cancelled.load(Ordering::Acquire) || !(self.authority_current)() {
            return Err(PublicationAdapterError::Cancelled);
        }
        if Instant::now() >= self.deadline {
            return Err(PublicationAdapterError::DeadlineExceeded);
        }
        Ok(())
    }
}

/// Fixed, credential-owning publication transport boundary. Implementations
/// derive operations from the master plan; they never accept caller commands,
/// paths, credentials, or raw output.
pub trait PublicationAdapter {
    fn is_available(&self) -> bool;

    fn execute(
        &mut self,
        plan: &PublicationExecutionPlan,
        action: PublicationActionKind,
        control: &PublicationExecutionControl,
    ) -> Result<PublicationActionEvidence, PublicationAdapterError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicationAdapterError {
    Unavailable,
    MissingEvidence,
    AmbiguousEffect,
    Cancelled,
    DeadlineExceeded,
}

pub fn run_publication<A: PublicationAdapter>(
    kernel: &mut MasterKernel,
    request: &FeatureConveyorPublicationRequest,
    adapter: &mut A,
    control: PublicationExecutionControl,
) -> Result<FeatureConveyorPublicationReceipt, MasterError> {
    let now_ms = current_time_ms()?;
    let plan = match kernel.prepare_publication(request, now_ms)? {
        PublicationAuthorization::Existing(receipt) => return Ok(*receipt),
        PublicationAuthorization::Planned(plan) => *plan,
    };
    if !adapter.is_available() {
        return Err(MasterError::PublicationCoordinatorUnavailable);
    }
    kernel.begin_publication(&plan, now_ms)?;
    for action in PublicationActionKind::ORDERED {
        if control.poll().is_err() {
            kernel.quarantine_ambiguous_publication(&plan, action, current_time_ms()?)?;
            return Err(MasterError::PublicationEffectAmbiguous);
        }
        let before_ms = current_time_ms()?;
        if !kernel.publication_execution_is_current(request, action, before_ms)? {
            kernel.quarantine_ambiguous_publication(&plan, action, before_ms)?;
            return Err(MasterError::PublicationEffectAmbiguous);
        }
        let evidence = match adapter.execute(&plan, action, &control) {
            Ok(evidence) => evidence,
            Err(_) => {
                kernel.quarantine_ambiguous_publication(&plan, action, current_time_ms()?)?;
                return Err(MasterError::PublicationEffectAmbiguous);
            }
        };
        if control.poll().is_err() {
            kernel.quarantine_ambiguous_publication(&plan, action, current_time_ms()?)?;
            return Err(MasterError::PublicationEffectAmbiguous);
        }
        let after_ms = current_time_ms()?;
        if !kernel.publication_execution_is_current(request, action, after_ms)? {
            kernel.quarantine_ambiguous_publication(&plan, action, after_ms)?;
            return Err(MasterError::PublicationEffectAmbiguous);
        }
        match kernel.complete_publication_action(&plan, &evidence, after_ms) {
            Ok(Some(receipt)) => {
                receipt.validate()?;
                return Ok(receipt);
            }
            Ok(None) => {}
            Err(_) => {
                kernel.quarantine_ambiguous_publication(&plan, action, after_ms)?;
                return Err(MasterError::PublicationEffectAmbiguous);
            }
        }
    }
    Err(MasterError::PublicationCoordinatorUnavailable)
}
