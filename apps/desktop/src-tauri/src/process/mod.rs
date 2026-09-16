mod supervisor;

#[cfg(test)]
mod tests;

pub(crate) use supervisor::{MachineEventReceiver, OwnedProcessGeneration};
pub use supervisor::{ProcessKind, ProcessPhase, ProcessSnapshot, ProcessSupervisor};
