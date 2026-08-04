use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteeringMessage {
    pub id: Uuid,
    pub content: String,
}

impl SteeringMessage {
    pub fn new(content: impl Into<String>) -> Self {
        Self { id: Uuid::new_v4(), content: content.into() }
    }
}

type InterruptAction = Box<dyn Fn() + Send + Sync>;

/// A per-run control plane for low-overhead steering. Messages are queued while
/// a native tool is active and interrupt only the current provider request.
pub struct AgentRunControl {
    maximum_queued_messages: usize,
    state: Mutex<State>,
}

struct State {
    queued: Vec<SteeringMessage>,
    provider_interrupt: Option<(Uuid, InterruptAction)>,
}

impl Default for AgentRunControl {
    fn default() -> Self {
        Self::new(16)
    }
}

impl AgentRunControl {
    pub fn new(maximum_queued_messages: usize) -> Self {
        Self {
            maximum_queued_messages: maximum_queued_messages.max(1),
            state: Mutex::new(State { queued: Vec::new(), provider_interrupt: None }),
        }
    }

    /// Interrupt actions abort or cancel the in-flight provider request; they
    /// never re-enter this control, so invoking them under the lock is safe.
    pub async fn submit(&self, raw_message: &str) -> bool {
        let message = raw_message.trim();
        let mut state = self.state.lock().await;
        if message.is_empty() || state.queued.len() >= self.maximum_queued_messages {
            return false;
        }
        state.queued.push(SteeringMessage::new(message));
        if let Some((_, action)) = &state.provider_interrupt {
            action();
        }
        true
    }

    pub async fn drain(&self) -> Vec<SteeringMessage> {
        let mut state = self.state.lock().await;
        std::mem::take(&mut state.queued)
    }

    pub async fn install_provider_interrupt<F: Fn() + Send + Sync + 'static>(&self, token: Uuid, action: F) {
        let mut state = self.state.lock().await;
        state.provider_interrupt = Some((token, Box::new(action)));
        if !state.queued.is_empty() {
            if let Some((_, action)) = &state.provider_interrupt {
                action();
            }
        }
    }

    pub async fn clear_provider_interrupt(&self, token: Uuid) {
        let mut state = self.state.lock().await;
        if matches!(&state.provider_interrupt, Some((installed, _)) if *installed == token) {
            state.provider_interrupt = None;
        }
    }
}
